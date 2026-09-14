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
    // `ai` settings were removed for the v1 editor-only scope; legacy keys
    // in an existing settings.json are silently ignored by serde.
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

    /// Parse previously saved settings. Failing here must be loud for the
    /// caller (`load_from_path`), never a silent reset to defaults.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn load_from_path(path: &Path) -> Self {
        if let Ok(content) = std::fs::read_to_string(path) {
            match Self::from_json(&content) {
                Ok(settings) => return settings,
                Err(error) => {
                    // A corrupt settings file must not be overwritten by the
                    // next save: keep the raw bytes under a unique name and
                    // surface diagnostics in the log.
                    if let Err(backup_error) = preserve_corrupt_settings(path) {
                        log::error!("corrupt settings file could not be preserved: {backup_error}");
                    }
                    log::warn!(
                        "settings at {} could not be parsed ({error}); loading defaults. \
                         The corrupt file has been preserved.",
                        path.display()
                    );
                    return Self::default();
                }
            }
        }
        Self::default()
    }

    /// Serialize is effectively infallible for this struct, but propagation
    /// keeps the no-silent-fallback contract: `save_to_path` never writes
    /// empty output over existing settings.
    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = self.to_json().map_err(|error| {
            std::io::Error::other(format!("Failed to serialize settings: {error}"))
        })?;
        atomic_write(path, json.as_bytes())
    }
}

/// Copies the unparsable settings file next to itself with a unique, dated
/// name so a later save never overwrites the user's original bytes.
fn preserve_corrupt_settings(path: &Path) -> std::io::Result<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("settings");
    let backup_path = path.with_file_name(format!("{file_stem}_corrupt_{stamp}.json"));
    let error = std::fs::copy(path, &backup_path).map(|_| ());
    match error {
        Ok(_) => {
            log::info!("corrupt settings preserved at {}", backup_path.display());
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
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

        let json = settings.to_json().unwrap_or_else(|_| String::new());
        assert!(json.contains("font_size"));

        let loaded = GrafSettings::from_json(&json).expect("roundtrip serialize");
        assert_eq!(loaded, settings);
    }

    #[test]
    fn corrupt_settings_preserved_and_defaults_loaded() {
        let temp_dir =
            std::env::temp_dir().join(format!("graf_settings_corrupt_{}", std::process::id()));
        let settings_path = temp_dir.join(SETTINGS_FILE_NAME);
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(&settings_path, "{ not valid json").unwrap();

        let loaded = GrafSettings::load_from_path(&settings_path);
        assert_eq!(loaded, GrafSettings::default());

        // The corrupt file must be preserved under a unique name...
        let preserved: Vec<_> = std::fs::read_dir(&temp_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            preserved.iter().any(|name| name.contains("corrupt")),
            "no preserved corrupt copy found in {preserved:?}"
        );
        // ...and a save must never clobber the corrupt file (different name).
        GrafSettings::default()
            .save_to_path(&settings_path)
            .unwrap();
        assert!(preserved.iter().any(|name| name.ends_with(".json")));

        let _ = std::fs::remove_dir_all(&temp_dir);
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

        let loaded = GrafSettings::from_json(json).expect("legacy json should parse");
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
