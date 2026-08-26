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
    pub fn to_bibtex(&self) -> String {
        let entry_type = if self.publication.is_some() {
            "article"
        } else {
            "misc"
        };
        let authors_str = self.authors.join(" and ");
        let mut out = format!("@{entry_type}{{{},\n", self.citekey);
        out.push_str(&format!("  title = {{{}}},\n", self.title));
        if !authors_str.is_empty() {
            out.push_str(&format!("  author = {{{authors_str}}},\n"));
        }
        if let Some(year) = self.year {
            out.push_str(&format!("  year = {{{year}}},\n"));
        }
        if let Some(pub_name) = &self.publication {
            out.push_str(&format!("  journal = {{{pub_name}}},\n"));
        }
        if let Some(doi) = &self.doi {
            out.push_str(&format!("  doi = {{{doi}}},\n"));
        }
        if let Some(url) = &self.url {
            out.push_str(&format!("  url = {{{url}}},\n"));
        }
        out.push_str("}\n");
        out
    }

    pub fn to_bib_entry(&self) -> BibEntry {
        BibEntry {
            key: self.citekey.clone(),
            entry_type: if self.publication.is_some() {
                "article".to_string()
            } else {
                "misc".to_string()
            },
            title: Some(self.title.clone()),
            author: if self.authors.is_empty() {
                None
            } else {
                Some(self.authors.join(" and "))
            },
            year: self.year.map(|y| y.to_string()),
        }
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

    pub fn search(&self, query: &str) -> Vec<&ZoteroItem> {
        let q = query.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                item.citekey.to_lowercase().contains(&q)
                    || item.title.to_lowercase().contains(&q)
                    || item.authors.iter().any(|a| a.to_lowercase().contains(&q))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zotero_item_to_bibtex_and_entry() {
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

        let bibtex = item.to_bibtex();
        assert!(bibtex.starts_with("@article{vaswani2017attention,"));
        assert!(bibtex.contains("title = {Attention Is All You Need},"));
        assert!(bibtex.contains("author = {Ashish Vaswani and Noam Shazeer},"));
        assert!(bibtex.contains("journal = {NeurIPS},"));
        assert!(bibtex.contains("year = {2017},"));

        let entry = item.to_bib_entry();
        assert_eq!(entry.key, "vaswani2017attention");
        assert_eq!(entry.title.as_deref(), Some("Attention Is All You Need"));
    }

    #[test]
    fn test_zotero_library_search() {
        let mut lib = ZoteroLibrary::new();
        lib.items.push(ZoteroItem {
            key: "1".to_string(),
            citekey: "lecun2015deep".to_string(),
            title: "Deep Learning".to_string(),
            authors: vec!["Yann LeCun".to_string(), "Yoshua Bengio".to_string()],
            year: Some(2015),
            publication: Some("Nature".to_string()),
            abstract_note: None,
            doi: None,
            url: None,
            pdf_path: None,
        });

        assert_eq!(lib.search("deep").len(), 1);
        assert_eq!(lib.search("bengio").len(), 1);
        assert_eq!(lib.search("transformer").len(), 0);
    }
}
