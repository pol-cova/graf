//! One folded-search implementation for the project layer. Every index
//! (QuickOpen, bib, labels, Zotero) matches case-insensitively by folding
//! the query once and comparing against pre-folded keys; empty queries
//! match everything.

/// The folded form of `text`: lowercased once, at load time.
pub fn fold(text: &str) -> String {
    text.to_lowercase()
}

/// True when the pre-folded haystack contains the folded `query`. An empty
/// (or whitespace-only) query matches everything, mirroring the previous
/// per-caller behavior without each call site re-implementing the trim.
pub fn matches(folded_haystack: &str, query_lower: &str) -> bool {
    query_lower.is_empty() || folded_haystack.contains(query_lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert!(matches("anything", ""));
        assert!(matches("", ""));
    }

    #[test]
    fn matching_is_case_insensitive_substring() {
        let folded = fold("PRE");
        assert!(matches("preface.typ", &folded));
        assert!(!matches("preface.typ", &fold("post")));
        assert!(matches(
            &fold("Attention Is All You Need"),
            &fold("all you")
        ));
    }

    #[test]
    fn fold_matches_lowercase() {
        assert_eq!(fold("Attention"), "attention");
    }
}
