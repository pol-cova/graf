//! Project-scoped indexes the workspace keeps in memory. They load from the
//! project files and refresh through `reload_*`; bundling them in one place
//! keeps the workspace shell from re-implementing a project-index layer.

use super::bibtex::{BibtexIndex, LabelIndex};
use super::kinds::FileKind;

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

    /// Reloads every `*.bib` file at the project root into one index.
    /// Entries from all files are kept; the first definition of a key wins.
    pub fn reload_bib_files(&mut self, root: &std::path::Path) {
        self.bib_index.entries.clear();
        let Ok(entries) = std::fs::read_dir(root) else {
            return;
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| FileKind::from_path(path) == FileKind::Bibtex)
            .collect();
        // Directory order is arbitrary; sort so duplicate keys resolve the
        // same way every time.
        paths.sort();
        for path in paths {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            for entry in super::bibtex::parse_bibtex_entries(&content) {
                self.bib_index.add_entry(entry);
            }
        }
    }

    /// Adds Zotero library entries after the project's own, so a project
    /// `.bib` key always wins over the same key from Zotero.
    pub fn add_zotero_library(&mut self, library: &super::zotero::ZoteroLibrary) {
        for item in &library.items {
            self.bib_index.add_entry(item.to_bib_entry());
        }
    }

    /// Loads `\label` keys from every LaTeX file in the project, so a
    /// reference can point at a label defined in another file.
    pub fn reload_project_labels(&mut self, root: &std::path::Path) {
        let project = super::tree::ProjectTree::scan(root);
        let mut sources = String::new();
        for entry in project.quick_open_matches("", usize::MAX) {
            if entry.kind == FileKind::Latex
                && let Ok(content) = std::fs::read_to_string(&entry.path)
            {
                sources.push_str(&content);
                sources.push('\n');
            }
        }
        self.label_index.parse_and_load(&sources);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bib_file_contributes_entries() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(
            temp.path().join("a.bib"),
            "@article{alpha2020, title={Alpha}, year={2020}}",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("b.bib"),
            "@book{beta2021, title={Beta}, year={2021}}",
        )
        .unwrap();

        let mut state = ProjectState::new();
        state.reload_bib_files(temp.path());
        state.reload_bib_files(temp.path());

        let mut keys: Vec<_> = state
            .bib_index
            .entries
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            ["alpha2020", "beta2021"],
            "reloading must not duplicate"
        );
    }

    #[test]
    fn labels_come_from_every_latex_file() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("sections")).unwrap();
        std::fs::write(temp.path().join("main.tex"), "\\label{sec:intro}").unwrap();
        std::fs::write(
            temp.path().join("sections/method.tex"),
            "\\label{fig:recall}",
        )
        .unwrap();

        let mut state = ProjectState::new();
        state.reload_project_labels(temp.path());

        let mut labels = state.label_index.labels.clone();
        labels.sort();
        assert_eq!(labels, ["fig:recall", "sec:intro"]);
    }
}
