//! Open document model, dirty state tracking, and disk persistence (spec §50–52).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::editor::buffer::TextBuffer;

use super::persistence::atomic_write;

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

/// Unique identifier for an open document in the workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

impl DocumentId {
    /// Generates a new unique document ID.
    pub fn next() -> Self {
        Self(NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// An open document managed by the workspace.
pub struct Document {
    id: DocumentId,
    path: Option<PathBuf>,
    title: String,
    buffer: TextBuffer,
    saved_revision: u64,
}

impl Document {
    /// Creates a new in-memory document with initial text.
    pub fn new_untitled(title: impl Into<String>, initial_text: impl Into<String>) -> Self {
        let buffer = TextBuffer::from_text(initial_text);
        let saved_rev = buffer.revision();
        Self {
            id: DocumentId::next(),
            path: None,
            title: title.into(),
            buffer,
            saved_revision: saved_rev,
        }
    }

    /// Opens a document from the filesystem.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)?;
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document")
            .to_string();

        let buffer = TextBuffer::from_text(content);
        let saved_revision = buffer.revision();

        Ok(Self {
            id: DocumentId::next(),
            path: Some(path),
            title,
            buffer,
            saved_revision,
        })
    }

    /// Returns the unique ID of the document.
    pub fn id(&self) -> DocumentId {
        self.id
    }

    /// Returns the path of the document, if it exists on disk.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Returns the title of the document.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns a reference to the text buffer.
    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    /// Returns a mutable reference to the text buffer.
    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    /// Checks if the document has unsaved modifications.
    pub fn is_dirty(&self) -> bool {
        self.buffer.revision() != self.saved_revision
    }

    /// Saves the document content back to its disk path atomically.
    pub fn save(&mut self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot save document without a path",
            ));
        };
        atomic_write(path, self.buffer.content().as_bytes())?;
        self.saved_revision = self.buffer.revision();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_dirty_tracking_and_save() {
        let temp = tempfile::tempdir().unwrap();
        let file_path = temp.path().join("test.tex");

        fs::write(&file_path, "\\documentclass{article}\n").unwrap();

        let mut doc = Document::open(&file_path).unwrap();
        assert_eq!(doc.title(), "test.tex");
        assert!(!doc.is_dirty());

        // Make an edit
        let len = doc.buffer().len();
        doc.buffer_mut().insert(len, "\\begin{document}\n");
        assert!(doc.is_dirty());

        // Save
        doc.save().unwrap();
        assert!(!doc.is_dirty());

        // Verify disk content
        let reloaded = fs::read_to_string(&file_path).unwrap();
        assert_eq!(reloaded, "\\documentclass{article}\n\\begin{document}\n");
    }

    #[test]
    fn test_untitled_document() {
        let mut doc = Document::new_untitled("untitled.tex", "Hello world");
        assert_eq!(doc.title(), "untitled.tex");
        assert!(!doc.is_dirty());

        doc.buffer_mut().insert(0, "Prefix: ");
        assert!(doc.is_dirty());
    }
}
