//! Compile controller, state machine, debounce, and stale result rejection (spec §32–35).

use std::time::{Duration, Instant};

use super::diagnostics::Diagnostic;
use super::engine::{CompileError, CompileId, CompileOutput};

/// The lifecycle state of document compilation (spec §35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileState {
    Idle,
    Waiting,
    Compiling {
        id: CompileId,
        revision: u64,
    },
    Success {
        id: CompileId,
        revision: u64,
        duration: Duration,
    },
    Failed {
        id: CompileId,
        revision: u64,
        diagnostics: Vec<Diagnostic>,
        duration: Duration,
    },
}

use std::borrow::Cow;

impl CompileState {
    /// Returns a user-friendly status string for the status bar with zero allocation for static states.
    pub fn status_text(&self) -> Cow<'static, str> {
        match self {
            Self::Idle => Cow::Borrowed("ready"),
            Self::Waiting => Cow::Borrowed("waiting..."),
            Self::Compiling { revision, .. } => {
                Cow::Owned(format!("compiling (rev {revision})..."))
            }
            Self::Success {
                duration, revision, ..
            } => Cow::Owned(format!(
                "ready (rev {revision}, {:.0}ms)",
                duration.as_secs_f64() * 1000.0
            )),
            Self::Failed { diagnostics, .. } => match diagnostics.len() {
                0 => Cow::Borrowed("failed"),
                1 => Cow::Borrowed("1 error"),
                n => Cow::Owned(format!("{n} errors")),
            },
        }
    }
}

/// Represents an output that was discarded because a newer revision was already requested or completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleResult {
    pub completed_revision: u64,
    pub current_revision: u64,
}

/// Manages the compilation lifecycle, revision tracking, debouncing, and stale rejection (spec §34).
pub struct CompilerController {
    current_revision: u64,
    latest_completed_revision: u64,
    state: CompileState,
    debounce_duration: Duration,
    last_edit_time: Option<Instant>,
    latest_artifact: Option<Vec<u8>>,
}

impl Default for CompilerController {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilerController {
    /// Create a new compile controller with default 150ms debounce for responsive hot reload.
    pub fn new() -> Self {
        Self::with_debounce(Duration::from_millis(150))
    }

    /// Create a new compile controller with custom debounce duration.
    pub fn with_debounce(debounce_duration: Duration) -> Self {
        Self {
            current_revision: 0,
            latest_completed_revision: 0,
            state: CompileState::Idle,
            debounce_duration,
            last_edit_time: None,
            latest_artifact: None,
        }
    }

    /// Returns the current compilation state.
    pub fn state(&self) -> &CompileState {
        &self.state
    }

    /// Returns the user-friendly status text for the current state.
    pub fn status_text(&self) -> Cow<'static, str> {
        self.state.status_text()
    }

    /// Returns the latest document revision registered with the controller.
    pub fn current_revision(&self) -> u64 {
        self.current_revision
    }

    /// Returns the latest successfully compiled revision.
    pub fn latest_completed_revision(&self) -> u64 {
        self.latest_completed_revision
    }

    /// Returns the latest compiled PDF artifact bytes, if available.
    pub fn latest_artifact(&self) -> Option<&[u8]> {
        self.latest_artifact.as_deref()
    }

    /// Configured debounce duration.
    pub fn debounce_duration(&self) -> Duration {
        self.debounce_duration
    }

    /// Called when the editor content is modified with a new revision number.
    pub fn on_source_edited(&mut self, new_revision: u64, now: Instant) {
        if new_revision > self.current_revision {
            self.current_revision = new_revision;
            self.last_edit_time = Some(now);
            self.state = CompileState::Waiting;
        }
    }

    /// Checks if the debounce interval has elapsed and compilation should begin.
    pub fn is_debounce_elapsed(&self, now: Instant) -> bool {
        self.last_edit_time.is_some_and(|edit_time| {
            now.saturating_duration_since(edit_time) >= self.debounce_duration
        })
    }

    /// Transition to `Compiling` state for a given job and revision.
    pub fn begin_compile(&mut self, id: CompileId, revision: u64) {
        self.state = CompileState::Compiling { id, revision };
    }

    fn reject_if_stale(&mut self, revision: u64) -> Result<(), StaleResult> {
        if revision < self.current_revision {
            if let CompileState::Compiling { revision: cur, .. } = self.state
                && cur == revision
            {
                self.state = CompileState::Waiting;
            }
            Err(StaleResult {
                completed_revision: revision,
                current_revision: self.current_revision,
            })
        } else {
            Ok(())
        }
    }

    /// Handles a successful compile output.
    pub fn handle_output(&mut self, output: CompileOutput) -> Result<&[u8], StaleResult> {
        self.reject_if_stale(output.revision)?;

        self.latest_completed_revision = output.revision;
        self.state = CompileState::Success {
            id: output.compile_id,
            revision: output.revision,
            duration: output.duration,
        };
        Ok(self.latest_artifact.insert(output.artifact).as_slice())
    }

    /// Handles a failed compilation error.
    pub fn handle_error(&mut self, error: CompileError) -> Result<(), StaleResult> {
        self.reject_if_stale(error.revision)?;

        self.state = CompileState::Failed {
            id: error.compile_id,
            revision: error.revision,
            diagnostics: error.diagnostics,
            duration: error.duration,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::diagnostics::{DiagnosticSource, Severity};
    use crate::compiler::engine::ArtifactKind;

    #[test]
    fn test_initial_state() {
        let controller = CompilerController::new();
        assert_eq!(controller.state(), &CompileState::Idle);
        assert_eq!(controller.current_revision(), 0);
        assert_eq!(controller.latest_completed_revision(), 0);
        assert_eq!(controller.status_text(), "ready");
    }

    #[test]
    fn test_debounce_and_waiting() {
        let mut controller = CompilerController::with_debounce(Duration::from_millis(100));
        let t0 = Instant::now();

        controller.on_source_edited(1, t0);
        assert_eq!(controller.state(), &CompileState::Waiting);
        assert_eq!(controller.current_revision(), 1);
        assert!(!controller.is_debounce_elapsed(t0 + Duration::from_millis(50)));
        assert!(controller.is_debounce_elapsed(t0 + Duration::from_millis(100)));
    }

    #[test]
    fn test_compiling_and_success_flow() {
        let mut controller = CompilerController::new();
        let t0 = Instant::now();

        controller.on_source_edited(1, t0);
        controller.begin_compile(CompileId(10), 1);
        assert_eq!(
            controller.state(),
            &CompileState::Compiling {
                id: CompileId(10),
                revision: 1
            }
        );

        let output = CompileOutput {
            compile_id: CompileId(10),
            revision: 1,
            artifact: b"%PDF-1.5 test content".to_vec(),
            artifact_kind: ArtifactKind::Pdf,
            diagnostics: vec![],
            duration: Duration::from_millis(45),
        };

        let res = controller.handle_output(output);
        assert!(res.is_ok());
        assert_eq!(
            controller.state(),
            &CompileState::Success {
                id: CompileId(10),
                revision: 1,
                duration: Duration::from_millis(45)
            }
        );
        assert_eq!(controller.latest_completed_revision(), 1);
        assert_eq!(
            controller.latest_artifact(),
            Some(b"%PDF-1.5 test content".as_slice())
        );
    }

    #[test]
    fn test_stale_output_rejection() {
        let mut controller = CompilerController::new();
        let t0 = Instant::now();

        controller.on_source_edited(1, t0);
        controller.begin_compile(CompileId(1), 1);

        controller.on_source_edited(2, t0 + Duration::from_millis(50));
        assert_eq!(controller.current_revision(), 2);

        let output_rev1 = CompileOutput {
            compile_id: CompileId(1),
            revision: 1,
            artifact: b"stale pdf".to_vec(),
            artifact_kind: ArtifactKind::Pdf,
            diagnostics: vec![],
            duration: Duration::from_millis(20),
        };

        let res = controller.handle_output(output_rev1);
        assert_eq!(
            res,
            Err(StaleResult {
                completed_revision: 1,
                current_revision: 2,
            })
        );
        assert_eq!(controller.latest_artifact(), None);
        assert_eq!(controller.state(), &CompileState::Waiting);
    }

    #[test]
    fn test_stale_error_rejection() {
        let mut controller = CompilerController::new();
        let t0 = Instant::now();

        controller.on_source_edited(1, t0);
        controller.begin_compile(CompileId(1), 1);

        controller.on_source_edited(2, t0 + Duration::from_millis(50));

        let err_rev1 = CompileError {
            compile_id: CompileId(1),
            revision: 1,
            diagnostics: vec![],
            message: "old error".to_string(),
            duration: Duration::from_millis(20),
        };

        let res = controller.handle_error(err_rev1);
        assert_eq!(
            res,
            Err(StaleResult {
                completed_revision: 1,
                current_revision: 2,
            })
        );
        assert_eq!(controller.state(), &CompileState::Waiting);
    }

    #[test]
    fn test_error_state_handling() {
        let mut controller = CompilerController::new();
        let t0 = Instant::now();

        controller.on_source_edited(1, t0);
        controller.begin_compile(CompileId(1), 1);

        let err = CompileError {
            compile_id: CompileId(1),
            revision: 1,
            diagnostics: vec![Diagnostic::new(
                1,
                Severity::Error,
                DiagnosticSource::Tectonic,
                None,
                Some(4),
                "syntax error",
            )],
            message: "syntax error".to_string(),
            duration: Duration::from_millis(15),
        };

        let res = controller.handle_error(err);
        assert!(res.is_ok());
        assert!(matches!(controller.state(), CompileState::Failed { .. }));
        assert_eq!(controller.status_text(), "1 error");
    }

    #[test]
    fn test_multiple_errors_status_text() {
        let mut controller = CompilerController::new();
        let t0 = Instant::now();

        controller.on_source_edited(1, t0);
        controller.begin_compile(CompileId(1), 1);

        let err = CompileError {
            compile_id: CompileId(1),
            revision: 1,
            diagnostics: vec![
                Diagnostic::new(
                    1,
                    Severity::Error,
                    DiagnosticSource::Tectonic,
                    None,
                    Some(1),
                    "err 1",
                ),
                Diagnostic::new(
                    2,
                    Severity::Error,
                    DiagnosticSource::Tectonic,
                    None,
                    Some(2),
                    "err 2",
                ),
            ],
            message: "multiple errors".to_string(),
            duration: Duration::from_millis(15),
        };

        let res = controller.handle_error(err);
        assert!(res.is_ok());
        assert_eq!(controller.status_text(), "2 errors");
    }
}
