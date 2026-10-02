use crate::project::bibtex::BibEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoteroItem {
    /// The BibTeX key from the Zotero export.
    pub citekey: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    /// The entry type from the export, lowercased (`article`, `inproceedings`,
    /// …). Carried through unchanged.
    ///
    /// This replaces a `publication: Option<String>` field that was always
    /// `None`, and whose only effect was choosing `@article` versus `@misc` —
    /// so every Zotero citation came out as `@misc` regardless of what the
    /// user's library said. The parser already produced the real type.
    pub entry_type: String,
}

impl ZoteroItem {
    pub fn to_bib_entry(&self) -> BibEntry {
        let title = Some(self.title.clone());
        let author = if self.authors.is_empty() {
            None
        } else {
            Some(self.authors.join(" and "))
        };
        BibEntry::new(
            self.citekey.clone(),
            self.entry_type.clone(),
            title,
            author,
            self.year.map(|y| y.to_string()),
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct ZoteroLibrary {
    pub items: Vec<ZoteroItem>,
}

impl ZoteroLibrary {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn scan_local_storage() -> Self {
        let mut lib = Self::new();

        if let Some(home) = crate::util::home_dir() {
            let candidates = [
                home.join("Zotero/better-bibtex.bib"),
                home.join("Zotero/My Library.bib"),
                home.join("Zotero/library.bib"),
                home.join("Documents/Zotero.bib"),
            ];

            for path in &candidates {
                if let Ok(content) = std::fs::read_to_string(path) {
                    lib.load_from_bibtex(&content);
                    if !lib.items.is_empty() {
                        break;
                    }
                }
            }
        }

        lib
    }

    pub fn load_from_bibtex(&mut self, content: &str) {
        let entries = crate::project::bibtex::parse_bibtex_entries(content);
        for e in entries {
            let authors: Vec<String> = e
                .author
                .as_deref()
                .unwrap_or("")
                .split(" and ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let year: Option<u32> = e.year.as_deref().and_then(|y| y.parse().ok());

            self.items.push(ZoteroItem {
                citekey: e.key,
                title: e.title.unwrap_or_else(|| "Untitled".to_string()),
                authors,
                year,
                entry_type: e.entry_type,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_entry_type_is_carried_through() {
        let item = ZoteroItem {
            citekey: "vaswani2017attention".to_string(),
            title: "Attention Is All You Need".to_string(),
            authors: vec!["Ashish Vaswani".to_string(), "Noam Shazeer".to_string()],
            year: Some(2017),
            entry_type: "inproceedings".to_string(),
        };

        let entry = item.to_bib_entry();
        assert_eq!(entry.key, "vaswani2017attention");
        assert_eq!(entry.title.as_deref(), Some("Attention Is All You Need"));
        // The type comes from the library, not from a guess. Before this,
        // `publication` was always None so every Zotero entry became @misc.
        assert_eq!(entry.entry_type, "inproceedings");
        assert_eq!(
            entry.author.as_deref(),
            Some("Ashish Vaswani and Noam Shazeer")
        );
        assert_eq!(entry.year.as_deref(), Some("2017"));
    }

    #[test]
    fn an_authorless_item_still_produces_a_usable_entry() {
        let item = ZoteroItem {
            citekey: "thesis2026".to_string(),
            title: "A Thesis".to_string(),
            authors: vec![],
            year: None,
            entry_type: "phdthesis".to_string(),
        };

        let entry = item.to_bib_entry();
        assert_eq!(entry.entry_type, "phdthesis");
        assert_eq!(entry.author, None);
        assert_eq!(entry.year, None);
    }

    /// The end-to-end case that was wrong: a real `@inproceedings` entry in a
    /// Better BibTeX export must keep its type all the way to a BibEntry.
    #[test]
    fn loading_a_bibtex_export_preserves_each_entrys_type() {
        let content = r#"
@inproceedings{vaswani2017,
  title = {Attention Is All You Need},
  author = {Vaswani, Ashish and Shazeer, Noam},
  booktitle = {NeurIPS},
  year = {2017}
}
@book{knuth1984,
  title = {The TeXbook},
  author = {Knuth, Donald E.},
  year = {1984}
}
"#;
        let mut library = ZoteroLibrary::new();
        library.load_from_bibtex(content);

        let types: Vec<String> = library
            .items
            .iter()
            .map(|item| item.to_bib_entry().entry_type)
            .collect();
        assert_eq!(types, ["@inproceedings", "@book"]);
    }
}
