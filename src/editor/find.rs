use std::ops::Range;

#[cfg(test)]
use crate::editor::buffer::TextBuffer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindState {
    pub query: String,
    pub replace_with: String,
    pub case_sensitive: bool,
    pub matches: Vec<Range<usize>>,
    pub active_match_idx: Option<usize>,
}

impl Default for FindState {
    fn default() -> Self {
        Self::new()
    }
}

impl FindState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            replace_with: String::new(),
            case_sensitive: false,
            matches: Vec::new(),
            active_match_idx: None,
        }
    }

    pub fn set_query(&mut self, query: impl Into<String>, content: &str) {
        self.query = query.into();
        self.recompute_matches(content);
    }

    pub fn toggle_case_sensitive(&mut self, content: &str) {
        self.case_sensitive = !self.case_sensitive;
        self.recompute_matches(content);
    }

    pub fn recompute_matches(&mut self, content: &str) {
        self.matches.clear();
        if self.query.is_empty() {
            self.active_match_idx = None;
            return;
        }

        let query_len = self.query.len();
        if self.case_sensitive {
            for (idx, _) in content.match_indices(&self.query) {
                self.matches.push(idx..idx + query_len);
            }
        } else if self.query.is_ascii() {
            // Fast path: the query is ASCII, so folding per byte keeps the
            // byte ranges valid and needs no full-document copy.
            let query = self.query.as_bytes();
            for start in 0..content.len().saturating_sub(query_len - 1) {
                if equal_ignoring_ascii_case(&content.as_bytes()[start..start + query_len], query) {
                    self.matches.push(start..start + query_len);
                }
            }
        } else {
            // General Unicode path: walk char boundaries and compare each
            // candidate with full lowercase folding, so multi-byte characters
            // whose fold changes byte length (İ) can never shift ranges the
            // way a folded copy would.
            let query_chars: Vec<char> = self.query.chars().collect();
            for (start, first) in content.char_indices() {
                if !chars_fold_equal(first, query_chars[0]) {
                    continue;
                }
                let mut consumed = first.len_utf8();
                let mut matched = true;
                let mut rest = content[start + consumed..].chars();
                for &expected in &query_chars[1..] {
                    match rest.next() {
                        Some(candidate) if chars_fold_equal(candidate, expected) => {
                            consumed += candidate.len_utf8();
                        }
                        _ => {
                            matched = false;
                            break;
                        }
                    }
                }
                if matched {
                    self.matches.push(start..start + consumed);
                }
            }
        }

        if self.matches.is_empty() {
            self.active_match_idx = None;
        } else if self
            .active_match_idx
            .is_none_or(|i| i >= self.matches.len())
        {
            self.active_match_idx = Some(0);
        }
    }

    pub fn next_match(&mut self) -> Option<&Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let next_idx = match self.active_match_idx {
            Some(curr) => (curr + 1) % self.matches.len(),
            None => 0,
        };
        self.active_match_idx = Some(next_idx);
        self.matches.get(next_idx)
    }

    pub fn prev_match(&mut self) -> Option<&Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }
        let prev_idx = match self.active_match_idx {
            Some(curr) if curr > 0 => curr - 1,
            _ => self.matches.len() - 1,
        };
        self.active_match_idx = Some(prev_idx);
        self.matches.get(prev_idx)
    }

    #[cfg(test)]
    pub fn active_match(&self) -> Option<&Range<usize>> {
        self.active_match_idx.and_then(|idx| self.matches.get(idx))
    }

    pub fn count_label(&self) -> String {
        if self.matches.is_empty() {
            "0 results".to_string()
        } else {
            let current = self.active_match_idx.map_or(0, |i| i + 1);
            format!("{} of {}", current, self.matches.len())
        }
    }

    #[cfg(test)]
    pub fn replace_current(&mut self, buffer: &mut TextBuffer) -> Option<usize> {
        let range = self.active_match()?.clone();
        buffer.begin_transaction(range.start);
        buffer.delete(range.clone());
        buffer.insert(range.start, &self.replace_with);
        let new_cursor = range.start + self.replace_with.len();
        buffer.end_transaction(new_cursor);

        self.recompute_matches(buffer.content());
        Some(new_cursor)
    }

    #[cfg(test)]
    pub fn replace_all(&mut self, buffer: &mut TextBuffer) -> usize {
        if self.matches.is_empty() {
            return 0;
        }

        let count = self.matches.len();
        let initial_cursor = self.matches[0].start;
        buffer.begin_transaction(initial_cursor);

        for m in self.matches.iter().rev() {
            buffer.delete(m.clone());
            buffer.insert(m.start, &self.replace_with);
        }

        buffer.end_transaction(initial_cursor + self.replace_with.len());
        self.recompute_matches(buffer.content());
        count
    }
}

/// Byte-wise ASCII case-insensitive comparison; non-ASCII bytes (never
/// present in the query on this path) simply must match exactly, so a
/// multi-byte character can never be confused with an ASCII one.
fn equal_ignoring_ascii_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

/// Case-fold equivalence per character using full lowercase expansion, no
/// allocations.
fn chars_fold_equal(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_and_navigation() {
        let text = "The quick brown fox jumps over the lazy dog. The fox is fast.";
        let mut find = FindState::new();

        find.set_query("fox", text);
        assert_eq!(find.matches.len(), 2);
        assert_eq!(find.active_match_idx, Some(0));
        assert_eq!(find.active_match(), Some(&(16..19)));

        let next = find.next_match().unwrap();
        assert_eq!(*next, 49..52);
        assert_eq!(find.active_match_idx, Some(1));

        let wrap = find.next_match().unwrap();
        assert_eq!(*wrap, 16..19);
        assert_eq!(find.active_match_idx, Some(0));

        let prev = find.prev_match().unwrap();
        assert_eq!(*prev, 49..52);
        assert_eq!(find.active_match_idx, Some(1));
    }

    #[test]
    fn test_toggle_case_sensitive() {
        let text = "LaTeX and latex and LATEX";
        let mut find = FindState::new();
        find.set_query("latex", text);
        assert_eq!(find.matches.len(), 3);

        find.toggle_case_sensitive(text);
        assert_eq!(find.matches.len(), 1);
        assert_eq!(find.matches[0], 10..15);
    }

    #[test]
    fn test_replace_current() {
        let mut buffer = TextBuffer::from_text("hello world, hello graf");
        let mut find = FindState::new();
        find.replace_with = "hi".to_string();
        find.set_query("hello", buffer.content());

        assert_eq!(find.matches.len(), 2);
        let cursor = find.replace_current(&mut buffer);
        assert_eq!(cursor, Some(2));
        assert_eq!(buffer.content(), "hi world, hello graf");
        assert_eq!(find.matches.len(), 1);
    }

    #[test]
    fn test_replace_all() {
        let mut buffer = TextBuffer::from_text("foo bar foo baz foo");
        let mut find = FindState::new();
        find.replace_with = "qux".to_string();
        find.set_query("foo", buffer.content());

        assert_eq!(find.matches.len(), 3);
        let count = find.replace_all(&mut buffer);
        assert_eq!(count, 3);
        assert_eq!(buffer.content(), "qux bar qux baz qux");
        assert_eq!(find.matches.len(), 0);

        buffer.undo();
        assert_eq!(buffer.content(), "foo bar foo baz foo");
    }

    #[test]
    fn case_insensitive_matches_stay_byte_aligned_after_folding_anomalies() {
        // 'İ' (U+0130, 2 bytes) lowers to 3 bytes in a folded copy, which
        // used to shift every highlight after it.
        let text = "İx İstanbul latex İ";
        let mut find = FindState::new();
        find.set_query("latex", text);
        assert_eq!(find.matches.len(), 1);
        assert_eq!(find.active_match(), Some(&(14..19)));
        // Exact ranges point at real chars.
        assert_eq!(&text[14..19], "latex");
    }

    #[test]
    fn case_insensitive_matches_chars_of_differing_width() {
        let mut find = FindState::new();
        // Σ (2 bytes) folds to σ (2 bytes) — ok; and 'İ' as haystack char.
        find.set_query("σ", "Σx Σy");
        assert_eq!(find.matches.len(), 2);
        assert_eq!(&"Σx Σy"[find.active_match().unwrap().clone()], "Σ");

        // A non-ASCII query never matches half of a multi-byte cluster.
        find.set_query("x", "日本語");
        assert!(find.matches.is_empty());
    }

    #[test]
    fn ascii_query_is_allocation_free_path() {
        let text = "TYPE type";
        let mut find = FindState::new();
        find.set_query("type", text);
        assert_eq!(find.matches.len(), 2);
        assert_eq!(find.matches[0], 0..4);
        assert_eq!(find.matches[1], 5..9);
    }
}
