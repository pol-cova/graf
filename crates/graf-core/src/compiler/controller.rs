use std::time::Duration;

use super::diagnostics::Diagnostic;
use super::engine::{CompileError, CompileId, CompileOutput};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleResult {
    pub completed_revision: u64,
    pub current_revision: u64,
}

/// Tracks the current revision and refuses results from superseded builds.
///
/// Debouncing used to live here, as a `debounce_duration` the workspace read
/// back and slept on. It no longer does: Swift owns the pause between typing
/// and compiling (`AppSettings.compileDelay`, driving a `Debouncer`), so the
/// field was written once with `Duration::ZERO` and never read. What remains is
/// the part that is load-bearing — rejecting a background result whose
/// revision is no longer current.
pub struct CompilerController {
    current_revision: u64,
    state: CompileState,
}

impl Default for CompilerController {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilerController {
    pub fn new() -> Self {
        Self {
            current_revision: 0,
            state: CompileState::Idle,
        }
    }

    pub fn state(&self) -> &CompileState {
        &self.state
    }

    pub fn current_revision(&self) -> u64 {
        self.current_revision
    }

    pub fn on_source_edited(&mut self, new_revision: u64) {
        if new_revision > self.current_revision {
            self.current_revision = new_revision;
            self.state = CompileState::Waiting;
        }
    }

    pub fn begin_compile(&mut self, id: CompileId, revision: u64) {
        self.current_revision = self.current_revision.max(revision);
        self.state = CompileState::Compiling { id, revision };
    }

    pub fn accepts_result(&self, id: CompileId, revision: u64) -> bool {
        revision >= self.current_revision
            && matches!(
                self.state,
                CompileState::Compiling {
                    id: active_id,
                    revision: active_revision,
                } if active_id == id && active_revision == revision
            )
    }

    fn reject_if_stale(&mut self, id: CompileId, revision: u64) -> Result<(), StaleResult> {
        if !self.accepts_result(id, revision) {
            if let CompileState::Compiling { id: active_id, .. } = self.state
                && active_id == id
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

    pub fn handle_output(&mut self, output: &CompileOutput) -> Result<(), StaleResult> {
        self.reject_if_stale(output.compile_id, output.revision)?;

        self.state = CompileState::Success {
            id: output.compile_id,
            revision: output.revision,
            duration: output.duration,
        };
        Ok(())
    }

    pub fn handle_error(&mut self, error: CompileError) -> Result<(), StaleResult> {
        self.reject_if_stale(error.compile_id, error.revision)?;

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
    use std::sync::Arc;

    #[test]
    fn test_initial_state() {
        let controller = CompilerController::new();
        assert_eq!(controller.state(), &CompileState::Idle);
        assert_eq!(controller.current_revision(), 0);
    }

    /// A completed build leaves the controller at that revision, and the next
    /// edit moves it forward. `reset()` used to exist for the GPUI workspace,
    /// which cleared the controller when a compile session restarted; nothing
    /// does that now, because a session is a `Workspace` and closing it drops
    /// the controller with it.
    #[test]
    fn a_completed_build_then_a_new_edit_advances_the_revision() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(7);
        controller.begin_compile(CompileId(1), 7);
        controller
            .handle_output(&CompileOutput {
                compile_id: CompileId(1),
                revision: 7,
                artifact: Arc::from(&b"pdf"[..]),
                diagnostics: vec![],
                duration: Duration::from_millis(1),
            })
            .expect("current output should be accepted");

        assert_eq!(controller.current_revision(), 7);
        assert!(matches!(controller.state(), CompileState::Success { .. }));

        controller.on_source_edited(8);

        assert_eq!(controller.current_revision(), 8);
        assert_eq!(controller.state(), &CompileState::Waiting);
    }

    #[test]
    fn begin_compile_tracks_manual_compile_revision() {
        let mut controller = CompilerController::new();

        controller.begin_compile(CompileId(1), 4);

        assert_eq!(controller.current_revision(), 4);
    }

    #[test]
    fn result_is_current_only_for_active_revision() {
        let mut controller = CompilerController::new();
        controller.begin_compile(CompileId(4), 7);
        assert!(controller.accepts_result(CompileId(4), 7));

        controller.on_source_edited(8);
        assert!(!controller.accepts_result(CompileId(4), 7));
    }

    #[test]
    fn source_edit_enters_waiting_state() {
        let mut controller = CompilerController::new();

        controller.on_source_edited(1);

        assert_eq!(controller.state(), &CompileState::Waiting);
        assert_eq!(controller.current_revision(), 1);
    }

    #[test]
    fn test_compiling_and_success_flow() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(1);
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
            artifact: Arc::from(&b"%PDF-1.5 test content"[..]),
            diagnostics: vec![],
            duration: Duration::from_millis(45),
        };

        let res = controller.handle_output(&output);
        assert!(res.is_ok());
        assert_eq!(
            controller.state(),
            &CompileState::Success {
                id: CompileId(10),
                revision: 1,
                duration: Duration::from_millis(45)
            }
        );
    }

    /// A result is only accepted for the compile that is currently running: a
    /// different id is stale even at a revision the controller has seen. This
    /// is what stops a slow build from one keystroke applying over a newer
    /// one.
    #[test]
    fn output_from_a_superseded_compile_is_rejected() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(7);
        controller.begin_compile(CompileId(1), 7);
        // A second build starts while the first is still running.
        controller.begin_compile(CompileId(2), 8);

        let result = controller.handle_output(&CompileOutput {
            compile_id: CompileId(1),
            revision: 7,
            artifact: Arc::from(&b"old document"[..]),
            diagnostics: vec![],
            duration: Duration::from_millis(1),
        });

        assert_eq!(
            result,
            Err(StaleResult {
                completed_revision: 7,
                current_revision: 8,
            })
        );
        assert_eq!(
            controller.state(),
            &CompileState::Compiling {
                id: CompileId(2),
                revision: 8,
            }
        );
    }

    #[test]
    fn test_stale_output_rejection() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(1);
        controller.begin_compile(CompileId(1), 1);

        controller.on_source_edited(2);
        assert_eq!(controller.current_revision(), 2);

        let output_rev1 = CompileOutput {
            compile_id: CompileId(1),
            revision: 1,
            artifact: Arc::from(&b"stale pdf"[..]),
            diagnostics: vec![],
            duration: Duration::from_millis(20),
        };

        let res = controller.handle_output(&output_rev1);
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
    fn test_stale_error_rejection() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(1);
        controller.begin_compile(CompileId(1), 1);

        controller.on_source_edited(2);

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
        controller.on_source_edited(1);
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
    }

    /// The failed state keeps the diagnostics and the duration, which is what
    /// the Swift side reads to render its error count and message. It used to
    /// also assert a formatted status string; that display lived here for the
    /// GPUI status bar and `Workspace.BuildStatus` owns it now.
    #[test]
    fn a_failure_reports_diagnostics_and_duration() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(3);
        controller.begin_compile(CompileId(1), 3);

        let err = CompileError {
            compile_id: CompileId(1),
            revision: 3,
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

        controller
            .handle_error(err)
            .expect("current failure is accepted");

        match controller.state() {
            CompileState::Failed {
                id,
                revision,
                diagnostics,
                duration,
            } => {
                assert_eq!(*id, CompileId(1));
                assert_eq!(*revision, 3);
                assert_eq!(diagnostics.len(), 2);
                assert_eq!(*duration, Duration::from_millis(15));
            }
            other => panic!("expected a failed state, got {other:?}"),
        }
    }
}
