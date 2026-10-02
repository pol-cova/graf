use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::persistence::atomic_write;

const RECOVERY_FILE_NAME: &str = "session_recovery.json";

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

    /// The recovery folder of a project: `.graf/recovery` under its root.
    pub fn project_dir(project_root: &Path) -> PathBuf {
        project_root.join(".graf").join("recovery")
    }

    /// Records unsaved `content` for `path`, replacing any older entry for
    /// the same file and keeping entries other windows wrote. Several
    /// windows can share one project without overwriting each other.
    pub fn record(dir: &Path, path: &Path, content: &str) -> std::io::Result<()> {
        let mut journal = Self::load_from_dir(dir).unwrap_or_default();
        journal
            .entries
            .retain(|entry| entry.path.as_deref() != Some(path));
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        journal
            .entries
            .push(RecoveryEntry::new(title, Some(path.to_path_buf()), content));
        journal.save_to_dir(dir).map(|_| ())
    }

    /// Drops the entry for `path` once it is safely saved. Removes the
    /// journal file when nothing is left to recover.
    pub fn forget(dir: &Path, path: &Path) -> std::io::Result<()> {
        let Some(mut journal) = Self::load_from_dir(dir) else {
            return Ok(());
        };
        let before = journal.entries.len();
        journal
            .entries
            .retain(|entry| entry.path.as_deref() != Some(path));
        if journal.entries.is_empty() {
            Self::clear_dir(dir)
        } else if journal.entries.len() != before {
            journal.save_to_dir(dir).map(|_| ())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_replaces_the_entry_for_the_same_file_and_keeps_others() {
        let temp = tempfile::tempdir().unwrap();
        let dir = RecoveryJournal::project_dir(temp.path());
        let main = temp.path().join("main.tex");
        let intro = temp.path().join("intro.tex");

        RecoveryJournal::record(&dir, &main, "first").unwrap();
        RecoveryJournal::record(&dir, &intro, "intro draft").unwrap();
        RecoveryJournal::record(&dir, &main, "second").unwrap();

        let journal = RecoveryJournal::load_from_dir(&dir).unwrap();
        assert_eq!(journal.entries.len(), 2);
        let main_entry = journal
            .entries
            .iter()
            .find(|entry| entry.path.as_deref() == Some(main.as_path()))
            .unwrap();
        assert_eq!(main_entry.content, "second");
        assert_eq!(main_entry.title, "main.tex");
    }

    #[test]
    fn forget_removes_one_entry_then_the_journal() {
        let temp = tempfile::tempdir().unwrap();
        let dir = RecoveryJournal::project_dir(temp.path());
        let main = temp.path().join("main.tex");
        let intro = temp.path().join("intro.tex");
        RecoveryJournal::record(&dir, &main, "a").unwrap();
        RecoveryJournal::record(&dir, &intro, "b").unwrap();

        RecoveryJournal::forget(&dir, &main).unwrap();
        assert_eq!(
            RecoveryJournal::load_from_dir(&dir).unwrap().entries.len(),
            1
        );

        RecoveryJournal::forget(&dir, &intro).unwrap();
        assert!(RecoveryJournal::load_from_dir(&dir).is_none());
        // Forgetting with no journal at all is not an error.
        RecoveryJournal::forget(&dir, &intro).unwrap();
    }

    #[test]
    fn an_entry_records_the_file_it_journaled() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("paper.tex");
        std::fs::write(&path, "on disk").unwrap();

        let existing = RecoveryEntry::new("paper.tex", Some(path.clone()), "unsaved");
        let untitled = RecoveryEntry::new("notes.typ", None, "= Notes");

        // The journal stores the path and leaves the "does this still exist"
        // decision to the app: `Workspace.restoreRecovered` writes the
        // recovered text to this path when it differs from the open file, and
        // a missing one surfaces as a save error rather than silently
        // creating a new buffer. `RestoreTarget` used to make that choice
        // here, and nothing constructed one.
        assert_eq!(existing.path.as_deref(), Some(path.as_path()));
        assert_eq!(existing.title, "paper.tex");
        assert_eq!(untitled.path, None);
        assert_eq!(untitled.title, "notes.typ");
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
