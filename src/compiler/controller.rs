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

use std::borrow::Cow;

impl CompileState {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleResult {
    pub completed_revision: u64,
    pub current_revision: u64,
}

pub struct CompilerController {
    current_revision: u64,
    state: CompileState,
    debounce_duration: Duration,
}

impl Default for CompilerController {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilerController {
    pub fn new() -> Self {
        Self::with_debounce(Duration::from_millis(150))
    }

    pub fn with_debounce(debounce_duration: Duration) -> Self {
        Self {
            current_revision: 0,
            state: CompileState::Idle,
            debounce_duration,
        }
    }

    pub fn state(&self) -> &CompileState {
        &self.state
    }

    pub fn status_text(&self) -> Cow<'static, str> {
        self.state.status_text()
    }

    pub fn current_revision(&self) -> u64 {
        self.current_revision
    }

    pub fn debounce_duration(&self) -> Duration {
        self.debounce_duration
    }

    pub fn reset(&mut self) {
        self.current_revision = 0;
        self.state = CompileState::Idle;
    }

    pub fn set_debounce_duration(&mut self, duration: Duration) {
        self.debounce_duration = duration;
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

    fn reject_if_stale(&mut self, id: CompileId, revision: u64) -> Result<(), StaleResult> {
        let is_active_compile = matches!(
            self.state,
            CompileState::Compiling {
                id: active_id,
                revision: active_revision,
            } if active_id == id && active_revision == revision
        );
        if revision < self.current_revision || !is_active_compile {
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

    pub fn handle_output(&mut self, output: CompileOutput) -> Result<(), StaleResult> {
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
    use crate::compiler::engine::ArtifactKind;

    #[test]
    fn test_initial_state() {
        let controller = CompilerController::new();
        assert_eq!(controller.state(), &CompileState::Idle);
        assert_eq!(controller.current_revision(), 0);
        assert_eq!(controller.status_text(), "ready");
    }

    #[test]
    fn reset_clears_revision_tracking() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(7);
        controller.begin_compile(CompileId(1), 7);
        controller
            .handle_output(CompileOutput {
                compile_id: CompileId(1),
                revision: 7,
                artifact: b"pdf".to_vec(),
                artifact_kind: ArtifactKind::Pdf,
                diagnostics: vec![],
                duration: Duration::from_millis(1),
            })
            .expect("current output should be accepted");

        controller.reset();

        assert_eq!(controller.current_revision(), 0);
        assert_eq!(controller.state(), &CompileState::Idle);
    }

    #[test]
    fn begin_compile_tracks_manual_compile_revision() {
        let mut controller = CompilerController::new();

        controller.begin_compile(CompileId(1), 4);

        assert_eq!(controller.current_revision(), 4);
    }

    #[test]
    fn source_edit_enters_waiting_state() {
        let mut controller = CompilerController::with_debounce(Duration::from_millis(100));

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
    }

    #[test]
    fn output_from_reset_compile_session_is_rejected() {
        let mut controller = CompilerController::new();
        controller.begin_compile(CompileId(1), 7);
        controller.reset();
        controller.begin_compile(CompileId(2), 0);

        let result = controller.handle_output(CompileOutput {
            compile_id: CompileId(1),
            revision: 7,
            artifact: b"old document".to_vec(),
            artifact_kind: ArtifactKind::Pdf,
            diagnostics: vec![],
            duration: Duration::from_millis(1),
        });

        assert_eq!(
            result,
            Err(StaleResult {
                completed_revision: 7,
                current_revision: 0,
            })
        );
        assert_eq!(
            controller.state(),
            &CompileState::Compiling {
                id: CompileId(2),
                revision: 0,
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
        assert_eq!(controller.status_text(), "1 error");
    }

    #[test]
    fn test_multiple_errors_status_text() {
        let mut controller = CompilerController::new();
        controller.on_source_edited(1);
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
