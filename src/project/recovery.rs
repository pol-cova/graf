//! Crash recovery and autosave journaling system (spec §7.4, M7).
//!
//! Preserves unsaved document buffers periodically to prevent data loss.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::persistence::atomic_write;

/// A snapshot entry of an unsaved document buffer.
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

/// The recovery journal state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RecoveryJournal {
    pub entries: Vec<RecoveryEntry>,
}

impl RecoveryJournal {
    pub fn new(entries: Vec<RecoveryEntry>) -> Self {
        Self { entries }
    }

    /// Serializes journal to JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Deserializes journal from JSON.
    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    /// Saves the current journal to the recovery file path atomically.
    pub fn save_to_dir(&self, dir: &Path) -> std::io::Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let file_path = dir.join("session_recovery.json");
        atomic_write(&file_path, self.to_json().as_bytes())?;
        Ok(file_path)
    }

    /// Loads the latest recovery journal from directory, if present.
    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let file_path = dir.join("session_recovery.json");
        if file_path.exists() {
            let content = fs::read_to_string(&file_path).ok()?;
            Self::from_json(&content)
        } else {
            None
        }
    }

    /// Removes the recovery journal once all buffers are cleanly saved.
    pub fn clear_dir(dir: &Path) -> std::io::Result<()> {
        let file_path = dir.join("session_recovery.json");
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
}
