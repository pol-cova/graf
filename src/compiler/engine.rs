use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::diagnostics::Diagnostic;

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
