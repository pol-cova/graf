use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::persistence::atomic_write;

const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GrafSettings {
    #[serde(default)]
    pub editor: EditorSettings,
    // `layout` (sidebar_width, preview_width, diagnostics_height) was the
    // GPUI three-pane geometry. SwiftUI sizes the sidebar and preview itself,
    // so nothing read these; they are gone rather than left to be carried in
    // every user's settings.json forever.
    // `ai` went earlier for the v1 editor-only scope. Legacy keys in an
    // existing settings.json are silently ignored by serde, which is also
    // what happens to a stale `layout` block on the next save.
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
        match std::fs::read_to_string(path) {
            Ok(content) => match Self::from_json(&content) {
                Ok(settings) => settings,
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
                    Self::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A missing file on a first run is not an error.
                Self::default()
            }
            Err(error) => {
                // Losing a settings file the user actually owns (permissions,
                // transient FS errors) is a degradation: log it loudly in
                // addition to falling back to defaults.
                log::error!(
                    "could not read settings at {} ({error}); loading defaults",
                    path.display()
                );
                Self::default()
            }
        }
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

/// Copies the unparsable settings file next to itself with a unique name
/// (seconds + nanos, so two corrupt loads within one second cannot hit the
/// same backup name) so a later save never overwrites the user's original
/// bytes.
fn preserve_corrupt_settings(path: &Path) -> std::io::Result<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
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
pub struct EditorSettings {
    /// Spaces inserted by Tab in the editor, and the width ⌘⇥ and the
    /// Find/Replace indents use.
    pub tab_size: usize,
    #[serde(alias = "auto_compile_on_save")]
    pub auto_compile: bool,
    pub compile_debounce_ms: u64,
    /// Size of prose in the writing column. Markup scales with it.
    pub prose_font_size: f32,
    /// Whether new windows dim everything but the paragraph being written.
    pub focus_mode: bool,
    /// Whether citations include the local Zotero library export.
    pub use_zotero: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_size: 2,
            auto_compile: true,
            // Builds run when the writer pauses, not between keystrokes.
            compile_debounce_ms: 800,
            prose_font_size: 19.0,
            focus_mode: true,
            use_zotero: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_serialization_and_defaults() {
        let settings = GrafSettings::default();
        assert_eq!(settings.editor.tab_size, 2);
        assert!(settings.editor.auto_compile);
        assert_eq!(settings.editor.compile_debounce_ms, 800);

        let json = settings.to_json().unwrap_or_else(|_| String::new());
        assert!(json.contains("tab_size"));

        let loaded = GrafSettings::from_json(&json).expect("roundtrip serialize");
        assert_eq!(loaded, settings);
    }

    /// The GPUI settings that are gone must not break an existing file. serde
    /// ignores unknown keys, so a user's saved `layout`, `font_size`, and
    /// `line_numbers` load fine and simply stop coming back on the next save.
    #[test]
    fn ignores_removed_settings_in_an_existing_file() {
        let json = r#"{
            "editor": {
                "font_size": 15.0,
                "line_numbers": false,
                "tab_size": 4
            },
            "layout": {
                "sidebar_width": 300.0,
                "preview_width": 500.0,
                "diagnostics_height": 200.0
            },
            "ai": { "provider": "anthropic" }
        }"#;

        let loaded = GrafSettings::from_json(json).expect("legacy file still parses");

        assert_eq!(loaded.editor.tab_size, 4);
        // Everything the Swift app actually reads keeps its default.
        assert_eq!(loaded.editor.compile_debounce_ms, 800);
        assert_eq!(loaded.editor.prose_font_size, 19.0);
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
                "tab_size": 4,
                "auto_compile_on_save": false,
                "compile_debounce_ms": 500
            }
        }"#;

        let loaded = GrafSettings::from_json(json).expect("legacy json should parse");
        assert!(!loaded.editor.auto_compile);
        assert_eq!(loaded.editor.compile_debounce_ms, 500);
    }

    #[test]
    fn test_settings_file_io() {
        let temp_dir =
            std::env::temp_dir().join(format!("graf_settings_test_{}", std::process::id()));
        let settings_path = temp_dir.join(SETTINGS_FILE_NAME);

        let mut settings = GrafSettings::default();
        settings.editor.prose_font_size = 21.0;
        settings.save_to_path(&settings_path).unwrap();

        let loaded = GrafSettings::load_from_path(&settings_path);
        assert_eq!(loaded.editor.prose_font_size, 21.0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
