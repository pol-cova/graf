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
    revision: Revision,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    pending: Option<Transaction>,
    /// Byte offsets where each line starts, always sorted and starting at 0.
    /// Maintained incrementally so line lookups are O(log n) instead of
    /// scanning the whole document per keypress.
    line_starts: Vec<usize>,
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
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
            line_starts: vec![0],
        }
    }

    pub fn from_text(text: impl Into<String>) -> Self {
        let content = text.into();
        let line_starts = compute_line_starts(&content);
        Self {
            content,
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
            line_starts,
        }
    }

    pub fn replace_all(&mut self, text: impl Into<String>) {
        self.content = text.into();
        self.revision += 1;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending = None;
        self.line_starts = compute_line_starts(&self.content);
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    /// The largest byte offset `offset` that is a char boundary, walking
    /// back; the canonical clamp used by every slicing caller in the module.
    #[cfg(test)]
    pub fn clamp_char_boundary(&self, offset: usize) -> usize {
        clamp_str_boundary(&self.content, offset)
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
        if line >= self.line_count() {
            return None;
        }
        let start = self.line_start_offset(line);
        let end = if line + 1 < self.line_count() {
            self.line_start_offset(line + 1)
        } else {
            self.content.len()
        };
        Some(start..end)
    }

    pub fn line_content(&self, line: usize) -> Option<&str> {
        let range = self.line_range(line)?;
        let mut text = &self.content[range];
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
        // The line number is the count of starts <= offset, minus one.
        self.line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }

    pub fn line_start_offset(&self, line: usize) -> usize {
        self.line_starts
            .get(line)
            .copied()
            .unwrap_or(self.content.len())
    }

    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        self.content.insert_str(offset, text);
        self.revision += 1;
        self.update_line_starts_for_insert(offset, text);

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

        let deleted = self.content[range.clone()].to_string();
        self.content.replace_range(range.clone(), "");
        self.revision += 1;
        self.update_line_starts_for_delete(range.start, range.end);

        let (cursor_before, cursor_after) = (range.end, range.start);
        let edit = Edit::Delete { range, deleted };

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

    fn update_line_starts_for_insert(&mut self, position: usize, text: &str) {
        // Entries at/before the insertion point keep their offsets, the
        // inserted text's newlines introduce new line starts between them and
        // the shifted tail, all strictly after the original entries.
        let split = self.line_starts.partition_point(|&start| start <= position);
        let new_starts: Vec<usize> = text
            .bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(i, _)| position + i + 1)
            .collect();
        let mut line_starts = Vec::with_capacity(self.line_starts.len() + new_starts.len());
        line_starts.extend_from_slice(&self.line_starts[..split]);
        line_starts.extend(new_starts.iter().copied());
        line_starts.extend(
            self.line_starts[split..]
                .iter()
                .map(|start| start + text.len()),
        );
        self.line_starts = line_starts;
    }

    fn update_line_starts_for_delete(&mut self, start: usize, end: usize) {
        // Starts inside the deleted range vanish. A start exactly at `end`
        // only survives when a newline still precedes the mapped position
        // (its own preceding newline lived inside the deleted chunk).
        // Duplicates collapse.
        let removed_len = end - start;
        let mut line_starts = Vec::with_capacity(self.line_starts.len());
        let mut last_pushed = usize::MAX;
        for &line_start in &self.line_starts {
            if line_start <= start {
                if line_start != last_pushed {
                    line_starts.push(line_start);
                    last_pushed = line_start;
                }
            } else if line_start < end {
                // Line start pointed inside the deleted text.
            } else {
                let mapped = line_start - removed_len;
                // A start that moved onto `start` (only when it sat exactly
                // at `end`) stays a line start only if a newline still
                // precedes it; the deleted chunk may have erased both its
                // preceding newline and the line itself.
                let still_line_start =
                    line_start != end || start == 0 || self.content.as_bytes()[start - 1] == b'\n';
                if mapped != last_pushed && still_line_start {
                    line_starts.push(mapped);
                    last_pushed = mapped;
                }
            }
        }
        if line_starts.is_empty() {
            line_starts.push(0);
        }
        self.line_starts = line_starts;
    }

    fn rebuild_line_index(&mut self) {
        self.line_starts = compute_line_starts(&self.content);
    }

    pub fn undo(&mut self) -> Option<usize> {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.push_undo(tx);
        }

        let tx = self.undo_stack.pop()?;

        for edit in tx.edits.iter().rev() {
            match edit {
                Edit::Insert { position, text } => {
                    let end = *position + text.len();
                    self.content.replace_range(*position..end, "");
                }
                Edit::Delete { range, deleted } => {
                    self.content.insert_str(range.start, deleted);
                }
            }
            self.revision += 1;
        }
        self.rebuild_line_index();

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

        for edit in &tx.edits {
            match edit {
                Edit::Insert { position, text } => {
                    self.content.insert_str(*position, text);
                }
                Edit::Delete { range, .. } => {
                    self.content.replace_range(range.clone(), "");
                }
            }
            self.revision += 1;
        }
        self.rebuild_line_index();

        let cursor = tx.cursor_after;
        self.push_undo(tx);
        Some(cursor)
    }
}

/// Free-standing clamp for callers holding a `&str` without a buffer.
pub fn clamp_str_boundary(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while offset > 0 && !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// Byte offsets of every line start in `content` (always including 0).
fn compute_line_starts(content: &str) -> Vec<usize> {
    let mut starts: Vec<usize> = std::iter::once(0)
        .chain(
            content
                .bytes()
                .enumerate()
                // items are `(usize, u8)` by value in `filter`.
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(i, _)| i + 1),
        )
        .collect();
    starts.shrink_to_fit();
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference implementation the incremental index must always match.
    fn expected_starts(content: &str) -> Vec<usize> {
        compute_line_starts(content)
    }

    fn assert_index_matches(buffer: &TextBuffer) {
        assert_eq!(
            buffer.line_starts,
            expected_starts(buffer.content()),
            "line index diverged for content {:?}",
            buffer.content()
        );
    }

    #[test]
    fn line_index_tracks_incremental_edits() {
        let mut buffer = TextBuffer::from_text("first\nsecond\nthird");
        assert_index_matches(&buffer);

        buffer.insert(6, "zero\n");
        assert_index_matches(&buffer);

        buffer.insert(buffer.len(), "\nplus");
        assert_index_matches(&buffer);

        buffer.delete(0..6);
        assert_index_matches(&buffer);

        buffer.delete(3..8);
        assert_index_matches(&buffer);

        buffer.insert(buffer.len(), "\n");
        assert_index_matches(&buffer);

        buffer.delete(buffer.len().saturating_sub(2)..buffer.len());
        assert_index_matches(&buffer);

        buffer.replace_all("a\nb\nc");
        assert_index_matches(&buffer);
        assert_eq!(buffer.line_count(), 3);
    }

    #[test]
    fn line_index_survives_unicode_edits() {
        let mut buffer = TextBuffer::from_text("日本語\nemoji 🎉\nİstanbul");
        assert_index_matches(&buffer);

        // Delete inside the first line ('日' is 3 bytes).
        buffer.delete(0..3);
        assert_index_matches(&buffer);

        // Content is "本語\nemoji 🎉\nİstanbul"; insert at the line-1 start.
        buffer.insert(7, "x\ny");
        assert_index_matches(&buffer);

        buffer.delete(7..8);
        assert_index_matches(&buffer);
    }

    #[test]
    fn line_index_maintained_through_undo_redo() {
        let mut buffer = TextBuffer::from_text("one\ntwo");
        buffer.insert(3, "\nthree");
        assert_index_matches(&buffer);

        buffer.undo();
        assert_index_matches(&buffer);

        buffer.redo();
        assert_index_matches(&buffer);
    }

    #[test]
    fn line_queries_match_old_semantics() {
        let buffer = TextBuffer::from_text("alpha\nbeta\ngamma");
        assert_eq!(buffer.line_count(), 3);
        assert_eq!(buffer.line_of_offset(0), 0);
        assert_eq!(buffer.line_of_offset(6), 1); // at 'beta' start
        assert_eq!(buffer.line_of_offset(5), 0); // just before newline
        assert_eq!(buffer.line_of_offset(11), 2); // at 'gamma' start        assert_eq!(buffer.line_start_offset(0), 0);
        assert_eq!(buffer.line_start_offset(2), 11);
        assert_eq!(buffer.line_start_offset(99), buffer.len());
        assert_eq!(buffer.line_content(1), Some("beta"));
        assert_eq!(buffer.line_range(1), Some(6..11));
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
}

#[cfg(test)]
mod clamp_tests {
    use super::*;

    #[test]
    fn clamp_walks_back_to_a_character_edge() {
        let buffer = TextBuffer::from_text("日本語");
        assert_eq!(buffer.clamp_char_boundary(4), 3);
        assert_eq!(buffer.clamp_char_boundary(9), 9);
        assert_eq!(buffer.clamp_char_boundary(100), 9);
        assert_eq!(buffer.clamp_char_boundary(0), 0);
        assert_eq!(clamp_str_boundary("ab", 5), 2);
    }
}
