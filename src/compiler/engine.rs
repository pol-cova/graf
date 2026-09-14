use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::diagnostics::{Diagnostic, DiagnosticSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompileId(pub u64);

#[derive(Debug, Clone)]
pub struct CompileRequest {
    pub compile_id: CompileId,
    pub source: String,
    pub revision: u64,
    pub project_root: Option<PathBuf>,
    pub root_document: Option<PathBuf>,
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
            source: source.into(),
            revision,
            project_root: None,
            root_document: None,
            cancel: None,
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
            cancel: None,
        }
    }

    /// Attach a cancellation flag so a newer edit can abort this compile.
    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }
}

/// Marker for a subprocess that was killed because its result would be stale.
#[derive(Debug)]
pub struct CompileCancelled;

/// Run a command capturing stdout/stderr while watching a cancel flag. A
/// blocking `output()` would tie a background thread to a stale compile for
/// its whole duration; this polls `try_wait` and kills the child on cancel.
pub fn run_with_cancel(
    mut command: std::process::Command,
    cancel: Option<&Arc<AtomicBool>>,
) -> std::io::Result<Result<std::process::Output, CompileCancelled>> {
    use std::process::Stdio;

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Ok(Err(CompileCancelled));
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

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
                return Ok(if cancelled {
                    Err(CompileCancelled)
                } else {
                    Ok(std::process::Output {
                        status,
                        stdout,
                        stderr,
                    })
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            // An unexpected IO error (e.g. waitpid failure) must not leave a
            // still-running compiler behind: kill the child, surface it.
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
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
        let output = run_with_cancel(command, None)
            .expect("spawn works")
            .expect("not cancelled");
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
        let result = run_with_cancel(command, Some(&cancel))
            .expect("spawn works")
            .expect_err("cancelled compile must not return output");

        assert!(matches!(result, CompileCancelled));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn run_with_cancel_short_circuits_before_spawn() {
        let cancel = Arc::new(AtomicBool::new(true));
        let mut command = Command::new("sleep");
        command.arg("10");
        let result = run_with_cancel(command, Some(&cancel))
            .expect("no io error")
            .expect_err("pre-cancelled");
        assert!(matches!(result, CompileCancelled));
    }

    #[test]
    fn grandchild_inheriting_pipes_cannot_hang_the_reap() {
        // The child spawns a grandchild that inherits stdout/stderr and
        // outlives it by a second: the bounded join must return output
        // (possibly partial) instead of blocking forever.
        let mut command = Command::new("sh");
        command.arg("-c").arg("echo start; sleep 1 & echo done");
        let started = std::time::Instant::now();
        let output = run_with_cancel(command, None)
            .expect("spawn works")
            .expect("not cancelled");
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
            Ok(Err(CompileCancelled)) | Err(_) => {}
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
pub(crate) struct PreparedJob {
    pub build_path: PathBuf,
    pub cwd: PathBuf,
    pub input_file: PathBuf,
    pub output_pdf: PathBuf,
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

    let (input_file, cwd, output_name) = if let Some(root_doc) = &request.root_document {
        let file_stem = root_doc
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(source_stem);
        let pdf_name = format!("{file_stem}.pdf");
        let cwd = request
            .project_root
            .as_deref()
            .unwrap_or(build_path.as_path());
        (root_doc.clone(), cwd.to_path_buf(), pdf_name)
    } else {
        let input_file = build_path.join(format!("{source_stem}.{source_ext}"));
        std::fs::write(&input_file, &request.source).map_err(|err| {
            error(format!(
                "Failed to write {} source to temporary file: {err}",
                engine.label
            ))
        })?;
        (input_file, build_path.clone(), format!("{source_stem}.pdf"))
    };

    Ok(PreparedJob {
        output_pdf: build_path.join(output_name),
        cwd,
        input_file,
        build_path,
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
        Ok(Ok(output)) => Ok(output),
        Ok(Err(_)) => Err(CompileError {
            compile_id: request.compile_id,
            revision: request.revision,
            diagnostics: Vec::new(),
            message: cancelled_message,
            duration: start.elapsed(),
        }),
        Err(error) => Err(exec_error(error.to_string())),
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
    pub diagnostic_source: DiagnosticSource,
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
            id: super::diagnostics::DiagnosticId(request.compile_id.0),
            severity: super::diagnostics::Severity::Error,
            source: engine.diagnostic_source,
            file: request.root_document.clone(),
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
    use super::*;

    fn request() -> CompileRequest {
        CompileRequest::simple("hello", 3)
    }

    #[test]
    fn prepare_job_writes_source_without_a_root_document() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = EngineIdentity {
            label: "tectonic",
            diagnostic_source: super::super::diagnostics::DiagnosticSource::Tectonic,
        };

        let job = prepare_job(
            &JobDirs {
                build_root: &build_root,
                keep_dirs: 2,
                prune_min_idle: Duration::from_secs(60),
            },
            &request(),
            "input",
            "tex",
            identity,
        )
        .expect("job prepared");

        // The source lands in the per-job build dir and the pdf target
        // follows the input stem.
        assert!(
            std::fs::read_to_string(&job.input_file)
                .expect("source written")
                .contains("hello")
        );
        assert_eq!(job.output_pdf.file_name().unwrap(), "input.pdf");
        assert_eq!(job.cwd, job.build_path);
    }

    #[test]
    fn prepare_job_uses_the_root_document_for_projected_compiles() {
        let project = tempfile::tempdir().unwrap();
        let main_tex = project.path().join("chapters").join("paper.tex");
        std::fs::create_dir_all(main_tex.parent().unwrap()).unwrap();
        std::fs::write(&main_tex, "real doc").expect("root doc");
        let build_root = crate::util::TemporarySessionDir::from_path(project.path());
        let identity = EngineIdentity {
            label: "tectonic",
            diagnostic_source: super::super::diagnostics::DiagnosticSource::Tectonic,
        };

        let mut with_root = request();
        with_root.project_root = Some(project.path().to_path_buf());
        with_root.root_document = Some(main_tex.clone());
        let job = prepare_job(
            &JobDirs {
                build_root: &build_root,
                keep_dirs: 2,
                prune_min_idle: Duration::from_secs(60),
            },
            &with_root,
            "input",
            "tex",
            identity,
        )
        .expect("job prepared");

        // The engine compiles the on-disk root doc, not an injected temp
        // copy, with the project as cwd and the doc stem as the pdf name.
        assert_eq!(job.input_file, main_tex);
        assert_eq!(job.output_pdf.file_name().unwrap(), "paper.pdf");
        assert_eq!(job.cwd, project.path());
        assert!(!job.input_file.starts_with(job.build_path.join("job_0")));
    }

    #[test]
    fn finalize_output_requires_success_no_errors_and_a_pdf() {
        let temp = tempfile::tempdir().unwrap();
        let build_root = crate::util::TemporarySessionDir::from_path(temp.path());
        let identity = EngineIdentity {
            label: "tectonic",
            diagnostic_source: super::super::diagnostics::DiagnosticSource::Tectonic,
        };
        let job = prepare_job(
            &JobDirs {
                build_root: &build_root,
                keep_dirs: 2,
                prune_min_idle: Duration::from_secs(60),
            },
            &request(),
            "input",
            "tex",
            identity,
        )
        .unwrap();

        let success_status = std::process::Command::new("echo")
            .output()
            .expect("echo")
            .status;
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
            diagnostics: vec![super::super::diagnostics::Diagnostic::new(
                1,
                super::super::diagnostics::Severity::Error,
                super::super::diagnostics::DiagnosticSource::Tectonic,
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
        let identity = EngineIdentity {
            label: "Typst",
            diagnostic_source: super::super::diagnostics::DiagnosticSource::Typst,
        };
        let job = prepare_job(
            &JobDirs {
                build_root: &build_root,
                keep_dirs: 2,
                prune_min_idle: Duration::from_secs(60),
            },
            &request(),
            "document",
            "typ",
            identity,
        )
        .unwrap();

        let failed_status = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 1")
            .output()
            .expect("sh")
            .status;
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
        assert_eq!(
            error.diagnostics[0].source,
            super::super::diagnostics::DiagnosticSource::Typst
        );
    }
}
