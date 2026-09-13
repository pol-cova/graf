use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::persistence::atomic_write;

const RECOVERY_FILE_NAME: &str = "session_recovery.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreTarget {
    Existing(PathBuf),
    Untitled(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub title: String,
    pub path: Option<PathBuf>,
    pub content: String,
    pub timestamp: u64,
}

impl RecoveryEntry {
    pub fn new(
        title: impl Into<String>,
        path: Option<PathBuf>,
        content: impl Into<String>,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            title: title.into(),
            path,
            content: content.into(),
            timestamp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RecoveryJournal {
    pub entries: Vec<RecoveryEntry>,
}

impl RecoveryJournal {
    pub fn new(entries: Vec<RecoveryEntry>) -> Self {
        Self { entries }
    }

    pub fn restore_target(entry: &RecoveryEntry) -> RestoreTarget {
        match &entry.path {
            Some(path) if path.is_file() => RestoreTarget::Existing(path.clone()),
            _ => RestoreTarget::Untitled(entry.title.clone()),
        }
    }

    pub fn to_json(&self) -> String {
        // Compact JSON: smaller journal, faster to write on every debounced
        // flush. Never pretty-print recovery data on the hot path.
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn try_to_compact_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    fn json_error_to_io(error: serde_json::Error) -> std::io::Error {
        std::io::Error::other(error.to_string())
    }

    pub fn save_to_dir(&self, dir: &Path) -> std::io::Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let file_path = dir.join(RECOVERY_FILE_NAME);
        let json = serde_json::to_string(self).map_err(Self::json_error_to_io)?;
        atomic_write(&file_path, json.as_bytes())?;
        Ok(file_path)
    }

    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let file_path = dir.join(RECOVERY_FILE_NAME);
        if file_path.exists() {
            let content = fs::read_to_string(&file_path).ok()?;
            Self::from_json(&content)
        } else {
            None
        }
    }

    pub fn clear_dir(dir: &Path) -> std::io::Result<()> {
        let file_path = dir.join(RECOVERY_FILE_NAME);
        if file_path.exists() {
            fs::remove_file(file_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_target_prefers_existing_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("paper.tex");
        std::fs::write(&path, "on disk").unwrap();

        let existing = RecoveryEntry::new("paper.tex", Some(path.clone()), "unsaved");
        let missing = RecoveryEntry::new("draft.tex", Some(temp.path().join("gone.tex")), "text");
        let untitled = RecoveryEntry::new("notes.typ", None, "= Notes");

        assert_eq!(
            RecoveryJournal::restore_target(&existing),
            RestoreTarget::Existing(path)
        );
        assert_eq!(
            RecoveryJournal::restore_target(&missing),
            RestoreTarget::Untitled("draft.tex".to_string())
        );
        assert_eq!(
            RecoveryJournal::restore_target(&untitled),
            RestoreTarget::Untitled("notes.typ".to_string())
        );
    }

    #[test]
    fn test_recovery_journal_serialization() {
        let entry1 = RecoveryEntry::new("main.tex", None, "\\documentclass{article}");
        let entry2 = RecoveryEntry::new(
            "notes.typ",
            Some(PathBuf::from("/tmp/notes.typ")),
            "= Title",
        );

        let journal = RecoveryJournal::new(vec![entry1.clone(), entry2.clone()]);
        let json = journal.to_json();

        let loaded =
            RecoveryJournal::from_json(&json).expect("Failed to deserialize recovery journal");
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].title, "main.tex");
        assert_eq!(loaded.entries[1].content, "= Title");
    }

    #[test]
    fn test_recovery_journal_disk_persistence() {
        let temp_dir =
            std::env::temp_dir().join(format!("graf_recovery_test_{}", std::process::id()));
        let entry = RecoveryEntry::new("draft.tex", None, "Unsaved text content");
        let journal = RecoveryJournal::new(vec![entry]);

        journal
            .save_to_dir(&temp_dir)
            .expect("Failed to save recovery journal");

        let loaded =
            RecoveryJournal::load_from_dir(&temp_dir).expect("Failed to load recovery journal");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].title, "draft.tex");

        RecoveryJournal::clear_dir(&temp_dir).expect("Failed to clear recovery journal");
        assert!(RecoveryJournal::load_from_dir(&temp_dir).is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn recovery_json_is_compact_not_pretty() {
        let entry = RecoveryEntry::new("main.tex", None, "\\documentclass{article}");
        let journal = RecoveryJournal::new(vec![entry]);

        let json = journal
            .try_to_compact_json()
            .expect("recovery journal should serialize");
        assert_eq!(json, journal.to_json());
        assert!(
            !json.contains("\n  "),
            "recovery JSON must be compact (to_string), not pretty"
        );

        let loaded = RecoveryJournal::from_json(&json).expect("compact JSON should round-trip");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].title, "main.tex");
    }

    #[test]
    fn recovery_disk_payload_is_compact() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let entry = RecoveryEntry::new("draft.tex", None, "Unsaved text content");
        let journal = RecoveryJournal::new(vec![entry]);

        journal
            .save_to_dir(temp.path())
            .expect("Failed to save recovery journal");

        let raw = std::fs::read_to_string(temp.path().join("session_recovery.json"))
            .expect("recovery file should exist");
        assert!(
            !raw.contains("\n  "),
            "recovery file must be compact JSON, not pretty-printed"
        );
        let loaded = RecoveryJournal::from_json(&raw).expect("compact payload should deserialize");
        assert_eq!(loaded.entries[0].content, "Unsaved text content");
    }
}
