use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use log::info;

use super::diagnostics::{Diagnostic, DiagnosticId, DiagnosticSource, Severity};
use super::engine::{CompileError, CompileOutput, CompileRequest, DocumentEngine};
use super::resolve::{ResolvedEngine, resolve};

/// Intermediates retention, in line with the tectonic backend; both keep the
/// newest two job dirs when idle for at least a minute.
const KEEP_JOB_DIRS: usize = 2;
const PRUNE_MIN_IDLE: Duration = Duration::from_secs(60);

const TYPST_COMMON_PATHS: &[&str] = &[
    "/opt/homebrew/bin/typst",
    "/usr/local/bin/typst",
    "/usr/bin/typst",
    "~/.cargo/bin/typst",
];

static NEXT_DIAG_ID: AtomicU64 = AtomicU64::new(1);

pub struct TypstEngine {
    resolved: Option<ResolvedEngine>,
    build_dir: crate::util::TemporarySessionDir,
}

impl Default for TypstEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TypstEngine {
    pub fn new() -> Self {
        let resolved = resolve("typst", "GRAF_TYPST_PATH", TYPST_COMMON_PATHS);
        match &resolved {
            Some(engine) => info!(
                "typst: {} engine at {}",
                engine.source,
                engine.path.display()
            ),
            None => info!("typst: no engine found"),
        }
        let build_dir = crate::util::TemporarySessionDir::new("graf_typst");
        Self {
            resolved,
            build_dir,
        }
    }
}

impl DocumentEngine for TypstEngine {
    fn compile(&self, request: CompileRequest) -> Result<CompileOutput, CompileError> {
        let start = Instant::now();
        let compile_id = request.compile_id;
        let revision = request.revision;

        let Some(engine) = &self.resolved else {
            let message = "Typst is not installed or configured".to_string();
            return Err(CompileError {
                compile_id,
                revision,
                diagnostics: vec![Diagnostic {
                    id: DiagnosticId(NEXT_DIAG_ID.fetch_add(1, Ordering::Relaxed)),
                    severity: Severity::Error,
                    source: DiagnosticSource::Typst,
                    message: message.clone(),
                    file: request.root_document.clone(),
                    line: None,
                }],
                message,
                duration: start.elapsed(),
            });
        };
        let request = &request;

        let identity = super::engine::EngineIdentity {
            label: "Typst",
            diagnostic_source: DiagnosticSource::Typst,
        };
        let job = super::engine::prepare_job(
            &super::engine::JobDirs {
                build_root: &self.build_dir,
                keep_dirs: KEEP_JOB_DIRS,
                prune_min_idle: PRUNE_MIN_IDLE,
            },
            request,
            "document",
            "typ",
            identity,
        )?;

        let mut command = Command::new(&engine.path);
        command
            .arg("compile")
            .arg(&job.input_file)
            .arg(&job.output_pdf)
            .arg("--diagnostic-format")
            .arg("short")
            .current_dir(&job.cwd);
        // Typst resolves image paths relative to the input file and refuses to
        // read outside its project root. Widening the root to the project lets
        // documents in subfolders reference project-level assets.
        if let Some(root) = request.project_root.as_deref() {
            command.arg("--root").arg(root);
        }

        let output = super::engine::run_compile_subprocess(command, request, start, identity)?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = parse_typst_diagnostics_from_streams(stderr.lines(), stdout.lines());

        // Same success predicate as tectonic (a previously divergent rule:
        // typst used to accept error-grade diagnostics whenever the file
        // existed); one shared finalize_output keeps both engines aligned.
        super::engine::finalize_output(
            request,
            &job,
            start,
            identity,
            super::engine::RunOutcome {
                status: output.status,
                diagnostics,
                raw_failure_message: if stderr.trim().is_empty() {
                    None
                } else {
                    Some(stderr.trim().to_string())
                },
            },
        )
    }
}

/// Cap on parsed diagnostics so pathological builds cannot balloon memory.
const MAX_DIAGNOSTICS: usize = 100;

#[cfg(test)]
pub(crate) fn parse_typst_diagnostics(log: &str) -> Vec<Diagnostic> {
    parse_typst_diagnostics_from_streams(log.lines(), std::iter::empty())
}

pub fn parse_typst_diagnostics_from_streams<'a>(
    stderr: impl Iterator<Item = &'a str>,
    stdout: impl Iterator<Item = &'a str>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for line in stderr.chain(stdout) {
        if diagnostics.len() >= MAX_DIAGNOSTICS {
            break;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("error:") || trimmed.starts_with("warning:") {
            let is_error = trimmed.starts_with("error:");
            let severity = if is_error {
                Severity::Error
            } else {
                Severity::Warning
            };

            let message = if is_error {
                trimmed.strip_prefix("error:").unwrap_or(trimmed).trim()
            } else {
                trimmed.strip_prefix("warning:").unwrap_or(trimmed).trim()
            };

            diagnostics.push(Diagnostic {
                id: DiagnosticId(NEXT_DIAG_ID.fetch_add(1, Ordering::Relaxed)),
                severity,
                source: DiagnosticSource::Typst,
                message: message.to_string(),
                file: None,
                line: None,
            });
        } else if trimmed.starts_with("-->") {
            let loc_part = trimmed.trim_start_matches("-->").trim();
            if let Some((file_name, rest)) = loc_part.split_once(':') {
                let line_num: Option<usize> = rest
                    .split(':')
                    .next()
                    .and_then(|num| num.trim().parse().ok());

                if let Some(last) = diagnostics.last_mut() {
                    last.file = Some(PathBuf::from(file_name));
                    last.line = line_num;
                }
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::resolve::EngineSource;
    use std::fs;

    use std::path::Path;

    #[test]
    fn test_typst_diagnostic_parsing() {
        let log = r#"error: expected string, found integer
  --> main.typ:5:10
warning: variable 'x' is never used
  --> main.typ:12:4"#;

        let diags = parse_typst_diagnostics(log);
        assert_eq!(diags.len(), 2);

        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].message, "expected string, found integer");
        assert_eq!(diags[0].file.as_deref(), Some(Path::new("main.typ")));
        assert_eq!(diags[0].line, Some(5));

        assert_eq!(diags[1].severity, Severity::Warning);
        assert_eq!(diags[1].line, Some(12));
    }

    #[test]
    fn test_typst_parses_stderr_and_stdout_without_concatenation() {
        let stderr = "error: expected string, found integer\n";
        let stdout = "warning: variable 'x' is never used\n";

        let diags = parse_typst_diagnostics_from_streams(stderr.lines(), stdout.lines());
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[1].severity, Severity::Warning);
    }

    #[test]
    fn test_typst_diagnostics_capped_at_limit() {
        let log: String = (0..250).map(|i| format!("error: problem {i}\n")).collect();

        let diags = parse_typst_diagnostics(&log);
        assert_eq!(diags.len(), MAX_DIAGNOSTICS);
    }

    #[test]
    fn test_typst_location_with_column_parses_file_and_line() {
        let log = "error: bad math\n  --> chapters/intro.typ:42:13\n";
        let diags = parse_typst_diagnostics(log);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].file.as_deref(),
            Some(Path::new("chapters/intro.typ"))
        );
        assert_eq!(diags[0].line, Some(42));
    }

    #[test]
    fn test_typst_compile_with_image() {
        // Skip rather than fail on machines without typst; CI does not install it.
        let Some(executable) =
            resolve("typst", "GRAF_TYPST_PATH", TYPST_COMMON_PATHS).map(|engine| engine.path)
        else {
            eprintln!("typst not installed; skipping image test");
            return;
        };
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("chapters")).unwrap();
        fs::create_dir_all(project.path().join("assets")).unwrap();
        fs::write(
            project.path().join("assets/img.png"),
            crate::compiler::engine::test_assets::ONE_BY_ONE_PNG,
        )
        .unwrap();
        let main_typ = project.path().join("chapters/main.typ");
        fs::write(&main_typ, "#image(\"../assets/img.png\")\n").unwrap();

        let temp_build = tempfile::tempdir().unwrap();
        let engine = TypstEngine {
            resolved: Some(ResolvedEngine {
                path: executable,
                source: EngineSource::System,
            }),
            build_dir: crate::util::TemporarySessionDir::from_path(temp_build.path()),
        };
        let request = CompileRequest::with_project(
            fs::read_to_string(&main_typ).unwrap(),
            1,
            Some(project.path().to_path_buf()),
            Some(main_typ),
        );

        let output = engine
            .compile(request)
            .expect("compile with project asset should succeed");
        assert!(output.artifact.starts_with(b"%PDF-"));
    }

    #[test]
    fn reports_when_typst_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let engine = TypstEngine {
            resolved: None,
            build_dir: crate::util::TemporarySessionDir::from_path(directory.path()),
        };
        let request = CompileRequest::simple("= Document", 1);
        let compile_id = request.compile_id;

        let error = engine.compile(request).unwrap_err();

        assert_eq!(error.compile_id, compile_id);
        assert_eq!(error.message, "Typst is not installed or configured");
        assert_eq!(error.diagnostics.len(), 1);
    }
}
