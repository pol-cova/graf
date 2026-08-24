use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSource {
    Tectonic,
    Typst,
    Parser,
    Ai,
}

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
