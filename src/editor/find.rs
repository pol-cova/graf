//! In-buffer Find and Replace engine (spec §M2.4).

use std::ops::Range;

#[cfg(test)]
use crate::editor::buffer::TextBuffer;

/// State for document search and replacement.
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
    /// Creates a new empty find state.
    pub fn new() -> Self {
        Self {
            query: String::new(),
            replace_with: String::new(),
            case_sensitive: false,
            matches: Vec::new(),
            active_match_idx: None,
        }
    }

    /// Updates the search query and recomputes match positions in `content`.
    pub fn set_query(&mut self, query: impl Into<String>, content: &str) {
        self.query = query.into();
        self.recompute_matches(content);
    }

    /// Toggles case-sensitive searching and updates matches.
    pub fn toggle_case_sensitive(&mut self, content: &str) {
        self.case_sensitive = !self.case_sensitive;
        self.recompute_matches(content);
    }

    /// Recomputes all occurrences of `query` in `content`.
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
        } else {
            let lower_content = content.to_lowercase();
            let lower_query = self.query.to_lowercase();
            for (idx, _) in lower_content.match_indices(&lower_query) {
                self.matches.push(idx..idx + query_len);
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

    /// Advances to the next match, wrapping around to the start.
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

    /// Steps backward to the previous match, wrapping around to the end.
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

    /// Returns the currently active match range.
    #[cfg(test)]
    pub fn active_match(&self) -> Option<&Range<usize>> {
        self.active_match_idx.and_then(|idx| self.matches.get(idx))
    }

    /// Returns the match count display string (e.g. `1 of 5` or `0 of 0`).
    pub fn count_label(&self) -> String {
        if self.matches.is_empty() {
            "0 results".to_string()
        } else {
            let current = self.active_match_idx.map_or(0, |i| i + 1);
            format!("{} of {}", current, self.matches.len())
        }
    }

    /// Replaces the currently active match with `replace_with` in `buffer`.
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

    /// Replaces all matches in the buffer within an atomic transaction. Returns count of replacements.
    #[cfg(test)]
    pub fn replace_all(&mut self, buffer: &mut TextBuffer) -> usize {
        if self.matches.is_empty() {
            return 0;
        }

        let count = self.matches.len();
        let initial_cursor = self.matches[0].start;
        buffer.begin_transaction(initial_cursor);

        // Apply replacements in reverse order so character offsets of earlier matches remain valid
        for m in self.matches.iter().rev() {
            buffer.delete(m.clone());
            buffer.insert(m.start, &self.replace_with);
        }

        buffer.end_transaction(initial_cursor + self.replace_with.len());
        self.recompute_matches(buffer.content());
        count
    }
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

        // Next match
        let next = find.next_match().unwrap();
        assert_eq!(*next, 49..52);
        assert_eq!(find.active_match_idx, Some(1));

        // Wrap around
        let wrap = find.next_match().unwrap();
        assert_eq!(*wrap, 16..19);
        assert_eq!(find.active_match_idx, Some(0));

        // Prev match
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

        // Verify that undo completely restores the original content
        buffer.undo();
        assert_eq!(buffer.content(), "foo bar foo baz foo");
    }
}
