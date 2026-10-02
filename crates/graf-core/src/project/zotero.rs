use std::path::PathBuf;

use crate::project::bibtex::BibEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoteroItem {
    pub key: String,
    pub citekey: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub publication: Option<String>,
    pub abstract_note: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub pdf_path: Option<PathBuf>,
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
            if self.publication.is_some() {
                "article".to_string()
            } else {
                "misc".to_string()
            },
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
                key: e.key.clone(),
                citekey: e.key,
                title: e.title.unwrap_or_else(|| "Untitled".to_string()),
                authors,
                year,
                publication: None,
                abstract_note: None,
                doi: None,
                url: None,
                pdf_path: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zotero_item_to_bib_entry() {
        let item = ZoteroItem {
            key: "item1".to_string(),
            citekey: "vaswani2017attention".to_string(),
            title: "Attention Is All You Need".to_string(),
            authors: vec!["Ashish Vaswani".to_string(), "Noam Shazeer".to_string()],
            year: Some(2017),
            publication: Some("NeurIPS".to_string()),
            abstract_note: Some("The dominant sequence transduction models...".to_string()),
            doi: Some("10.5555/3295222.3295349".to_string()),
            url: Some("https://arxiv.org/abs/1706.03762".to_string()),
            pdf_path: None,
        };

        let entry = item.to_bib_entry();
        assert_eq!(entry.key, "vaswani2017attention");
        assert_eq!(entry.title.as_deref(), Some("Attention Is All You Need"));
        assert_eq!(entry.entry_type, "article");
        assert_eq!(
            entry.author.as_deref(),
            Some("Ashish Vaswani and Noam Shazeer")
        );
        assert_eq!(entry.year.as_deref(), Some("2017"));
    }

    #[test]
    fn test_zotero_item_to_bib_entry_without_publication() {
        let item = ZoteroItem {
            key: "item1".to_string(),
            citekey: "thesis2026".to_string(),
            title: "A Thesis".to_string(),
            authors: vec![],
            year: None,
            publication: None,
            abstract_note: None,
            doi: None,
            url: None,
            pdf_path: None,
        };

        let entry = item.to_bib_entry();
        assert_eq!(entry.entry_type, "misc");
        assert_eq!(entry.author, None);
        assert_eq!(entry.year, None);
    }
}
