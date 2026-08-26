use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::persistence::atomic_write;

const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GrafSettings {
    #[serde(default)]
    pub editor: EditorSettings,
    #[serde(default)]
    pub layout: LayoutSettings,
    #[serde(default)]
    pub ai: AiSettings,
}

impl GrafSettings {
    pub fn default_path() -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME")?;
            Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("graf")
                    .join(SETTINGS_FILE_NAME),
            )
        }

        #[cfg(target_os = "windows")]
        {
            let app_data = std::env::var_os("APPDATA")?;
            Some(
                PathBuf::from(app_data)
                    .join("graf")
                    .join(SETTINGS_FILE_NAME),
            )
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
                return Some(
                    PathBuf::from(config_home)
                        .join("graf")
                        .join(SETTINGS_FILE_NAME),
                );
            }
            let home = std::env::var_os("HOME")?;
            Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("graf")
                    .join(SETTINGS_FILE_NAME),
            )
        }
    }

    pub fn load_default() -> Self {
        let Some(path) = Self::default_path() else {
            return Self::default();
        };
        if path.exists() {
            return Self::load_from_path(&path);
        }

        #[cfg(target_os = "macos")]
        {
            let legacy_path = path
                .parent()
                .and_then(Path::parent)
                .map(|application_support| application_support.join("Graf/settings.json"));
            legacy_path
                .filter(|legacy_path| legacy_path.exists())
                .map_or_else(Self::default, |legacy_path| {
                    Self::load_from_path(&legacy_path)
                })
        }

        #[cfg(not(target_os = "macos"))]
        Self::default()
    }

    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn load_from_path(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            Self::from_json(&content)
        } else {
            Self::default()
        }
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(path, self.to_json().as_bytes())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    #[default]
    Acp,
    OpenAiCompatible,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct AcpSettings {
    pub command: Option<PathBuf>,
    pub args: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct AiSettings {
    pub provider: AiProviderKind,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub acp: AcpSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LayoutSettings {
    pub sidebar_width: f32,
    pub preview_width: f32,
    pub diagnostics_height: f32,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            sidebar_width: 236.0,
            preview_width: 460.0,
            diagnostics_height: 180.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct EditorSettings {
    pub font_size: f32,
    pub tab_size: usize,
    pub line_numbers: bool,
    #[serde(alias = "auto_compile_on_save")]
    pub auto_compile: bool,
    pub compile_debounce_ms: u64,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            tab_size: 2,
            line_numbers: true,
            auto_compile: true,
            compile_debounce_ms: 300,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_serialization_and_defaults() {
        let settings = GrafSettings::default();
        assert_eq!(settings.editor.font_size, 14.0);
        assert_eq!(settings.editor.tab_size, 2);
        assert!(settings.editor.line_numbers);

        let json = settings.to_json();
        assert!(json.contains("font_size"));

        let loaded = GrafSettings::from_json(&json);
        assert_eq!(loaded, settings);
    }

    #[test]
    fn loads_legacy_auto_compile_key() {
        let json = r#"{
            "editor": {
                "font_size": 15.0,
                "tab_size": 4,
                "line_numbers": false,
                "auto_compile_on_save": false,
                "compile_debounce_ms": 500
            }
        }"#;

        let loaded = GrafSettings::from_json(json);
        assert!(!loaded.editor.auto_compile);
        assert_eq!(loaded.editor.font_size, 15.0);
        assert_eq!(loaded.layout, LayoutSettings::default());
    }

    #[test]
    fn test_settings_file_io() {
        let temp_dir =
            std::env::temp_dir().join(format!("graf_settings_test_{}", std::process::id()));
        let settings_path = temp_dir.join(SETTINGS_FILE_NAME);

        let mut settings = GrafSettings::default();
        settings.editor.font_size = 16.0;
        settings.save_to_path(&settings_path).unwrap();

        let loaded = GrafSettings::load_from_path(&settings_path);
        assert_eq!(loaded.editor.font_size, 16.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
