//! BibTeX bibliography indexer and LaTeX label registry (spec §M3.4–M3.7).

/// A parsed entry from a `.bib` bibliography file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibEntry {
    pub key: String,
    pub entry_type: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub year: Option<String>,
}

impl BibEntry {
    /// Returns a short label for completion pop-ups (e.g. "Attention Is All You Need (Vaswani et al., 2017)").
    pub fn display_summary(&self) -> String {
        let title = self.title.as_deref().unwrap_or("Untitled");
        let author = self.author.as_deref().unwrap_or("Unknown author");
        let year = self.year.as_deref().unwrap_or("");

        if year.is_empty() {
            format!("{title} — {author}")
        } else {
            format!("{title} ({author}, {year})")
        }
    }
}

/// In-memory index of parsed bibliography entries.
#[derive(Debug, Clone, Default)]
pub struct BibtexIndex {
    pub entries: Vec<BibEntry>,
}

impl BibtexIndex {
    /// Creates a new empty index.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Parses `.bib` source text and populates the index.
    pub fn parse_and_load(&mut self, content: &str) {
        self.entries = parse_bibtex_entries(content);
    }

    /// Adds a single entry to the index if key is unique.
    pub fn add_entry(&mut self, entry: BibEntry) {
        if !self.entries.iter().any(|e| e.key == entry.key) {
            self.entries.push(entry);
        }
    }

    /// Searches entries matching `query` by cite key, title, or author.
    pub fn search(&self, query: &str) -> Vec<&BibEntry> {
        if query.is_empty() {
            return self.entries.iter().collect();
        }

        let query_lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.key.to_lowercase().contains(&query_lower)
                    || e.title
                        .as_ref()
                        .is_some_and(|t| t.to_lowercase().contains(&query_lower))
                    || e.author
                        .as_ref()
                        .is_some_and(|a| a.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

/// Parses a BibTeX formatted string into a vector of [`BibEntry`].
pub fn parse_bibtex_entries(content: &str) -> Vec<BibEntry> {
    let mut entries = Vec::new();

    for block in content.split('@') {
        let trimmed = block.trim();
        if trimmed.is_empty() || trimmed.starts_with('%') {
            continue;
        }

        let Some(open_brace) = trimmed.find('{') else {
            continue;
        };
        let entry_type = trimmed[..open_brace].trim().to_lowercase();
        if entry_type == "comment" || entry_type == "preamble" {
            continue;
        }

        let rest = &trimmed[open_brace + 1..];
        let Some(comma_pos) = rest.find(',') else {
            continue;
        };
        let key = rest[..comma_pos].trim().to_string();

        let mut title = None;
        let mut author = None;
        let mut year = None;

        let fields_str = &rest[comma_pos + 1..];
        for line in fields_str.lines() {
            let line_trimmed = line.trim();
            if let Some((k, v)) = parse_field_line(line_trimmed) {
                match k.as_str() {
                    "title" => title = Some(v),
                    "author" => author = Some(v),
                    "year" => year = Some(v),
                    _ => {}
                }
            }
        }

        if !key.is_empty() {
            entries.push(BibEntry {
                key,
                entry_type,
                title,
                author,
                year,
            });
        }
    }

    entries
}

fn parse_field_line(line: &str) -> Option<(String, String)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim().to_lowercase();
    let mut val = line[eq_pos + 1..].trim();

    val = val.trim_end_matches(',');
    val = val.trim();

    if (val.starts_with('{') && val.ends_with('}')) || (val.starts_with('"') && val.ends_with('"'))
    {
        val = &val[1..val.len() - 1];
    }

    Some((key, val.trim().to_string()))
}

/// Registry of LaTeX cross-reference labels (`\label{...}`).
#[derive(Debug, Clone, Default)]
pub struct LabelIndex {
    pub labels: Vec<String>,
}

impl LabelIndex {
    /// Extracts all `\label{...}` targets from LaTeX content.
    pub fn parse_and_load(&mut self, content: &str) {
        self.labels = parse_latex_labels(content);
    }

    /// Searches labels matching `query`.
    pub fn search(&self, query: &str) -> Vec<&str> {
        if query.is_empty() {
            return self.labels.iter().map(String::as_str).collect();
        }
        let query_lower = query.to_lowercase();
        self.labels
            .iter()
            .filter(|l| l.to_lowercase().contains(&query_lower))
            .map(String::as_str)
            .collect()
    }
}

/// Extracts all `\label{name}` occurrences from source text.
pub fn parse_latex_labels(source: &str) -> Vec<String> {
    let mut labels = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('%') {
            continue;
        }

        let mut pos = 0;
        while let Some(label_idx) = trimmed[pos..].find("\\label{") {
            let start = pos + label_idx + 7;
            if let Some(end) = trimmed[start..].find('}') {
                let label_name = trimmed[start..start + end].trim().to_string();
                if !label_name.is_empty() {
                    labels.push(label_name);
                }
                pos = start + end + 1;
            } else {
                break;
            }
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bibtex_entries() {
        let bib_content = r#"
@article{vaswani2017attention,
  title = {Attention Is All You Need},
  author = {Ashish Vaswani and Noam Shazeer},
  year = {2017},
  journal = {Advances in Neural Information Processing Systems}
}

@book{knuth1984texbook,
  title = "The TeXbook",
  author = "Donald E. Knuth",
  year = "1984"
}
"#;

        let mut index = BibtexIndex::new();
        index.parse_and_load(bib_content);

        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.entries[0].key, "vaswani2017attention");
        assert_eq!(
            index.entries[0].title.as_deref(),
            Some("Attention Is All You Need")
        );
        assert_eq!(index.entries[0].year.as_deref(), Some("2017"));

        assert_eq!(index.entries[1].key, "knuth1984texbook");
        assert_eq!(index.entries[1].title.as_deref(), Some("The TeXbook"));

        // Search test
        let results = index.search("attention");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "vaswani2017attention");

        let results_author = index.search("Knuth");
        assert_eq!(results_author.len(), 1);
        assert_eq!(results_author[0].key, "knuth1984texbook");
    }

    #[test]
    fn test_parse_latex_labels() {
        let latex = r#"
\section{Introduction}\label{sec:intro}
Here is equation \ref{eq:einstein}.
\begin{equation}\label{eq:einstein}
E = mc^2
\end{equation}
See Figure~\ref{fig:arch}.
\begin{figure}\label{fig:arch}
\caption{Architecture}
\end{figure}
"#;

        let mut labels = LabelIndex::default();
        labels.parse_and_load(latex);

        assert_eq!(labels.labels.len(), 3);
        assert_eq!(labels.labels[0], "sec:intro");
        assert_eq!(labels.labels[1], "eq:einstein");
        assert_eq!(labels.labels[2], "fig:arch");

        let matches = labels.search("eq:");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "eq:einstein");
    }
}
