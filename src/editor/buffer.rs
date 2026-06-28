//! Text buffer with operation-based undo/redo.

use std::ops::Range;

/// A unique revision counter. Increments on every content change.
pub type Revision = u64;

/// An atomic edit operation.
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

/// A group of edits that form one undo step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    edits: Vec<Edit>,
    cursor_before: usize,
    cursor_after: usize,
}

/// A text buffer backed by a contiguous `String` with undo/redo history.
pub struct TextBuffer {
    content: String,
    revision: Revision,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    pending: Option<Transaction>,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    /// Creates a new, empty buffer.
    pub fn new() -> Self {
        Self {
            content: String::new(),
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
        }
    }

    /// Creates a buffer with initial content.
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            content: text.into(),
            revision: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pending: None,
        }
    }

    /// Replaces the entire buffer contents and updates the revision.
    pub fn replace_all(&mut self, text: impl Into<String>) {
        self.content = text.into();
        self.revision += 1;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending = None;
    }

    /// Returns the full content of the buffer.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns the current revision of the buffer.
    pub fn revision(&self) -> Revision {
        self.revision
    }

    /// Returns the byte length of the content.
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Returns true if the buffer is empty.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Returns the number of lines in the buffer (always at least 1).
    pub fn line_count(&self) -> usize {
        self.content.bytes().filter(|&b| b == b'\n').count() + 1
    }

    /// Returns the byte range of the given line, including the newline if present.
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

    /// Returns the content of the given line, excluding the newline character(s).
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

    /// Returns the line index that contains the given byte offset.
    pub fn line_of_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.content.len());
        self.content.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count()
    }

    /// Returns the byte offset at the start of the given line.
    pub fn line_start_offset(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        self.content
            .match_indices('\n')
            .nth(line - 1)
            .map(|(idx, _)| idx + 1)
            .unwrap_or(self.content.len())
    }

    /// Inserts text at the given offset, records the edit, and bumps revision.
    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        self.content.insert_str(offset, text);
        self.revision += 1;

        let edit = Edit::Insert {
            position: offset,
            text: text.to_string(),
        };

        if let Some(tx) = &mut self.pending {
            tx.edits.push(edit);
            tx.cursor_after = offset + text.len();
        } else {
            self.undo_stack.push(Transaction {
                edits: vec![edit],
                cursor_before: offset,
                cursor_after: offset + text.len(),
            });
            self.redo_stack.clear();
        }
    }

    /// Deletes the given byte range, records the edit, and bumps revision.
    pub fn delete(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }

        let deleted = self.content[range.clone()].to_string();
        self.content.replace_range(range.clone(), "");
        self.revision += 1;

        let (cursor_before, cursor_after) = (range.end, range.start);
        let edit = Edit::Delete { range, deleted };

        if let Some(tx) = &mut self.pending {
            tx.edits.push(edit);
            tx.cursor_after = cursor_after;
        } else {
            self.undo_stack.push(Transaction {
                edits: vec![edit],
                cursor_before,
                cursor_after,
            });
            self.redo_stack.clear();
        }
    }

    /// Starts grouping edits into a transaction.
    pub fn begin_transaction(&mut self, cursor: usize) {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.undo_stack.push(tx);
            self.redo_stack.clear();
        }
        self.pending = Some(Transaction {
            edits: Vec::new(),
            cursor_before: cursor,
            cursor_after: cursor,
        });
    }

    /// Finishes grouping edits, pushes to undo stack, and clears redo stack.
    pub fn end_transaction(&mut self, cursor: usize) {
        if let Some(mut tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            tx.cursor_after = cursor;
            self.undo_stack.push(tx);
            self.redo_stack.clear();
        }
    }

    /// Undoes the top transaction, returning the cursor position before the transaction.
    pub fn undo(&mut self) -> Option<usize> {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.undo_stack.push(tx);
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

        let cursor = tx.cursor_before;
        self.redo_stack.push(tx);
        Some(cursor)
    }

    /// Redoes the top transaction on the redo stack, returning the new cursor position.
    pub fn redo(&mut self) -> Option<usize> {
        if let Some(tx) = self.pending.take()
            && !tx.edits.is_empty()
        {
            self.undo_stack.push(tx);
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

        let cursor = tx.cursor_after;
        self.undo_stack.push(tx);
        Some(cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Multi-byte Unicode: Japanese, Emojis, Accents, Math symbols
        let sample = "🦀 Rust ⚡ Graf\nこんにちは世界 🌸\nFormula: ∑_{i=1}^n x_i\n";
        buf.insert(0, sample);
        assert_eq!(buf.line_count(), 4);

        // Delete multibyte segment inside line 1 (emoji & Japanese)
        let _line1_range = buf.line_range(1).unwrap();
        let line1_text = buf.line_content(1).unwrap();
        assert!(line1_text.contains("こんにちは"));

        // Delete "世界 "
        let target = "世界 ";
        let start = buf.content().find(target).unwrap();
        let end = start + target.len();
        buf.delete(start..end);

        assert!(!buf.content().contains("世界 "));
        assert!(buf.content().contains("こんにちは"));

        // Undo deletion
        buf.undo();
        assert!(buf.content().contains("世界 "));

        // Redo deletion
        buf.redo();
        assert!(!buf.content().contains("世界 "));
    }
}
