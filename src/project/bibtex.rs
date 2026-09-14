#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibEntry {
    pub key: String,
    pub entry_type: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub year: Option<String>,
    /// Lowercased search keys computed once at parse time, so per-keystroke
    /// search never re-folds every candidate.
    pub key_lower: String,
    pub title_lower: Option<String>,
    pub author_lower: Option<String>,
}

impl BibEntry {
    pub fn new(
        key: String,
        entry_type: String,
        title: Option<String>,
        author: Option<String>,
        year: Option<String>,
    ) -> Self {
        BibEntry {
            key_lower: key.to_lowercase(),
            title_lower: title.as_ref().map(|t| t.to_lowercase()),
            author_lower: author.as_ref().map(|a| a.to_lowercase()),
            key,
            entry_type,
            title,
            author,
            year,
        }
    }

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

#[derive(Debug, Clone, Default)]
pub struct BibtexIndex {
    pub entries: Vec<BibEntry>,
}

impl BibtexIndex {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn parse_and_load(&mut self, content: &str) {
        self.entries = parse_bibtex_entries(content);
    }

    pub fn add_entry(&mut self, entry: BibEntry) {
        if !self.entries.iter().any(|e| e.key == entry.key) {
            self.entries.push(entry);
        }
    }

    pub fn search(&self, query: &str) -> Vec<&BibEntry> {
        let query_lower = crate::project::text_search::fold(query.trim());
        self.entries
            .iter()
            .filter(|e| {
                crate::project::text_search::matches(&e.key_lower, &query_lower)
                    || e.title_lower
                        .as_ref()
                        .is_some_and(|t| crate::project::text_search::matches(t, &query_lower))
                    || e.author_lower
                        .as_ref()
                        .is_some_and(|a| crate::project::text_search::matches(a, &query_lower))
            })
            .collect()
    }
}

pub fn parse_bibtex_entries(content: &str) -> Vec<BibEntry> {
    let mut entries = Vec::new();

    for block in delimit_entries(content) {
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
            entries.push(BibEntry::new(key, entry_type, title, author, year));
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

#[derive(Debug, Clone, Default)]
pub struct LabelIndex {
    pub labels: Vec<String>,
    /// Parallel lowercase copy of `labels`, folded once at load.
    pub labels_lower: Vec<String>,
}

impl LabelIndex {
    pub fn parse_and_load(&mut self, content: &str) {
        self.labels = parse_latex_labels(content);
        self.labels_lower = self.labels.iter().map(|l| l.to_lowercase()).collect();
    }

    pub fn search(&self, query: &str) -> Vec<&str> {
        let query_lower = crate::project::text_search::fold(query.trim());
        self.labels
            .iter()
            .zip(&self.labels_lower)
            .filter(|(_, lower)| crate::project::text_search::matches(lower, &query_lower))
            .map(|(label, _)| label.as_str())
            .collect()
    }
}

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

/// Splits a bib file into entry blocks whose boundaries are `@` characters
/// *outside* any brace-delimited value, so an `@` in an author email or a
/// note string can no longer fabricate bogus entries.
fn delimit_entries(content: &str) -> Vec<&str> {
    let bytes = content.as_bytes();
    let mut boundaries = Vec::new();
    let mut brace_depth = 0usize;
    let mut in_quotes = false;
    for (index, &byte) in bytes.iter().enumerate() {
        // Track quoting of the string-style field layout too: "@" inside a
        // quoted value is data.
        match byte {
            b'"' => in_quotes = !in_quotes,
            b'{' if !in_quotes => brace_depth += 1,
            b'}' if !in_quotes => brace_depth = brace_depth.saturating_sub(1),
            b'@' if brace_depth == 0 && !in_quotes => boundaries.push(index),
            _ => {}
        }
    }
    boundaries
        .iter()
        .zip(
            boundaries
                .iter()
                .skip(1)
                .chain(std::iter::once(&bytes.len())),
        )
        .map(|(&start, &end)| &content[start..end])
        .collect()
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

        let results = index.search("attention");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "vaswani2017attention");

        let results_author = index.search("Knuth");
        assert_eq!(results_author.len(), 1);
        assert_eq!(results_author[0].key, "knuth1984texbook");
    }

    #[test]
    fn search_is_trimmed_and_case_insensitive_for_every_field() {
        let mut index = BibtexIndex::new();
        index.parse_and_load(
            "@article{vaswani2017, title = {Attention Is All You Need}, author = {Ashish Vaswani}}",
        );

        // Shared folded-search semantics: surrounding whitespace in the
        // query is ignored and matching is a case-insensitive substring.
        assert_eq!(index.search("  ATTENTION  ").len(), 1);
        assert_eq!(index.search(" VASWANI2017 ").len(), 1);
        assert_eq!(index.search(" ashish ").len(), 1);
        assert!(index.search("nothing-matches").is_empty());
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
    #[test]
    fn email_at_in_value_does_not_fabricate_entries() {
        let source = r#"@article{withmail,
  title = {EmailHaiku},
  author = {Smith, Jane <jane@example.com> and Roe, John (mailto:john@org.io)}
}"#;
        // The `@`s inside the author value used to split entries.
        let entries = parse_bibtex_entries(source);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "withmail");
        assert!(
            entries[0]
                .author
                .as_deref()
                .is_some_and(|a| a.contains("john@org.io"))
        );
    }

    #[test]
    fn at_in_a_quoted_value_is_data() {
        let source = r#"@misc{quoted,
  title = "someone@host"
}"#;
        let entries = parse_bibtex_entries(source);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title.as_deref(), Some("someone@host"));
    }
}
