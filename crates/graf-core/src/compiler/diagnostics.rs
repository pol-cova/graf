use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSource {
    Tectonic,
    Typst,
    Parser,
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

    /// Builds a parser-grade warning from a style-linter finding. The
    /// mapping lives here (not in `project::linter`) so project code never
    /// assembles compiler structs by hand, and so every style warning draws
    /// its id from the same process-wide sequence as engine diagnostics.
    pub fn from_style_warning(line: usize, message: impl Into<String>) -> Self {
        Self {
            id: super::engine::next_diagnostic_id(),
            severity: Severity::Warning,
            source: DiagnosticSource::Parser,
            file: None,
            line: Some(line),
            message: message.into(),
        }
    }
}
