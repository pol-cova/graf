//! Unified diagnostic types and parsing.
//!
//! Represents errors, warnings, and messages from document engines (spec §38–41).

use std::path::PathBuf;

/// Unique identifier for a diagnostic item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticId(pub u64);

/// The severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

/// The origin subsystem that generated the diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSource {
    Tectonic,
    Typst,
    Parser,
    Ai,
}

/// A single compiler or parser diagnostic item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub id: DiagnosticId,
    pub severity: Severity,
    pub source: DiagnosticSource,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    /// Create a new diagnostic item.
    pub fn new(
        id: u64,
        severity: Severity,
        source: DiagnosticSource,
        file: Option<PathBuf>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: DiagnosticId(id),
            severity,
            source,
            file,
            line,
            message: message.into(),
        }
    }
}
