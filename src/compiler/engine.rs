use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::diagnostics::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompileId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    Pdf,
    Svg,
    Html,
}

#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub compile_id: CompileId,
    pub source: String,
    pub revision: u64,
    pub project_root: Option<PathBuf>,
    pub root_document: Option<PathBuf>,
}

impl CompileRequest {
    fn next_id() -> CompileId {
        static NEXT_COMPILE_ID: AtomicU64 = AtomicU64::new(1);
        CompileId(NEXT_COMPILE_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn simple(source: impl Into<String>, revision: u64) -> Self {
        Self {
            compile_id: Self::next_id(),
            source: source.into(),
            revision,
            project_root: None,
            root_document: None,
        }
    }

    pub fn with_project(
        source: impl Into<String>,
        revision: u64,
        project_root: Option<PathBuf>,
        root_document: Option<PathBuf>,
    ) -> Self {
        Self {
            compile_id: Self::next_id(),
            source: source.into(),
            revision,
            project_root,
            root_document,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompileOutput {
    pub compile_id: CompileId,
    pub revision: u64,
    pub artifact: Vec<u8>,
    pub artifact_kind: ArtifactKind,
    pub diagnostics: Vec<Diagnostic>,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct CompileError {
    pub compile_id: CompileId,
    pub revision: u64,
    pub diagnostics: Vec<Diagnostic>,
    pub message: String,
    pub duration: Duration,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Compilation failed (rev {}): {}",
            self.revision, self.message
        )
    }
}

impl std::error::Error for CompileError {}

pub trait DocumentEngine: Send + Sync {
    fn compile(&self, request: CompileRequest) -> Result<CompileOutput, CompileError>;
}
