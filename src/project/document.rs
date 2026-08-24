use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::editor::buffer::TextBuffer;

use super::persistence::atomic_write;

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentId(pub u64);

impl DocumentId {
    pub fn next() -> Self {
        Self(NEXT_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Document {
    id: DocumentId,
    path: Option<PathBuf>,
    title: String,
    buffer: TextBuffer,
    saved_revision: u64,
    saved_content: String,
}

impl Document {
    pub fn new_untitled(title: impl Into<String>, initial_text: impl Into<String>) -> Self {
        let initial_text = initial_text.into();
        let buffer = TextBuffer::from_text(initial_text.clone());
        let saved_rev = buffer.revision();
        Self {
            id: DocumentId::next(),
            path: None,
            title: title.into(),
            buffer,
            saved_revision: saved_rev,
            saved_content: initial_text,
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let content = fs::read_to_string(&path)?;
        let title = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document")
            .to_string();

        let buffer = TextBuffer::from_text(content.clone());
        let saved_revision = buffer.revision();

        Ok(Self {
            id: DocumentId::next(),
            path: Some(path),
            title,
            buffer,
            saved_revision,
            saved_content: content,
        })
    }

    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn buffer(&self) -> &TextBuffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut TextBuffer {
        &mut self.buffer
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.revision() != self.saved_revision
    }

    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> std::io::Result<()> {
        let path = path.into();
        atomic_write(&path, self.buffer.content().as_bytes())?;
        self.title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("document")
            .to_string();
        self.path = Some(path);
        self.saved_revision = self.buffer.revision();
        self.saved_content = self.buffer.content().to_string();
        Ok(())
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot save document without a path",
            ));
        };
        let disk_content = fs::read_to_string(path)?;
        if disk_content != self.saved_content {
            return Err(std::io::Error::other(
                "file changed on disk; reopen it before saving",
            ));
        }
        atomic_write(path, self.buffer.content().as_bytes())?;
        self.saved_revision = self.buffer.revision();
        self.saved_content = self.buffer.content().to_string();
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

        let len = doc.buffer().len();
        doc.buffer_mut().insert(len, "\\begin{document}\n");
        assert!(doc.is_dirty());

        doc.save().unwrap();
        assert!(!doc.is_dirty());

        let reloaded = fs::read_to_string(&file_path).unwrap();
        assert_eq!(reloaded, "\\documentclass{article}\n\\begin{document}\n");
    }

    #[test]
    fn refuses_to_overwrite_external_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.tex");
        fs::write(&path, "first").unwrap();
        let mut document = Document::open(&path).unwrap();
        document.buffer_mut().insert(5, " local");
        fs::write(&path, "external").unwrap();

        let error = document.save().unwrap_err();

        assert!(error.to_string().contains("changed on disk"));
        assert_eq!(fs::read_to_string(path).unwrap(), "external");
    }

    #[test]
    fn saves_untitled_document_to_a_new_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("notes.tex");
        let mut document = Document::new_untitled("untitled.tex", "hello");

        document.save_as(&path).unwrap();

        assert_eq!(document.path(), Some(path.as_path()));
        assert_eq!(document.title(), "notes.tex");
        assert_eq!(fs::read_to_string(path).unwrap(), "hello");
        assert!(!document.is_dirty());
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
