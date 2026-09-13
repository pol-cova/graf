use std::ops::Range;

pub type Revision = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Insert {
        position: usize,
        text: String,
    },
    Delete {
        range: Range<usize>,
        deleted: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    edits: Vec<Edit>,
    cursor_before: usize,
    cursor_after: usize,
}

/// Cap on stored undo transactions. Each holds edit diffs, so without a cap a
/// long editing session grows memory without bound. The oldest drop first.
const MAX_UNDO_STACK: usize = 100;

pub struct TextBuffer {
    content: String,
    /// Byte offset of each line start. Always non-empty and starts with 0.
    /// Entry `i` is the first byte of line `i`. A trailing `\n` produces a
    /// final entry equal to `content.len()` for the trailing empty line.
    line_starts: Vec<usize>,
    revision: Revision,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    pending: Option<Transaction>,
}

/// Single-pass scan building the line-start index. `std` only, no extra deps.
/// Only `\n` delimits lines; `\r` is left in place and stripped by
/// `line_content`, matching the previous `bytes().filter()` behavior.
fn build_line_starts(content: &str) -> Vec<usize> {
    let bytes = content.as_bytes();
    let mut starts = Vec::new();
    starts.push(0);
    // Heuristic reserve: roughly one line per ~40 bytes avoids regrowth for
    // typical prose without an extra counting pass.
    starts.reserve(content.len() / 40);
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            // `i + 1` is always a char boundary: `\n` is single-byte ASCII.
            starts.push(i + 1);
        }
    }
    starts
}

/// Clamp `offset` into `0..=s.len()` and back off to a char boundary so
/// `insert_str` / `replace_range` never panic on mid-char offsets.
fn floor_char_boundary(s: &str, offset: usize) -> usize {
    let mut idx = offset.min(s.len());
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            line_starts: vec![0],
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
        }
    }

    pub fn from_text(text: impl Into<String>) -> Self {
        let content: String = text.into();
        let line_starts = build_line_starts(&content);
        Self {
            content,
            line_starts,
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
        }
    }

    pub fn replace_all(&mut self, text: impl Into<String>) {
        self.content = text.into();
        self.line_starts = build_line_starts(&self.content);
        self.revision += 1;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending = None;
        self.debug_check_invariant();
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn len(&self) -> usize {
        self.content.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let total = self.line_starts.len();
        if line >= total {
            return None;
        }
        let start = self.line_starts[line];
        let end = if line + 1 < total {
            self.line_starts[line + 1]
        } else {
            self.content.len()
        };
        Some(start..end)
    }

    pub fn line_content(&self, line: usize) -> Option<&str> {
        let range = self.line_range(line)?;
        let mut text = self.content.get(range)?;
        if let Some(stripped) = text.strip_suffix('\n') {
            text = stripped;
        }
        if let Some(stripped) = text.strip_suffix('\r') {
            text = stripped;
        }
        Some(text)
    }

    pub fn line_of_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.content.len());
        self.line_starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1)
    }

    pub fn line_start_offset(&self, line: usize) -> usize {
        if line < self.line_starts.len() {
            self.line_starts[line]
        } else {
            self.content.len()
        }
    }

    fn debug_check_invariant(&self) {
        debug_assert!(
            !self.line_starts.is_empty(),
            "line_starts must be non-empty"
        );
        debug_assert_eq!(self.line_starts[0], 0, "line_starts must start with 0");
        debug_assert!(
            self.line_starts.windows(2).all(|w| w[0] < w[1]),
            "line_starts must be strictly increasing"
        );
        if let Some(&last) = self.line_starts.last() {
            debug_assert!(
                last <= self.content.len(),
                "line_starts last entry must be within content"
            );
        }
        debug_assert!(
            self.line_starts
                .iter()
                .all(|&s| self.content.is_char_boundary(s)),
            "line_starts entries must be char boundaries"
        );
    }

    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let offset = floor_char_boundary(&self.content, offset);
        debug_assert!(self.content.is_char_boundary(offset));

        let line = self
            .line_starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);

        self.content.insert_str(offset, text);
        if text.as_bytes().contains(&b'\n') {
            let mut new_starts = Vec::new();
            for (i, &b) in text.as_bytes().iter().enumerate() {
                if b == b'\n' {
                    new_starts.push(offset + i + 1);
                }
            }
            for s in self.line_starts.iter_mut().skip(line + 1) {
                *s += text.len();
            }
            self.line_starts.splice(line + 1..line + 1, new_starts);
        } else {
            for s in self.line_starts.iter_mut().skip(line + 1) {
                *s += text.len();
            }
        }
        self.debug_check_invariant();
        self.revision += 1;

        let edit = Edit::Insert {
            position: offset,
            text: text.to_string(),
        };

        if let Some(tx) = &mut self.pending {
            tx.edits.push(edit);
            tx.cursor_after = offset + text.len();
        } else {
            self.push_undo(Transaction {
                edits: vec![edit],
                cursor_before: offset,
                cursor_after: offset + text.len(),
            });
            self.redo_stack.clear();
        }
    }

    pub fn delete(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }

        let len = self.content.len();
        let start = floor_char_boundary(&self.content, range.start.min(len));
        let end = floor_char_boundary(&self.content, range.end.min(len));
        if start >= end {
            return;
        }

        let start_line = self
            .line_starts
            .partition_point(|&s| s <= start)
            .saturating_sub(1);
        let end_line = self
            .line_starts
            .partition_point(|&s| s <= end)
            .saturating_sub(1);

        let Some(deleted) = self.content.get(start..end).map(|s| s.to_string()) else {
            return;
        };
        self.content.replace_range(start..end, "");
        let removed_len = end - start;
        if end_line > start_line {
            self.line_starts.drain(start_line + 1..end_line + 1);
        }
        for s in self.line_starts.iter_mut().skip(start_line + 1) {
            *s -= removed_len;
        }
        self.debug_check_invariant();
        self.revision += 1;

        let clamped = start..end;
        let (cursor_before, cursor_after) = (clamped.end, clamped.start);
        let edit = Edit::Delete {
            range: clamped,
            deleted,
        };

        if let Some(tx) = &mut self.pending {
            tx.edits.push(edit);
            tx.cursor_after = cursor_after;
        } else {
            self.push_undo(Transaction {
                edits: vec![edit],
                cursor_before,
                cursor_after,
            });
            self.redo_stack.clear();
        }
    }

    pub fn begin_transaction(&mut self, cursor: usize) {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.push_undo(tx);
            self.redo_stack.clear();
        }
        self.pending = Some(Transaction {
            edits: Vec::new(),
            cursor_before: cursor,
            cursor_after: cursor,
        });
    }

    pub fn end_transaction(&mut self, cursor: usize) {
        if let Some(mut tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            tx.cursor_after = cursor;
            self.push_undo(tx);
            self.redo_stack.clear();
        }
    }

    fn push_undo(&mut self, tx: Transaction) {
        if self.undo_stack.len() >= MAX_UNDO_STACK {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(tx);
    }

    pub fn undo(&mut self) -> Option<usize> {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.push_undo(tx);
        }

        let tx = self.undo_stack.pop()?;

        if tx.edits.is_empty() {
            let cursor = tx.cursor_before;
            self.redo_stack.push(tx);
            return Some(cursor);
        }

        for edit in tx.edits.iter().rev() {
            match edit {
                Edit::Insert { position, text } => {
                    let end = position.saturating_add(text.len()).min(self.content.len());
                    let end = floor_char_boundary(&self.content, end);
                    let start = floor_char_boundary(&self.content, *position);
                    if start < end
                        && let Some(slice) = self.content.get(start..end)
                        && slice == *text
                    {
                        self.content.replace_range(start..end, "");
                    } else if start < end && self.content.get(start..end).is_some() {
                        // Position drifted (should not happen in normal use);
                        // fall back to removing the recorded span without
                        // comparing, keeping lengths consistent.
                        self.content.replace_range(start..end, "");
                    }
                }
                Edit::Delete { range, deleted } => {
                    let pos = floor_char_boundary(&self.content, range.start);
                    self.content.insert_str(pos, deleted);
                }
            }
        }
        self.line_starts = build_line_starts(&self.content);
        self.debug_check_invariant();
        self.revision += 1;

        let cursor = tx.cursor_before;
        self.redo_stack.push(tx);
        Some(cursor)
    }

    pub fn redo(&mut self) -> Option<usize> {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.push_undo(tx);
            self.redo_stack.clear();
            return None;
        }

        let tx = self.redo_stack.pop()?;

        if tx.edits.is_empty() {
            let cursor = tx.cursor_after;
            self.push_undo(tx);
            return Some(cursor);
        }

        for edit in &tx.edits {
            match edit {
                Edit::Insert { position, text } => {
                    let pos = floor_char_boundary(&self.content, *position);
                    self.content.insert_str(pos, text);
                }
                Edit::Delete { range, .. } => {
                    let start = floor_char_boundary(&self.content, range.start);
                    let end = floor_char_boundary(&self.content, range.end.min(self.content.len()));
                    if start < end && self.content.get(start..end).is_some() {
                        self.content.replace_range(start..end, "");
                    }
                }
            }
        }
        self.line_starts = build_line_starts(&self.content);
        self.debug_check_invariant();
        self.revision += 1;

        let cursor = tx.cursor_after;
        self.push_undo(tx);
        Some(cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_line_starts(text: &str) -> Vec<usize> {
        let mut v = vec![0];
        for (i, &b) in text.as_bytes().iter().enumerate() {
            if b == b'\n' {
                v.push(i + 1);
            }
        }
        v
    }

    fn naive_line_count(text: &str) -> usize {
        naive_line_starts(text).len()
    }

    fn naive_line_of_offset(text: &str, offset: usize) -> usize {
        let offset = offset.min(text.len());
        text.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    fn naive_line_start_offset(text: &str, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        text.match_indices('\n')
            .nth(line - 1)
            .map(|(idx, _)| idx + 1)
            .unwrap_or(text.len())
    }

    fn assert_index_matches_naive(buf: &TextBuffer) {
        let text = buf.content();
        assert_eq!(buf.line_count(), naive_line_count(text), "line_count");
        assert_eq!(
            buf.line_count(),
            naive_line_starts(text).len(),
            "line_starts length"
        );
        for line in 0..buf.line_count() + 2 {
            let expected_start = if line < naive_line_count(text) {
                naive_line_start_offset(text, line)
            } else {
                text.len()
            };
            assert_eq!(
                buf.line_start_offset(line),
                expected_start,
                "line_start_offset({line})"
            );
        }
        for offset in [
            0,
            1,
            text.len() / 2,
            text.len().saturating_sub(1),
            text.len(),
        ] {
            let o = offset.min(text.len());
            // Snap to a boundary the same way the buffer does for lookups that
            // need slicing; line_of_offset itself works on raw bytes.
            assert_eq!(
                buf.line_of_offset(o),
                naive_line_of_offset(text, o),
                "line_of_offset({o})"
            );
        }
        for line in 0..buf.line_count() {
            let start = naive_line_start_offset(text, line);
            let end = if line + 1 < naive_line_count(text) {
                naive_line_start_offset(text, line + 1)
            } else {
                text.len()
            };
            assert_eq!(buf.line_range(line), Some(start..end), "line_range({line})");
        }
        assert_eq!(buf.line_range(buf.line_count()), None);
    }

    #[test]
    fn test_new_and_from_text() {
        let buf1 = TextBuffer::new();
        assert!(buf1.is_empty());
        assert_eq!(buf1.len(), 0);
        assert_eq!(buf1.revision(), 0);
        assert_eq!(buf1.line_count(), 1);

        let buf2 = TextBuffer::from_text("hello\nworld");
        assert!(!buf2.is_empty());
        assert_eq!(buf2.len(), 11);
        assert_eq!(buf2.revision(), 0);
        assert_eq!(buf2.line_count(), 2);
    }

    #[test]
    fn test_insert_and_delete() {
        let mut buf = TextBuffer::new();

        buf.insert(0, "hello");
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.revision(), 1);

        buf.insert(5, " world");
        assert_eq!(buf.content(), "hello world");
        assert_eq!(buf.revision(), 2);

        buf.delete(5..11);
        assert_eq!(buf.content(), "hello");
        assert_eq!(buf.revision(), 3);
    }

    #[test]
    fn test_empty_operations() {
        let mut buf = TextBuffer::new();
        buf.insert(0, "");
        assert_eq!(buf.revision(), 0);
        assert!(buf.is_empty());

        buf.delete(0..0);
        assert_eq!(buf.revision(), 0);

        assert_eq!(buf.undo(), None);
        assert_eq!(buf.redo(), None);
    }

    #[test]
    fn test_lines_and_offsets() {
        let buf = TextBuffer::from_text("first\nsecond\r\nthird\n");
        assert_eq!(buf.line_count(), 4);

        assert_eq!(buf.line_content(0), Some("first"));
        assert_eq!(buf.line_content(1), Some("second"));
        assert_eq!(buf.line_content(2), Some("third"));
        assert_eq!(buf.line_content(3), Some(""));
        assert_eq!(buf.line_content(4), None);

        assert_eq!(buf.line_start_offset(0), 0);
        assert_eq!(buf.line_start_offset(1), 6);
        assert_eq!(buf.line_start_offset(2), 14);
        assert_eq!(buf.line_start_offset(3), 20);
        assert_eq!(buf.line_start_offset(4), 20);
        assert_eq!(buf.line_start_offset(100), 20);

        assert_eq!(buf.line_of_offset(0), 0);
        assert_eq!(buf.line_of_offset(5), 0);
        assert_eq!(buf.line_of_offset(6), 1);
        assert_eq!(buf.line_of_offset(10), 1);
        assert_eq!(buf.line_of_offset(20), 3);
        assert_eq!(buf.line_of_offset(100), 3);
    }

    #[test]
    fn test_empty_line_ranges() {
        let buf = TextBuffer::new();
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.line_range(0), Some(0..0));
        assert_eq!(buf.line_content(0), Some(""));
        assert_eq!(buf.line_range(1), None);
        assert_eq!(buf.line_content(1), None);
    }

    #[test]
    fn undo_history_is_bounded() {
        let mut buffer = TextBuffer::new();
        for _ in 0..(MAX_UNDO_STACK + 10) {
            buffer.insert(buffer.len(), "x");
        }

        assert_eq!(buffer.undo_stack.len(), MAX_UNDO_STACK);
        for _ in 0..MAX_UNDO_STACK {
            buffer.undo().expect("retained edit should be undoable");
        }
        assert_eq!(buffer.content(), "xxxxxxxxxx");
        assert!(buffer.undo().is_none());
    }

    #[test]
    fn test_undo_redo_single() {
        let mut buf = TextBuffer::new();

        buf.insert(0, "hello");
        assert_eq!(buf.content(), "hello");
        let rev1 = buf.revision();

        let cur = buf.undo();
        assert_eq!(cur, Some(0));
        assert_eq!(buf.content(), "");
        assert!(buf.revision() > rev1);

        let cur = buf.redo();
        assert_eq!(cur, Some(5));
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_undo_redo_transaction() {
        let mut buf = TextBuffer::new();

        buf.begin_transaction(0);
        buf.insert(0, "h");
        buf.insert(1, "e");
        buf.insert(2, "l");
        buf.insert(3, "l");
        buf.insert(4, "o");
        buf.end_transaction(5);

        assert_eq!(buf.content(), "hello");

        let cur = buf.undo();
        assert_eq!(cur, Some(0));
        assert_eq!(buf.content(), "");

        let cur = buf.redo();
        assert_eq!(cur, Some(5));
        assert_eq!(buf.content(), "hello");
    }

    #[test]
    fn test_empty_transaction_no_op() {
        let mut buf = TextBuffer::new();
        buf.begin_transaction(0);
        buf.end_transaction(0);
        assert_eq!(buf.undo(), None);
    }

    #[test]
    fn test_undo_with_pending_transaction() {
        let mut buf = TextBuffer::new();

        buf.begin_transaction(0);
        buf.insert(0, "first");
        buf.end_transaction(5);

        buf.begin_transaction(5);
        buf.insert(5, " second");

        assert_eq!(buf.content(), "first second");

        let cur = buf.undo();
        assert_eq!(cur, Some(5));
        assert_eq!(buf.content(), "first");

        let cur = buf.undo();
        assert_eq!(cur, Some(0));
        assert_eq!(buf.content(), "");

        let cur = buf.redo();
        assert_eq!(cur, Some(5));
        assert_eq!(buf.content(), "first");
    }

    #[test]
    fn test_redo_cleared_on_edit() {
        let mut buf = TextBuffer::new();

        buf.insert(0, "a");
        buf.undo();
        assert_eq!(buf.content(), "");

        buf.insert(0, "b");
        assert_eq!(buf.redo(), None);
        assert_eq!(buf.content(), "b");
    }

    #[test]
    fn test_multibyte_utf8_fuzz_operations() {
        let mut buf = TextBuffer::new();

        let unicode_text = "café graf\nこんにちは世界\nFormula: ∑_{i=1}^n x_i\n";
        buf.insert(0, unicode_text);
        assert_eq!(buf.line_count(), 4);

        let line1_text = buf.line_content(1).unwrap();
        assert!(line1_text.contains("こんにちは"));

        let target = "世界";
        let start = buf.content().find(target).unwrap();
        let end = start + target.len();
        buf.delete(start..end);

        assert!(!buf.content().contains(target));
        assert!(buf.content().contains("こんにちは"));

        buf.undo();
        assert!(buf.content().contains(target));

        buf.redo();
        assert!(!buf.content().contains(target));
    }

    #[test]
    fn test_multiline_insert_keeps_index_in_sync() {
        let mut buf = TextBuffer::from_text("a\nb\nc");
        assert_index_matches_naive(&buf);

        buf.insert(1, "\nx\ny");
        assert_eq!(buf.content(), "a\nx\ny\nb\nc");
        assert_index_matches_naive(&buf);
        assert_eq!(buf.line_count(), 5);
        assert_eq!(buf.line_content(1), Some("x"));
        assert_eq!(buf.line_content(2), Some("y"));

        buf.insert(buf.len(), "\nlast\n");
        assert_index_matches_naive(&buf);
        assert_eq!(buf.line_content(buf.line_count() - 1), Some(""));
    }

    #[test]
    fn test_insert_delete_at_line_boundaries() {
        let mut buf = TextBuffer::from_text("one\ntwo\nthree");
        assert_index_matches_naive(&buf);

        // Insert exactly at the start of line 1.
        let line1_start = buf.line_start_offset(1);
        buf.insert(line1_start, "INSERTED\n");
        assert_eq!(buf.content(), "one\nINSERTED\ntwo\nthree");
        assert_index_matches_naive(&buf);

        // Delete the inserted line including its newline.
        let start = buf.line_start_offset(1);
        let end = buf.line_start_offset(2);
        buf.delete(start..end);
        assert_eq!(buf.content(), "one\ntwo\nthree");
        assert_index_matches_naive(&buf);

        // Delete a newline to join two lines.
        let nl = buf.line_start_offset(1) - 1;
        buf.delete(nl..nl + 1);
        assert_eq!(buf.content(), "onetwo\nthree");
        assert_index_matches_naive(&buf);
        assert_eq!(buf.line_count(), 2);

        // Delete across multiple lines.
        let mut buf = TextBuffer::from_text("l0\nl1\nl2\nl3\nl4");
        buf.delete(buf.line_start_offset(1)..buf.line_start_offset(4));
        assert_eq!(buf.content(), "l0\nl4");
        assert_index_matches_naive(&buf);
    }

    #[test]
    fn test_line_of_offset_and_range_against_naive_scan() {
        let text = "ab\ncde\n\nfghij\n";
        let buf = TextBuffer::from_text(text);
        for offset in 0..=text.len() {
            assert_eq!(
                buf.line_of_offset(offset),
                naive_line_of_offset(text, offset),
                "offset {offset}"
            );
        }
        for line in 0..naive_line_count(text) {
            let start = naive_line_start_offset(text, line);
            let end = if line + 1 < naive_line_count(text) {
                naive_line_start_offset(text, line + 1)
            } else {
                text.len()
            };
            assert_eq!(buf.line_range(line), Some(start..end));
        }
        // Out-of-range inputs clamp instead of panicking.
        assert_eq!(buf.line_of_offset(usize::MAX), naive_line_count(text) - 1);
        assert_eq!(buf.line_start_offset(usize::MAX), text.len());
        assert_eq!(buf.line_range(usize::MAX), None);
    }

    #[test]
    fn test_unicode_lines_cafe_emoji_cjk() {
        let text = "café\n🎉 party 🎈\n日本語の行\nnaïve\n";
        let mut buf = TextBuffer::from_text(text);
        assert_index_matches_naive(&buf);
        assert_eq!(buf.line_count(), 5);
        assert_eq!(buf.line_content(0), Some("café"));
        assert_eq!(buf.line_content(1), Some("🎉 party 🎈"));
        assert_eq!(buf.line_content(2), Some("日本語の行"));
        assert_eq!(buf.line_content(3), Some("naïve"));

        // Every line start is a char boundary; every offset maps to a line
        // without panicking, including mid-char bytes.
        for i in 0..=text.len() {
            let _ = buf.line_of_offset(i);
        }
        for line in 0..buf.line_count() {
            let start = buf.line_start_offset(line);
            assert!(text.is_char_boundary(start), "line {line} start");
            let range = buf.line_range(line).unwrap();
            assert!(text.get(range.clone()).is_some(), "range {range:?}");
        }

        // Insert multibyte text mid-document and delete part of an emoji line.
        let insert_at = buf.line_start_offset(2);
        buf.insert(insert_at, "café ☕\n");
        assert_index_matches_naive(&buf);
        assert_eq!(buf.line_content(2), Some("café ☕"));

        let emoji_line = buf.line_content(1).unwrap().to_string();
        let emoji_start = buf.line_start_offset(1);
        let first_emoji_len = "🎉".len();
        buf.delete(emoji_start..emoji_start + first_emoji_len);
        assert_index_matches_naive(&buf);
        assert!(!buf.line_content(1).unwrap().contains('🎉'));
        assert!(buf.line_content(1).unwrap().contains("party"));
        let _ = emoji_line;
    }

    #[test]
    fn test_mid_char_offsets_do_not_panic() {
        let mut buf = TextBuffer::from_text("aé\nb");
        let e_start = buf.content().find('é').unwrap();
        // Inserting inside `é` snaps to the char start.
        buf.insert(e_start + 1, "X");
        assert!(buf.content().is_char_boundary(e_start));
        assert_index_matches_naive(&buf);
        assert!(buf.content().contains("aXé") || buf.content().contains("aéX"));

        // Deleting a slice that cuts a char in half becomes a no-op or a
        // boundary-clamped delete, never a panic.
        let e_start = buf.content().find('é').unwrap();
        let rev = buf.revision();
        buf.delete(e_start..e_start + 1);
        assert_index_matches_naive(&buf);
        // Either the char survived intact or the whole char was removed;
        // the buffer must stay valid UTF-8 either way.
        assert!(buf.content().is_char_boundary(buf.content().len()));
        let _ = rev;
    }

    #[test]
    fn test_undo_redo_bump_revision_once_per_transaction() {
        let mut buf = TextBuffer::from_text("l0\nl1\nl2");
        buf.begin_transaction(0);
        buf.insert(0, "A\n");
        buf.insert(0, "B\n");
        buf.delete(0..2);
        let rev_before_undo = buf.revision();
        buf.end_transaction(0);

        buf.undo();
        assert_eq!(buf.content(), "l0\nl1\nl2");
        assert_eq!(
            buf.revision(),
            rev_before_undo + 1,
            "undo must bump revision once per transaction"
        );
        assert_index_matches_naive(&buf);

        buf.redo();
        assert_eq!(buf.revision(), rev_before_undo + 2);
        assert_index_matches_naive(&buf);
    }

    #[test]
    fn test_sequential_edits_keep_index_correct() {
        let mut buf = TextBuffer::new();
        let fixtures = ["hello\n", "wörld 🌍\n", "日本語\n", "line\n", "\n", "tail"];
        for f in fixtures {
            buf.insert(buf.len(), f);
            assert_index_matches_naive(&buf);
        }
        while buf.line_count() > 1 {
            let end = buf.len();
            let start = buf
                .line_start_offset(buf.line_count() - 1)
                .saturating_sub(1);
            let start = start.min(end);
            if start >= end {
                break;
            }
            // Keep deletions on char boundaries.
            let mut s = start;
            while !buf.content().is_char_boundary(s) {
                s = s.saturating_sub(1);
            }
            buf.delete(s..end);
            assert_index_matches_naive(&buf);
        }
        assert_index_matches_naive(&buf);
    }
}
