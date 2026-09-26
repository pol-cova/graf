//! Swift-facing API for `graf-core`.
//!
//! The surface is deliberately coarse: Swift owns the live text and hands the
//! core a snapshot when it compiles, lints, or builds the outline. Every call
//! here that touches the file system or runs a compiler blocks, so Swift must
//! call it off the main actor.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use graf_core::compiler::controller::CompilerController;
use graf_core::compiler::diagnostics::{self, Diagnostic as CoreDiagnostic};
use graf_core::compiler::engine::{CompileRequest, DocumentEngine};
use graf_core::compiler::tectonic::TectonicEngine;
use graf_core::compiler::typst::TypstEngine;
use graf_core::project::{bibtex, linter, outline, stats, templates};

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Engine {
    Latex,
    Typst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Diagnostic {
    pub severity: Severity,
    pub file: Option<String>,
    /// One-based source line, when the backend reported one.
    pub line: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CompileInput {
    pub engine: Engine,
    /// Snapshot of the edited document. Ignored when `root_document` is set,
    /// because the project's root file on disk drives the compile.
    pub text: String,
    pub revision: u64,
    pub project_root: Option<String>,
    pub root_document: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct CompileSuccess {
    pub revision: u64,
    pub pdf: Vec<u8>,
    /// Warnings from a successful build.
    pub diagnostics: Vec<Diagnostic>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, uniffi::Error)]
pub enum CompileFailure {
    /// The backend ran and reported errors, or could not run at all. The
    /// preview must keep showing the last good PDF.
    Failed {
        revision: u64,
        message: String,
        diagnostics: Vec<Diagnostic>,
        duration_ms: u64,
    },
    /// A newer edit arrived while this build ran. Drop the result.
    Stale {
        completed_revision: u64,
        current_revision: u64,
    },
}

impl std::fmt::Display for CompileFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed {
                revision, message, ..
            } => write!(f, "compile failed (rev {revision}): {message}"),
            Self::Stale {
                completed_revision,
                current_revision,
            } => write!(
                f,
                "stale result: rev {completed_revision} finished after rev {current_revision}"
            ),
        }
    }
}

impl std::error::Error for CompileFailure {}

#[derive(Debug, Clone, uniffi::Error)]
pub enum FileError {
    Io { path: String, message: String },
    UnknownTemplate { id: String },
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{path}: {message}"),
            Self::UnknownTemplate { id } => write!(f, "unknown template {id}"),
        }
    }
}

impl std::error::Error for FileError {}

impl FileError {
    fn io(path: &Path, error: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            message: error.to_string(),
        }
    }
}

/// Owns both engines and the revision bookkeeping. One per window.
#[derive(uniffi::Object)]
pub struct Compiler {
    tectonic: TectonicEngine,
    typst: TypstEngine,
    controller: Mutex<CompilerController>,
    in_flight: Mutex<Option<Arc<AtomicBool>>>,
}

#[uniffi::export]
impl Compiler {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tectonic: TectonicEngine::new(),
            typst: TypstEngine::new(),
            // Swift debounces edits; the controller only tracks revisions.
            controller: Mutex::new(CompilerController::with_debounce(Duration::ZERO)),
            in_flight: Mutex::new(None),
        })
    }

    /// Resolves the compiler executables so the first real build is fast.
    /// Blocks while probing; call once from a background task.
    pub fn warm_up(&self) {
        self.tectonic.warm_up();
        self.typst.warm_up();
    }

    /// Records a newer revision of the text and cancels any build that is
    /// now out of date.
    pub fn source_edited(&self, revision: u64) {
        lock(&self.controller).on_source_edited(revision);
        if let Some(cancel) = lock(&self.in_flight).take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Builds the document. Blocks until the backend finishes; call from a
    /// background task.
    pub fn compile(&self, input: CompileInput) -> Result<CompileSuccess, CompileFailure> {
        let current_revision = lock(&self.controller).current_revision();
        if input.revision < current_revision {
            return Err(CompileFailure::Stale {
                completed_revision: input.revision,
                current_revision,
            });
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let request = CompileRequest::with_project(
            input.text,
            input.revision,
            input.project_root.map(PathBuf::from),
            input.root_document.map(PathBuf::from),
        )
        .with_cancel(cancel.clone());

        lock(&self.controller).begin_compile(request.compile_id, request.revision);
        if let Some(previous) = lock(&self.in_flight).replace(cancel) {
            previous.store(true, Ordering::Relaxed);
        }

        let result = match input.engine {
            Engine::Latex => self.tectonic.compile(request),
            Engine::Typst => self.typst.compile(request),
        };

        let mut controller = lock(&self.controller);
        match result {
            Ok(output) => {
                controller
                    .handle_output(&output)
                    .map_err(|stale| CompileFailure::Stale {
                        completed_revision: stale.completed_revision,
                        current_revision: stale.current_revision,
                    })?;
                Ok(CompileSuccess {
                    revision: output.revision,
                    pdf: output.artifact.to_vec(),
                    diagnostics: output.diagnostics.iter().map(Diagnostic::from).collect(),
                    duration_ms: millis(output.duration),
                })
            }
            Err(error) => {
                let failure = CompileFailure::Failed {
                    revision: error.revision,
                    message: error.message.clone(),
                    diagnostics: error.diagnostics.iter().map(Diagnostic::from).collect(),
                    duration_ms: millis(error.duration),
                };
                controller
                    .handle_error(error)
                    .map_err(|stale| CompileFailure::Stale {
                        completed_revision: stale.completed_revision,
                        current_revision: stale.current_revision,
                    })?;
                Err(failure)
            }
        }
    }
}

/// A poisoned lock only means another call panicked mid-update. The guarded
/// state is plain bookkeeping that stays consistent, so keep going.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl From<&CoreDiagnostic> for Diagnostic {
    fn from(diagnostic: &CoreDiagnostic) -> Self {
        Self {
            severity: match diagnostic.severity {
                diagnostics::Severity::Error => Severity::Error,
                diagnostics::Severity::Warning => Severity::Warning,
            },
            file: diagnostic
                .file
                .as_ref()
                .map(|path| path.display().to_string()),
            line: diagnostic.line.map(|line| line as u64),
            message: diagnostic.message.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OutlineItem {
    pub level: u64,
    pub title: String,
    pub line: u64,
}

/// Section outline of a LaTeX snapshot. Typst outlines are not parsed yet
/// and return an empty list.
#[uniffi::export]
pub fn outline(text: String, engine: Engine) -> Vec<OutlineItem> {
    match engine {
        Engine::Latex => outline::parse_latex_outline(&text)
            .into_iter()
            .map(|item| OutlineItem {
                level: item.level as u64,
                title: item.title,
                line: item.line_number as u64,
            })
            .collect(),
        Engine::Typst => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Stats {
    pub words: u64,
    pub characters: u64,
    pub equations: u64,
    pub citations: u64,
    pub reading_minutes: f32,
    pub estimated_pages: f32,
}

#[uniffi::export]
pub fn stats(text: String, engine: Engine) -> Stats {
    let stats = stats::DocumentStats::compute(&text, engine == Engine::Typst);
    Stats {
        words: stats.word_count as u64,
        characters: stats.char_count as u64,
        equations: stats.equation_count as u64,
        citations: stats.citation_count as u64,
        reading_minutes: stats.reading_time_mins,
        estimated_pages: stats.estimated_pages,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BibEntry {
    pub key: String,
    pub entry_type: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub year: Option<String>,
}

/// Entries of a `.bib` file on disk.
#[uniffi::export]
pub fn bib_entries(path: String) -> Result<Vec<BibEntry>, FileError> {
    let path = PathBuf::from(path);
    let content = std::fs::read_to_string(&path).map_err(|error| FileError::io(&path, error))?;
    Ok(bibtex::parse_bibtex_entries(&content)
        .into_iter()
        .map(|entry| BibEntry {
            key: entry.key,
            entry_type: entry.entry_type,
            title: entry.title,
            author: entry.author,
            year: entry.year,
        })
        .collect())
}

/// `\label{...}` keys defined in a LaTeX snapshot.
#[uniffi::export]
pub fn labels(text: String) -> Vec<String> {
    bibtex::parse_latex_labels(&text)
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StyleWarning {
    /// One-based line and zero-based column of the match.
    pub line: u64,
    pub column: u64,
    pub length: u64,
    pub message: String,
    pub suggestion: Option<String>,
}

#[uniffi::export]
pub fn lint(text: String, engine: Engine) -> Vec<StyleWarning> {
    linter::lint_academic_text(&text, engine == Engine::Typst)
        .into_iter()
        .map(|warning| StyleWarning {
            line: warning.line as u64,
            column: warning.col as u64,
            length: warning.length as u64,
            message: warning.message,
            suggestion: warning.suggestion,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: String,
    pub engine: Engine,
    pub file_name: String,
}

/// Built-in templates in picker order.
#[uniffi::export]
pub fn templates() -> Vec<Template> {
    templates::builtin_templates()
        .iter()
        .filter_map(|template| {
            let engine = match template.kind.as_engine()? {
                graf_core::compiler::EngineKind::Latex => Engine::Latex,
                graf_core::compiler::EngineKind::Typst => Engine::Typst,
            };
            Some(Template {
                id: template.id.to_string(),
                name: template.name.to_string(),
                description: template.description.to_string(),
                engine,
                file_name: template.file_name.to_string(),
            })
        })
        .collect()
}

/// Creates `directory` from a template and returns the root file's path.
/// Never overwrites an existing root file.
#[uniffi::export]
pub fn create_project(directory: String, template_id: String) -> Result<String, FileError> {
    let template = templates::template_by_id(&template_id)
        .ok_or(FileError::UnknownTemplate { id: template_id })?;
    let directory = PathBuf::from(directory);
    templates::scaffold_project(&directory, template)
        .map(|root| root.display().to_string())
        .map_err(|error| FileError::io(&directory, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_reports_latex_sections() {
        let items = outline(
            "\\section{Method}\ntext\n\\subsection{Participants}\n".to_string(),
            Engine::Latex,
        );
        let titles: Vec<_> = items.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, ["Method", "Participants"]);
        assert!(items[1].level > items[0].level);
    }

    #[test]
    fn every_template_maps_to_an_engine() {
        assert_eq!(
            templates().len(),
            templates::builtin_templates().len(),
            "a template without an engine would vanish from the picker"
        );
    }

    #[test]
    fn create_project_rejects_unknown_templates() {
        let temp = std::env::temp_dir().join("graf-ffi-unknown-template");
        let result = create_project(temp.display().to_string(), "nope".to_string());
        assert!(matches!(result, Err(FileError::UnknownTemplate { .. })));
    }

    #[test]
    fn missing_bib_file_is_an_io_error() {
        let result = bib_entries("/nonexistent/graf/refs.bib".to_string());
        assert!(matches!(result, Err(FileError::Io { .. })));
    }

    #[test]
    fn a_newer_edit_marks_a_finished_build_as_stale() {
        let compiler = Compiler::new();
        // Mark revision 2 as current before the rev-1 build reports back.
        compiler.source_edited(2);
        let result = compiler.compile(CompileInput {
            engine: Engine::Latex,
            text: "\\documentclass{article}\\begin{document}x\\end{document}".to_string(),
            revision: 1,
            project_root: None,
            root_document: None,
        });
        assert!(matches!(
            result,
            Err(CompileFailure::Stale {
                completed_revision: 1,
                current_revision: 2
            })
        ));
    }
}
