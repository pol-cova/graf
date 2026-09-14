//! Project-scoped indexes the workspace keeps in memory. They load from the
//! project files and refresh through `reload_*`; bundling them in one place
//! keeps the workspace shell from re-implementing a project-index layer.

use super::bibtex::{BibtexIndex, LabelIndex};

#[derive(Debug, Default)]
pub struct ProjectState {
    /// Bibtex entries from `*.bib` files plus the Zotero library.
    pub bib_index: BibtexIndex,
    /// `\label` items parsed from the active editor text.
    pub label_index: LabelIndex,
}

impl ProjectState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reloads `*.bib` files at the project root. Re-parses with entries
    /// accumulated until explicitly reset, matching the previous behavior
    /// of incremental bib loads.
    pub fn reload_bib_files(&mut self, root: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "bib") {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                self.bib_index.parse_and_load(&content);
            }
        }
    }

    pub fn reload_editor_labels(&mut self, text: &str) {
        self.label_index.parse_and_load(text);
    }
}
