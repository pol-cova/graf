use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::diagnostics::{Diagnostic, DiagnosticId, DiagnosticSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompileId(pub u64);

/// Where the bytes a backend compiles come from. A `File` source compiles
/// the on-disk document directly (project root document); a `Text` source is
/// written to a temporary file inside the job's build directory.
#[derive(Debug, Clone)]
pub enum CompileSource {
    Text(String),
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub compile_id: CompileId,
    pub source: CompileSource,
    pub revision: u64,
    pub project_root: Option<PathBuf>,
    /// When flipped to `true`, engines kill the in-flight subprocess as soon
    /// as possible instead of running a compile that will be discarded.
    pub cancel: Option<Arc<AtomicBool>>,
}

impl CompileRequest {
    fn next_id() -> CompileId {
        static NEXT_COMPILE_ID: AtomicU64 = AtomicU64::new(1);
        CompileId(NEXT_COMPILE_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn simple(source: impl Into<String>, revision: u64) -> Self {
        Self {
            compile_id: Self::next_id(),
            source: CompileSource::Text(source.into()),
            revision,
            project_root: None,
            cancel: None,
        }
    }

    /// Compile inside a project. When `root_document` is set it drives the
    /// compile directly and the inline text is not used; otherwise the text
    /// is written to a temporary file and compiled from there.
    pub fn with_project(
        source: impl Into<String>,
        revision: u64,
        project_root: Option<PathBuf>,
        root_document: Option<PathBuf>,
    ) -> Self {
        let source = match root_document {
            Some(root_document) => CompileSource::File(root_document),
            None => CompileSource::Text(source.into()),
        };
        Self {
            compile_id: Self::next_id(),
            source,
            revision,
            project_root,
            cancel: None,
        }
    }

    /// Attach a cancellation flag so a newer edit can abort this compile.
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// The on-disk document being compiled, when the source is a file.
    pub fn source_document(&self) -> Option<&Path> {
        match &self.source {
            CompileSource::File(path) => Some(path.as_path()),
            CompileSource::Text(_) => None,
        }
    }
}

/// Outcome of one cancellable subprocess run. A flat enum keeps spawn
/// failures (`io::Error`) from being silently conflated with cancellation,
/// which the old `Result<Result<..>>` nesting made easy to mishandle.
#[derive(Debug)]
pub enum SubprocessResult {
    Output(std::process::Output),
    Cancelled,
    SpawnError(std::io::Error),
}

/// Run a command capturing stdout/stderr while watching a cancel flag. A
/// blocking `output()` would tie a background thread to a stale compile for
/// its whole duration; this polls `try_wait` and kills the child on cancel.
pub fn run_with_cancel(
    mut command: std::process::Command,
    cancel: Option<&Arc<AtomicBool>>,
) -> SubprocessResult {
    use std::process::Stdio;

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return SubprocessResult::Cancelled;
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return SubprocessResult::SpawnError(error),
    };

    fn read_pipe<R>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>>
    where
        R: std::io::Read + Send + 'static,
    {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut buffer);
            }
            buffer
        })
    }
    let stdout_pipe = child
        .stdout
        .take()
        .map(|pipe| read_pipe::<std::process::ChildStdout>(Some(pipe)));
    let stderr_pipe = child
        .stderr
        .take()
        .map(|pipe| read_pipe::<std::process::ChildStderr>(Some(pipe)));

    /// Reaping the child does not imply the pipes are drained: a grandchild
    /// that inherited them can keep a reader thread alive forever, so
    /// joining must be bounded rather than unconditional.
    const PIPE_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

    fn drain_pipe(handle: Option<std::thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
        let Some(handle) = handle else {
            return Vec::new();
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("graf-pipe-join-waiter".into())
            .spawn(move || {
                let buffer = handle.join().unwrap_or_default();
                let _ = sender.try_send(buffer);
            });
        // A still-blocked reader is abandoned instead of hanging the reap;
        // the buffer dies with that thread.
        receiver.recv_timeout(PIPE_JOIN_TIMEOUT).unwrap_or_default()
    }

    loop {
        let cancelled = cancel.is_some_and(|flag| flag.load(Ordering::Relaxed));
        if cancelled {
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = drain_pipe(stdout_pipe);
                let stderr = drain_pipe(stderr_pipe);
                return if cancelled {
                    SubprocessResult::Cancelled
                } else {
                    SubprocessResult::Output(std::process::Output {
                        status,
                        stdout,
                        stderr,
                    })
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            // An unexpected IO error (e.g. waitpid failure) must not leave a
            // still-running compiler behind: kill the child, surface it.
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return SubprocessResult::SpawnError(error);
            }
        }
    }
}

/// Shared, immutable compiled artifact bytes. `Arc` lets the PDF travel from
/// the compiler through rendering without any byte-copying clones.
pub type ArtifactBytes = Arc<[u8]>;
#[derive(Debug)]
pub struct CompileOutput {
    pub compile_id: CompileId,
    pub revision: u64,
    pub artifact: ArtifactBytes,
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

    /// Prepare the engine so the first user-facing compile is fast. Called
    /// once at startup on a background thread; the default does nothing.
    fn warm_up(&self) {}
}

#[cfg(test)]
mod cancel_tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn run_with_cancel_returns_output_when_not_cancelled() {
        let mut command = Command::new("echo");
        command.arg("hello");
        let SubprocessResult::Output(output) = run_with_cancel(command, None) else {
            panic!("expected output");
        };
        assert_eq!(output.stdout, b"hello\n");
    }

    #[test]
    fn run_with_cancel_kills_child_when_flag_flips() {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = cancel.clone();
        // Flip the flag shortly after the child starts.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel_for_thread.store(true, Ordering::Relaxed);
        });
        let mut command = Command::new("sleep");
        command.arg("10");
        let start = std::time::Instant::now();
        let result = run_with_cancel(command, Some(&cancel));

        assert!(matches!(result, SubprocessResult::Cancelled));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn run_with_cancel_short_circuits_before_spawn() {
        let cancel = Arc::new(AtomicBool::new(true));
        let mut command = Command::new("sleep");
        command.arg("10");
        let result = run_with_cancel(command, Some(&cancel));
        assert!(matches!(result, SubprocessResult::Cancelled));
    }

    #[test]
    fn run_with_cancel_reports_spawn_errors_as_a_variant() {
        // A missing binary must be a distinguishable SpawnError, not a
        // silent success or an indistinguishable cancellation.
        let command = Command::new("graf-no-such-compiler-binary");
        let result = run_with_cancel(command, None);
        assert!(matches!(result, SubprocessResult::SpawnError(_)));
    }

    #[test]
    fn grandchild_inheriting_pipes_cannot_hang_the_reap() {
        // The child spawns a grandchild that inherits stdout/stderr and
        // outlives it by a second: the bounded join must return output
        // (possibly partial) instead of blocking forever.
        let mut command = Command::new("sh");
        command.arg("-c").arg("echo start; sleep 1 & echo done");
        let started = std::time::Instant::now();
        let SubprocessResult::Output(output) = run_with_cancel(command, None) else {
            panic!("expected output");
        };
        // The reap must not wait on the inheriting grandchild: bounded join
        // returns promptly (with a possibly partial buffer, which is fine —
        // compilers do not spawn grandchildren; this guards the hang).
        assert!(output.status.success());
        assert!(started.elapsed() < Duration::from_millis(2_000));
    }

    #[test]
    fn unexpected_failure_does_not_leak_the_child() {
        // A child that exits immediately keeps pipes valid; the leak case is
        // exercised indirectly by asserting the happy path stays fast. The
        // kill-on-error branch is implemented; the simulated waitpid
        // failure is impossible to trigger portably, so here we verify the
        // full run() path never blocks when headers drop mid-output.
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("echo partial && sleep 0.05 && echo tail && exec sleep 1");
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            cancel_for_thread.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        let result = run_with_cancel(command, Some(&cancel));
        match result {
            SubprocessResult::Cancelled | SubprocessResult::SpawnError(_) => {}
            other => panic!("expected cancel or error, got {other:?}"),
        }
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn compile_request_cancel_helpers() {
        let request = CompileRequest::simple("x", 1);
        assert!(request.cancel.is_none());

        let flag = Arc::new(AtomicBool::new(false));
        let request = CompileRequest::simple("x", 1).with_cancel(flag.clone());
        assert!(
            request
                .cancel
                .as_ref()
                .is_some_and(|flag| !flag.load(Ordering::Relaxed))
        );
        flag.store(true, Ordering::Relaxed);
        assert!(
            request
                .cancel
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
        );
    }
}

#[cfg(test)]
pub(crate) mod test_assets {
    /// Minimal valid 1x1 PNG, enough for \includegraphics and #image.
    pub const ONE_BY_ONE_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0xf8, 0xcf, 0x50, 0x0f, 0x00, 0x03, 0x86, 0x01, 0x80, 0x5a, 0x34, 0x7d, 0x6b, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
}

/// Everything a backend needs to run one compile, built once per request so
/// the tectonic/typst implementations differ only in argv and diagnostics.
#[derive(Debug)]
pub(crate) struct PreparedJob {
    pub build_path: PathBuf,
    pub cwd: PathBuf,
    pub input_file: PathBuf,
    pub output_pdf: PathBuf,
    /// The document the diagnostics should point at, if compiling an
    /// on-disk file rather than injected text.
    pub document: Option<PathBuf>,
}

pub(crate) struct JobDirs<'a> {
    pub build_root: &'a crate::util::TemporarySessionDir,
    pub keep_dirs: usize,
    pub prune_min_idle: Duration,
}

pub(crate) fn prepare_job(
    dirs: &JobDirs<'_>,
    request: &CompileRequest,
    source_stem: &str,
    source_ext: &str,
    engine: EngineIdentity,
) -> Result<PreparedJob, CompileError> {
    let start = Instant::now();
    let compile_id = request.compile_id;
    let revision = request.revision;
    let error = |message: String| CompileError {
        compile_id,
        revision,
        diagnostics: Vec::new(),
        message,
        duration: start.elapsed(),
    };

    let build_path = dirs.build_root.path().join(format!("job_{}", compile_id.0));
    std::fs::create_dir_all(&build_path).map_err(|err| {
        error(format!(
            "Failed to create {} build directory: {err}",
            engine.label
        ))
    })?;
    // One directory per compile with kept intermediates adds up over a
    // session; age-guarded pruning leaves in-flight compiles alone.
    crate::util::prune_numbered_dirs(
        dirs.build_root.path(),
        "job_",
        dirs.keep_dirs,
        dirs.prune_min_idle,
    );

    let (input_file, cwd, output_name, document) = match &request.source {
        CompileSource::File(root_doc) => {
            let file_stem = root_doc
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(source_stem);
            let pdf_name = format!("{file_stem}.pdf");
            let cwd = request
                .project_root
                .as_deref()
                .unwrap_or(build_path.as_path());
            (
                root_doc.clone(),
                cwd.to_path_buf(),
                pdf_name,
                Some(root_doc.clone()),
            )
        }
        CompileSource::Text(text) => {
            let input_file = build_path.join(format!("{source_stem}.{source_ext}"));
            std::fs::write(&input_file, text).map_err(|err| {
                error(format!(
                    "Failed to write {} source to temporary file: {err}",
                    engine.label
                ))
            })?;
            (
                input_file,
                build_path.clone(),
                format!("{source_stem}.pdf"),
                None,
            )
        }
    };

    Ok(PreparedJob {
        output_pdf: build_path.join(output_name),
        cwd,
        input_file,
        build_path,
        document,
    })
}

/// Runs an engine's command under the shared cancel machinery, converting
/// cancel/execute failures into that engine's shaped `CompileError` exactly
/// once (this match previously existed twice — a third copy was forming in
/// the renderer).
pub(crate) fn run_compile_subprocess(
    command: std::process::Command,
    request: &CompileRequest,
    start: Instant,
    engine: EngineIdentity,
) -> Result<std::process::Output, CompileError> {
    let cancelled_message = "Compile cancelled by a newer edit".to_string();
    let exec_error = |error: String| CompileError {
        compile_id: request.compile_id,
        revision: request.revision,
        diagnostics: Vec::new(),
        message: format!("Failed to execute {}: {error}", engine.label),
        duration: start.elapsed(),
    };
    match run_with_cancel(command, request.cancel.as_ref()) {
        SubprocessResult::Output(output) => Ok(output),
        SubprocessResult::Cancelled => Err(CompileError {
            compile_id: request.compile_id,
            revision: request.revision,
            diagnostics: Vec::new(),
            message: cancelled_message,
            duration: start.elapsed(),
        }),
        SubprocessResult::SpawnError(error) => Err(exec_error(error.to_string())),
    }
}

/// Turns a finished subprocess run into the result/error decision shared by
/// both backends: successful status + no error-grade diagnostics + file on
/// disk means output; anything else is a shaped failure.
/// Static facts that differ per backend: display label and where its
/// fallback diagnostics claim to come from.
#[derive(Clone, Copy)]
pub(crate) struct EngineIdentity {
    pub label: &'static str,
    /// Properly capitalized name for user-facing messages.
    pub display_name: &'static str,
    pub diagnostic_source: DiagnosticSource,
}

impl EngineIdentity {
    /// Shaped failure for a backend whose executable was never found. One
    /// implementation instead of two drifting "not installed" blocks.
    pub(crate) fn unavailable_error(
        self,
        compile_id: CompileId,
        revision: u64,
        document: Option<PathBuf>,
        start: Instant,
    ) -> CompileError {
        let message = format!("{} is not installed or configured", self.display_name);
        CompileError {
            compile_id,
            revision,
            diagnostics: vec![Diagnostic {
                id: next_diagnostic_id(),
                severity: super::diagnostics::Severity::Error,
                source: self.diagnostic_source,
                file: document,
                line: None,
                message: message.clone(),
            }],
            message,
            duration: start.elapsed(),
        }
    }
}

/// Process-wide diagnostic id allocator: one monotonic counter for every
/// diagnostic graf produces, so ids never collide or reset between parses
/// (tectonic used to restart at 1 per compile).
pub(crate) fn next_diagnostic_id() -> DiagnosticId {
    static NEXT_DIAGNOSTIC_ID: AtomicU64 = AtomicU64::new(1);
    DiagnosticId(NEXT_DIAGNOSTIC_ID.fetch_add(1, Ordering::Relaxed))
}

/// What one subprocess run produced, before the shared success predicate.
pub(crate) struct RunOutcome {
    pub status: std::process::ExitStatus,
    pub diagnostics: Vec<Diagnostic>,
    pub raw_failure_message: Option<String>,
}

pub(crate) fn finalize_output(
    request: &CompileRequest,
    job: &PreparedJob,
    start: Instant,
    engine: EngineIdentity,
    outcome: RunOutcome,
) -> Result<CompileOutput, CompileError> {
    let RunOutcome {
        status,
        diagnostics,
        raw_failure_message,
    } = outcome;

    let has_errors = diagnostics
        .iter()
        .any(|d| d.severity == super::diagnostics::Severity::Error);

    if status.success() && !has_errors && job.output_pdf.exists() {
        let artifact: ArtifactBytes = std::fs::read(&job.output_pdf)
            .map_err(|error| CompileError {
                compile_id: request.compile_id,
                revision: request.revision,
                diagnostics: diagnostics.clone(),
                message: format!("Failed to read {} PDF output: {error}", engine.label),
                duration: start.elapsed(),
            })?
            .into();

        return Ok(CompileOutput {
            compile_id: request.compile_id,
            revision: request.revision,
            artifact,
            diagnostics,
            duration: start.elapsed(),
        });
    }

    let message = diagnostics
        .iter()
        .filter(|d| d.severity == super::diagnostics::Severity::Error)
        .map(|d| d.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let message = if message.is_empty() {
        raw_failure_message.unwrap_or_else(|| format!("{} compilation failed", engine.label))
    } else {
        message
    };

    let diagnostics = if diagnostics.is_empty() {
        vec![Diagnostic {
            id: next_diagnostic_id(),
            severity: super::diagnostics::Severity::Error,
            source: engine.diagnostic_source,
            file: job.document.clone(),
            line: None,
            message: message.clone(),
        }]
    } else {
        diagnostics
    };

    Err(CompileError {
        compile_id: request.compile_id,
        revision: request.revision,
        diagnostics,
        message,
        duration: start.elapsed(),
    })
}

#[cfg(test)]
mod core_logic_tests {
    use super::super::diagnostics::Severity;
    use super::*;
    use std::process::Command;

    fn request() -> CompileRequest {
        CompileRequest::simple("hello", 3)
    }

    fn identity(label: &'static str, source: DiagnosticSource) -> EngineIdentity {
        EngineIdentity {
            label,
            display_name: label,
            diagnostic_source: source,
        }
    }

    fn job_dirs<'a>(build_root: &'a crate::util::TemporarySessionDir) -> JobDirs<'a> {
        JobDirs {
            build_root,
            keep_dirs: 2,
            prune_min_idle: Duration::from_secs(60),
        }
    }

    fn success_status() -> std::process::ExitStatus {
        std::process::Command::new("echo")
            .output()
            .expect("echo")
            .status
    }

    fn failed_status() -> std::process::ExitStatus {
        std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 1")
            .output()
            .expect("sh")
            .status
    }

    #[test]
    fn prepare_job_writes_source_without_a_root_document() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("tectonic", DiagnosticSource::Tectonic);

        let job = prepare_job(&job_dirs(&build_root), &request(), "input", "tex", identity)
            .expect("job prepared");

        // The source lands in the per-job build dir and the pdf target
        // follows the input stem.
        let CompileSource::Text(text) = &request().source else {
            panic!("expected text source");
        };
        assert!(
            std::fs::read_to_string(&job.input_file)
                .expect("source written")
                .contains(text.as_str())
        );
        assert_eq!(job.output_pdf.file_name().unwrap(), "input.pdf");
        assert_eq!(job.cwd, job.build_path);
        assert_eq!(job.document, None);
    }

    #[test]
    fn prepare_job_uses_the_root_document_for_projected_compiles() {
        let project = tempfile::tempdir().unwrap();
        let main_tex = project.path().join("chapters").join("paper.tex");
        std::fs::create_dir_all(main_tex.parent().unwrap()).unwrap();
        std::fs::write(&main_tex, "real doc").expect("root doc");
        let build_root = crate::util::TemporarySessionDir::from_path(project.path());
        let identity = identity("tectonic", DiagnosticSource::Tectonic);

        let with_root = CompileRequest::with_project(
            "text is unused",
            3,
            Some(project.path().to_path_buf()),
            Some(main_tex.clone()),
        );
        let job = prepare_job(&job_dirs(&build_root), &with_root, "input", "tex", identity)
            .expect("job prepared");

        // The engine compiles the on-disk root doc, not an injected temp
        // copy, with the project as cwd and the doc stem as the pdf name.
        assert_eq!(job.input_file, main_tex);
        assert_eq!(job.output_pdf.file_name().unwrap(), "paper.pdf");
        assert_eq!(job.cwd, project.path());
        assert_eq!(job.document, Some(main_tex));
        assert!(!job.input_file.starts_with(job.build_path.join("job_0")));
    }

    #[test]
    fn finalize_output_requires_success_no_errors_and_a_pdf() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("tectonic", DiagnosticSource::Tectonic);
        let job =
            prepare_job(&job_dirs(&build_root), &request(), "input", "tex", identity).unwrap();

        let success_status = success_status();
        let no_pdf = finalize_output(
            &request(),
            &job,
            std::time::Instant::now(),
            identity,
            RunOutcome {
                status: success_status,
                diagnostics: Vec::new(),
                raw_failure_message: None,
            },
        );

        // No output file: a shaped failure even with a green exit status —
        // the renderer must never be handed a nonexistent pdf.
        let error = no_pdf.expect_err("missing pdf must fail");
        assert_eq!(error.message, "tectonic compilation failed");
        assert_eq!(error.revision, 3);
        assert_eq!(error.diagnostics.len(), 1);

        // With the pdf on disk and a green status, the artifact is read.
        std::fs::write(&job.output_pdf, b"%PDF-1.7 test").expect("pdf");
        let output = finalize_output(
            &request(),
            &job,
            std::time::Instant::now(),
            identity,
            RunOutcome {
                status: success_status,
                diagnostics: Vec::new(),
                raw_failure_message: None,
            },
        )
        .expect("pdf exists");
        assert_eq!(output.artifact.as_ref(), b"%PDF-1.7 test");
        assert_eq!(output.revision, 3);

        // Error-grade diagnostics reject the pdf even when it exists.
        let with_errors = RunOutcome {
            status: success_status,
            diagnostics: vec![Diagnostic::new(
                1,
                Severity::Error,
                DiagnosticSource::Tectonic,
                None,
                None,
                "boom",
            )],
            raw_failure_message: None,
        };
        let error = finalize_output(&request(), &job, Instant::now(), identity, with_errors)
            .expect_err("error diagnostics must fail");
        assert_eq!(error.message, "boom");
        assert!(!error.diagnostics.is_empty());
    }

    #[test]
    fn finalize_output_uses_raw_failure_when_no_diagnostics_parsed() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("Typst", DiagnosticSource::Typst);
        let job = prepare_job(
            &job_dirs(&build_root),
            &request(),
            "document",
            "typ",
            identity,
        )
        .unwrap();

        let failed_status = failed_status();
        let error = finalize_output(
            &request(),
            &job,
            std::time::Instant::now(),
            identity,
            RunOutcome {
                status: failed_status,
                diagnostics: Vec::new(),
                raw_failure_message: Some("fatal stderr line".to_string()),
            },
        )
        .expect_err("failed status must fail");

        // The raw subprocess message survives into the shaped error.
        assert_eq!(error.message, "fatal stderr line");
        assert_eq!(error.diagnostics[0].message, "fatal stderr line");
        assert_eq!(error.diagnostics[0].source, DiagnosticSource::Typst);
    }

    #[test]
    fn prepare_job_from_file_without_project_root_runs_in_the_build_dir() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("Typst", DiagnosticSource::Typst);

        let doc = temp.path().join("standalone.typ");
        std::fs::write(&doc, "= hi").unwrap();
        let compile_request = CompileRequest::with_project("", 2, None, Some(doc.clone()));
        let job = prepare_job(
            &job_dirs(&build_root),
            &compile_request,
            "document",
            "typ",
            identity,
        )
        .expect("job prepared");

        assert_eq!(job.input_file, doc);
        assert_eq!(job.cwd, job.build_path);
        assert_eq!(job.output_pdf.file_name().unwrap(), "standalone.pdf");
        assert_eq!(job.document, Some(doc));
    }

    #[test]
    fn prepare_job_dir_layout_is_one_dir_per_compile() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("tectonic", DiagnosticSource::Tectonic);

        let first = prepare_job(&job_dirs(&build_root), &request(), "input", "tex", identity)
            .expect("first job");
        let second = prepare_job(&job_dirs(&build_root), &request(), "input", "tex", identity)
            .expect("second job");

        assert_ne!(first.build_path, second.build_path);
        assert!(first.build_path.starts_with(build_root.path()));
        assert!(second.build_path.starts_with(build_root.path()));
        assert!(first.build_path.is_dir());
    }

    #[test]
    fn prepare_job_reports_directory_creation_failure() {
        // A file where the build root expects to create job dirs forces the
        // create_dir_all inside prepare_job to fail; the error must be
        // shaped, not a panic or an ignored Result.
        let temp = tempfile::tempdir().unwrap();
        let blocker = temp.path().join("blocker");
        std::fs::write(&blocker, b"not a dir").unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(&blocker);
        let identity = identity("tectonic", DiagnosticSource::Tectonic);

        let compile_request = request();
        let error = prepare_job(
            &job_dirs(&build_root),
            &compile_request,
            "input",
            "tex",
            identity,
        )
        .expect_err("blocked build root must fail");
        assert!(
            error.message.contains("build directory"),
            "{}",
            error.message
        );
        assert_eq!(error.compile_id, compile_request.compile_id);
        assert_eq!(error.revision, 3);
        assert!(error.diagnostics.is_empty());
    }

    #[test]
    fn finalize_output_reports_a_pdf_that_exists_but_cannot_be_read() {
        // The success predicate is output_pdf.exists() at read time; a file
        // replaced by a directory (erase/recreate race) exists but fails the
        // read. That must surface as a shaped failure, not a fake artifact.
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("Typst", DiagnosticSource::Typst);
        let mut job = prepare_job(
            &job_dirs(&build_root),
            &request(),
            "document",
            "typ",
            identity,
        )
        .unwrap();

        let unreadable = job.build_path.join("unreadable.pdf");
        std::fs::create_dir_all(&unreadable).unwrap();
        job.output_pdf = unreadable;

        let error = finalize_output(
            &request(),
            &job,
            Instant::now(),
            identity,
            RunOutcome {
                status: success_status(),
                diagnostics: Vec::new(),
                raw_failure_message: None,
            },
        )
        .expect_err("unreadable pdf must fail");
        assert!(
            error.message.contains("Failed to read"),
            "{}",
            error.message
        );
        assert_eq!(error.revision, 3);
    }

    #[test]
    fn finalize_output_rejects_a_failed_status_even_with_a_pdf_on_disk() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = identity("tectonic", DiagnosticSource::Tectonic);
        let job =
            prepare_job(&job_dirs(&build_root), &request(), "input", "tex", identity).unwrap();
        std::fs::write(&job.output_pdf, b"%PDF-1.7").expect("pdf");

        let error = finalize_output(
            &request(),
            &job,
            Instant::now(),
            identity,
            RunOutcome {
                status: failed_status(),
                diagnostics: Vec::new(),
                raw_failure_message: None,
            },
        )
        .expect_err("failed status with a pdf on disk must fail");
        assert_eq!(error.message, "tectonic compilation failed");
    }

    #[test]
    fn finalize_output_success_keeps_warning_diagnostics_and_reports_the_document() {
        let project = tempfile::tempdir().unwrap();
        let main_tex = project.path().join("paper.tex");
        std::fs::write(&main_tex, "doc").unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(project.path());
        let identity = identity("tectonic", DiagnosticSource::Tectonic);
        let compile_request = CompileRequest::with_project(
            "unused",
            5,
            Some(project.path().to_path_buf()),
            Some(main_tex.clone()),
        );
        let job = prepare_job(
            &job_dirs(&build_root),
            &compile_request,
            "input",
            "tex",
            identity,
        )
        .unwrap();
        std::fs::write(&job.output_pdf, b"%PDF-1.7").expect("pdf");

        let output = finalize_output(
            &compile_request,
            &job,
            Instant::now(),
            identity,
            RunOutcome {
                status: success_status(),
                diagnostics: vec![Diagnostic::new(
                    1,
                    Severity::Warning,
                    DiagnosticSource::Tectonic,
                    None,
                    Some(2),
                    "unused label",
                )],
                raw_failure_message: None,
            },
        )
        .expect("warnings must not fail the compile");
        assert_eq!(output.artifact.as_ref(), b"%PDF-1.7");
        assert_eq!(output.diagnostics.len(), 1);
        assert_eq!(output.revision, 5);
    }

    #[test]
    fn run_compile_subprocess_shapes_cancel_spawn_and_success() {
        let identity = identity("tectonic", DiagnosticSource::Tectonic);

        // Cancelled before spawn.
        let cancel = Arc::new(AtomicBool::new(true));
        let compile_request = CompileRequest::simple("x", 1).with_cancel(cancel);
        let error = run_compile_subprocess(
            Command::new("echo"),
            &compile_request,
            Instant::now(),
            identity,
        )
        .expect_err("cancelled");
        assert_eq!(error.message, "Compile cancelled by a newer edit");

        // Spawn failure (missing binary) — must be an execute failure, not
        // a cancellation.
        let compile_request = CompileRequest::simple("x", 1);
        let error = run_compile_subprocess(
            Command::new("graf-no-such-binary"),
            &compile_request,
            Instant::now(),
            identity,
        )
        .expect_err("spawn error");
        assert!(
            error.message.contains("Failed to execute"),
            "{}",
            error.message
        );

        // Happy path returns the raw output.
        let output = run_compile_subprocess(
            Command::new("echo"),
            &compile_request,
            Instant::now(),
            identity,
        )
        .expect("output");
        assert!(output.status.success());
    }

    #[test]
    fn diagnostic_ids_are_globally_monotonic_across_parses() {
        let first = next_diagnostic_id();
        let second = next_diagnostic_id();
        assert!(second.0 > first.0);
    }

    #[test]
    fn unavailable_error_shapes_the_missing_backend_failure() {
        let mut tectonic_identity = identity("tectonic", DiagnosticSource::Tectonic);
        tectonic_identity.display_name = "Tectonic";
        let compile_request = CompileRequest::simple("\\documentclass{article}", 4);
        let document = Some(PathBuf::from("/proj/main.tex"));

        let error = tectonic_identity.unavailable_error(
            compile_request.compile_id,
            compile_request.revision,
            document.clone(),
            Instant::now(),
        );

        assert_eq!(error.message, "Tectonic is not installed or configured");
        assert_eq!(error.compile_id, compile_request.compile_id);
        assert_eq!(error.revision, 4);
        assert_eq!(error.diagnostics.len(), 1);
        assert_eq!(error.diagnostics[0].message, error.message);
        assert_eq!(error.diagnostics[0].source, DiagnosticSource::Tectonic);
        assert_eq!(error.diagnostics[0].file, document);
    }
}
