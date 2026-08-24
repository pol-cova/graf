//! Workspace configuration and persistent user preferences.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::persistence::atomic_write;

/// Root configuration settings for the Graf workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GrafSettings {
    #[serde(default)]
    pub editor: EditorSettings,
    #[serde(default)]
    pub ai: AiSettings,
    #[serde(default)]
    pub canvas: CanvasSettings,
    #[serde(default)]
    pub theme: ThemeSettings,
    #[serde(default)]
    pub layout: LayoutSettings,
}

impl GrafSettings {
    pub fn default_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Graf")
                .join("settings.json"),
        )
    }

    /// Loads settings from JSON string or falls back to defaults.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Serializes settings to pretty-printed JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Loads settings from a file on disk.
    pub fn load_from_path(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            Self::from_json(&content)
        } else {
            Self::default()
        }
    }

    /// Saves settings to a file on disk atomically.
    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        atomic_write(path, self.to_json().as_bytes())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// Editor display and behavior preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditorSettings {
    pub font_size: f32,
    pub tab_size: usize,
    pub line_numbers: bool,
    pub auto_compile_on_save: bool,
    pub compile_debounce_ms: u64,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            tab_size: 2,
            line_numbers: true,
            auto_compile_on_save: true,
            compile_debounce_ms: 300,
        }
    }
}

/// Agent Client Protocol (ACP) and AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiSettings {
    pub acp_command: String,
    pub acp_server_url: String,
    pub temperature: f32,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            acp_command: String::new(),
            acp_server_url: String::new(),
            temperature: 0.2,
        }
    }
}

/// Vector Canvas drawing preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasSettings {
    pub grid_enabled: bool,
    pub snap_to_grid: bool,
    pub default_stroke_color: String,
}

impl Default for CanvasSettings {
    fn default() -> Self {
        Self {
            grid_enabled: true,
            snap_to_grid: true,
            default_stroke_color: "#528bff".to_string(),
        }
    }
}

/// Visual theme preferences.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeSettings {
    pub theme_name: String,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            theme_name: "Zed Dark".to_string(),
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
        assert!(settings.canvas.grid_enabled);

        let json = settings.to_json();
        assert!(json.contains("font_size"));
        assert!(json.contains("Zed Dark"));

        let loaded = GrafSettings::from_json(&json);
        assert_eq!(loaded, settings);
    }

    #[test]
    fn test_settings_file_io() {
        let temp_dir =
            std::env::temp_dir().join(format!("graf_settings_test_{}", std::process::id()));
        let settings_path = temp_dir.join("settings.json");

        let mut settings = GrafSettings::default();
        settings.editor.font_size = 16.0;
        settings.save_to_path(&settings_path).unwrap();

        let loaded = GrafSettings::load_from_path(&settings_path);
        assert_eq!(loaded.editor.font_size, 16.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
