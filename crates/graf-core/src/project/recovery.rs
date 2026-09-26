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

/// Attempts to salvage a tail-truncated JSON document by cutting back to
/// anchor points (closing brackets, innermost last) and auto-closing the
/// outstanding brackets of each prefix.
fn repair_truncated(json: &str) -> Option<RecoveryJournal> {
    let mut anchors: Vec<usize> = json
        .char_indices()
        .filter_map(|(i, c)| matches!(c, '}' | ']').then_some(i))
        .collect();
    anchors.reverse();

    for anchor in anchors {
        let prefix = &json[..=anchor];
        let closers = closing_suffix(prefix);
        let Ok(candidate) = serde_json::from_str::<RecoveryJournal>(&format!("{prefix}{closers}"))
        else {
            continue;
        };
        return Some(candidate);
    }
    None
}

/// Closing characters needed to balance `prefix`, ignoring bracket
/// characters that appear inside strings.
fn closing_suffix(prefix: &str) -> String {
    let mut stack = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for character in prefix.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else {
            match character {
                '"' => in_string = true,
                '{' => stack.push('}'),
                '[' => stack.push(']'),
                '}' | ']' => {
                    stack.pop();
                }
                _ => {}
            }
        }
    }
    if in_string {
        stack.push('"');
    }
    stack.iter().rev().collect()
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

    /// Serialization is infallible for this shape, but the Result keeps the
    /// no-clobber contract: a serialize failure means `save_to_dir` writes
    /// nothing rather than an empty journal over a good one.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json)
            .ok()
            // A journal written mid-crash can be tail-truncated: close the
            // document at the last anchor point that still parses so
            // completed entries survive instead of being discarded.
            .or_else(|| repair_truncated(json))
    }

    pub fn save_to_dir(&self, dir: &Path) -> std::io::Result<PathBuf> {
        fs::create_dir_all(dir)?;
        let json = self.to_json().map_err(|error| {
            std::io::Error::other(format!("Failed to serialize recovery journal: {error}"))
        })?;
        let file_path = dir.join(RECOVERY_FILE_NAME);
        atomic_write(&file_path, json.as_bytes())?;
        Ok(file_path)
    }

    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let file_path = dir.join(RECOVERY_FILE_NAME);
        if file_path.exists() {
            let content = match fs::read_to_string(&file_path) {
                Ok(content) => content,
                Err(error) => {
                    log::warn!("recovery journal could not be read: {error}");
                    return None;
                }
            };
            match Self::from_json(&content) {
                Some(journal) => Some(journal),
                None => {
                    // Never delete the raw journal: the user may rescue the
                    // content by hand. Keep it for manual inspection.
                    log::warn!(
                        "recovery journal at {} could not be parsed; the raw file is left \
                         untouched for manual recovery",
                        file_path.display()
                    );
                    None
                }
            }
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
        let json = journal.to_json().expect("journal serialization");

        let loaded =
            RecoveryJournal::from_json(&json).expect("Failed to deserialize recovery journal");
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].title, "main.tex");
        assert_eq!(loaded.entries[1].content, "= Title");
    }

    #[test]
    fn corrupted_journal_is_preserved_and_truncation_repaired() {
        let temp_dir =
            std::env::temp_dir().join(format!("graf_recovery_corrupt_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let entry = RecoveryEntry::new("draft.typ", None, "unsaved ideas");
        let journal = RecoveryJournal::new(vec![entry]);

        // A crash mid-write leaves a tail-truncated journal: the parse must
        // salvage completed entries without deleting the file.
        let full_json = journal.to_json().unwrap();
        let truncated = &full_json[..full_json.len() - 1];
        fs::write(temp_dir.join(RECOVERY_FILE_NAME), truncated).unwrap();

        let loaded = RecoveryJournal::load_from_dir(&temp_dir);
        assert!(loaded.is_some(), "truncation repair should recover entries");
        assert_eq!(loaded.unwrap().entries[0].content, "unsaved ideas");

        // Cut at a known anchor (after the entry's closing `}`) rather than
        // counting serde's exact whitespace: earlier entries still recover.
        let entry_object_end = full_json
            .find("}")
            .expect("pretty-printed journal contains object braces")
            + "}".len();
        let mid_cut = &full_json[..entry_object_end];
        fs::write(temp_dir.join(RECOVERY_FILE_NAME), mid_cut).unwrap();
        let repaired = RecoveryJournal::load_from_dir(&temp_dir);
        let repaired = repaired.expect("anchor-cut journal must still salvage the first entry");
        assert_eq!(repaired.entries.len(), 1);
        assert_eq!(repaired.entries[0].content, "unsaved ideas");

        // The raw journal is never deleted on parse failure.
        assert!(temp_dir.join(RECOVERY_FILE_NAME).exists());

        let _ = fs::remove_dir_all(&temp_dir);
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
