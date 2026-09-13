use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{info, warn};

use super::diagnostics::{Diagnostic, DiagnosticSource, Severity};
use super::engine::{CompileError, CompileOutput, CompileRequest, DocumentEngine};
use super::resolve::{ResolvedEngine, resolve};

const TECTONIC_COMMON_PATHS: &[&str] = &[
    "/opt/homebrew/bin/tectonic",
    "/usr/local/bin/tectonic",
    "/usr/bin/tectonic",
];

const WARM_UP_SOURCE: &str =
    "\\documentclass{article}\n\\begin{document}\nWarm-up.\n\\end{document}\n";

const KEEP_JOB_DIRS: usize = 2;
const PRUNE_MIN_IDLE: Duration = Duration::from_secs(60);

pub struct TectonicEngine {
    resolved: Option<ResolvedEngine>,
    build_dir: crate::util::TemporarySessionDir,
}

impl Default for TectonicEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TectonicEngine {
    pub fn new() -> Self {
        let resolved = resolve("tectonic", "GRAF_TECTONIC_PATH", TECTONIC_COMMON_PATHS);
        match &resolved {
            Some(engine) => info!(
                "tectonic: {} engine at {}",
                engine.source,
                engine.path.display()
            ),
            None => info!("tectonic: no engine found"),
        }
        let build_dir = crate::util::TemporarySessionDir::new("graf_tectonic");
        Self {
            resolved,
            build_dir,
        }
    }

    /// Test-only constructor; production engines resolve their own paths.
    #[cfg(test)]
    pub fn with_paths(executable: impl Into<PathBuf>, build_dir: impl Into<PathBuf>) -> Self {
        Self {
            resolved: Some(ResolvedEngine {
                path: executable.into(),
                source: super::resolve::EngineSource::System,
            }),
            build_dir: crate::util::TemporarySessionDir::from_path(build_dir.into()),
        }
    }
}

impl DocumentEngine for TectonicEngine {
    fn warm_up(&self) {
        let Some(engine) = &self.resolved else {
            info!("tectonic warm-up skipped: no engine found");
            return;
        };
        let request = CompileRequest::simple(WARM_UP_SOURCE, 0);
        match self.compile(request) {
            Ok(_) => info!("tectonic warm-up finished ({})", engine.source),
            Err(error) => warn!("tectonic warm-up failed: {}", error.message),
        }
    }

    fn compile(&self, request: CompileRequest) -> Result<CompileOutput, CompileError> {
        let start = Instant::now();
        let compile_id = request.compile_id;
        let revision = request.revision;

        let Some(engine) = &self.resolved else {
            let message = "Tectonic is not installed or configured".to_string();
            return Err(CompileError {
                compile_id,
                revision,
                diagnostics: vec![Diagnostic::new(
                    1,
                    Severity::Error,
                    DiagnosticSource::Tectonic,
                    request.root_document.clone(),
                    None,
                    message.clone(),
                )],
                message,
                duration: start.elapsed(),
            });
        };

        let build_path = self.build_dir.path().join(format!("job_{}", compile_id.0));
        fs::create_dir_all(&build_path).map_err(|err| CompileError {
            compile_id,
            revision,
            diagnostics: Vec::new(),
            message: format!("Failed to create build directory: {err}"),
            duration: start.elapsed(),
        })?;
        // One directory per compile with kept intermediates adds up over a
        // session. Age-guarded pruning leaves in-flight compiles alone.
        crate::util::prune_numbered_dirs(
            self.build_dir.path(),
            "job_",
            KEEP_JOB_DIRS,
            PRUNE_MIN_IDLE,
        );

        let (input_file, cwd, output_pdf_name) = if let Some(root_doc) = &request.root_document {
            let file_stem = root_doc
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("input");
            let pdf_name = format!("{file_stem}.pdf");
            let cwd = request.project_root.as_deref().unwrap_or(&build_path);
            (root_doc.clone(), cwd, pdf_name)
        } else {
            let input_file = build_path.join("input.tex");
            fs::write(&input_file, &request.source).map_err(|err| CompileError {
                compile_id,
                revision,
                diagnostics: Vec::new(),
                message: format!("Failed to write source to temporary file: {err}"),
                duration: start.elapsed(),
            })?;
            (input_file, build_path.as_path(), "input.pdf".to_string())
        };

        let output_pdf = build_path.join(output_pdf_name);

        let mut command = Command::new(&engine.path);
        command
            .arg("--keep-intermediates")
            .arg("-o")
            .arg(&build_path)
            .arg(&input_file)
            .current_dir(cwd);
        // Tectonic resolves \includegraphics and \input relative to the input
        // file's directory. Also search the project root so assets referenced
        // from nested documents resolve.
        if let Some(root) = request.project_root.as_deref() {
            command
                .arg("-Z")
                .arg(format!("search-path={}", root.display()));
        }
        if let Some(cache_dir) = support_cache_dir() {
            command.env("TECTONIC_CACHE_DIR", cache_dir);
        }
        let result = super::engine::run_with_cancel(command, request.cancel.as_ref());
        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => {
                return Err(CompileError {
                    compile_id,
                    revision,
                    diagnostics: Vec::new(),
                    message: "Compile cancelled by a newer edit".to_string(),
                    duration: start.elapsed(),
                });
            }
            Err(err) => {
                return Err(CompileError {
                    compile_id,
                    revision,
                    diagnostics: Vec::new(),
                    message: format!("Failed to execute tectonic: {err}"),
                    duration: start.elapsed(),
                });
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let duration = start.elapsed();
        let diagnostics = parse_tectonic_diagnostics_from_streams(stdout.lines(), stderr.lines());
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        if output.status.success() && !has_errors && output_pdf.exists() {
            let artifact: Arc<[u8]> = fs::read(&output_pdf)
                .map_err(|err| CompileError {
                    compile_id,
                    revision,
                    diagnostics: diagnostics.clone(),
                    message: format!("Failed to read compiled PDF output: {err}"),
                    duration,
                })?
                .into();

            Ok(CompileOutput {
                compile_id,
                revision,
                artifact,
                diagnostics,
                duration,
            })
        } else {
            let error_msg = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("\n");

            let error_msg = if error_msg.is_empty() {
                let trimmed = stderr.trim();
                if trimmed.is_empty() {
                    "Compilation failed with no error output".to_string()
                } else {
                    trimmed.to_string()
                }
            } else {
                error_msg
            };

            let fallback_diagnostics = if diagnostics.is_empty() {
                vec![Diagnostic::new(
                    1,
                    Severity::Error,
                    DiagnosticSource::Tectonic,
                    request.root_document,
                    None,
                    error_msg.clone(),
                )]
            } else {
                diagnostics
            };

            Err(CompileError {
                compile_id,
                revision,
                diagnostics: fallback_diagnostics,
                message: error_msg,
                duration,
            })
        }
    }
}

/// Tectonic downloads TeX support files on demand into a cache directory. Its
/// macOS default, ~/Library/Caches/Tectonic, reads as disposable, and a wiped
/// cache forces a full re-download on the next compile. Keep the cache in
/// Application Support instead. An explicit TECTONIC_CACHE_DIR wins.
fn support_cache_dir() -> Option<PathBuf> {
    support_cache_dir_with(
        std::env::var_os("TECTONIC_CACHE_DIR").is_some(),
        crate::util::home_dir().as_deref(),
    )
}

fn support_cache_dir_with(user_override: bool, home: Option<&Path>) -> Option<PathBuf> {
    if user_override {
        return None;
    }
    Some(PathBuf::from(home?).join("Library/Application Support/graf/compilers/tectonic-cache"))
}

/// Cap on parsed diagnostics so pathological builds (e.g. thousands of
/// repeated warnings) cannot balloon memory or stall the UI.
const MAX_DIAGNOSTICS: usize = 100;

#[cfg(test)]
pub(crate) fn parse_tectonic_diagnostics(log: &str) -> Vec<Diagnostic> {
    parse_tectonic_diagnostics_from_streams(log.lines(), std::iter::empty())
}

pub fn parse_tectonic_diagnostics_from_streams<'a>(
    stdout: impl Iterator<Item = &'a str>,
    stderr: impl Iterator<Item = &'a str>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut diag_id = 1u64;
    let mut lines = stdout
        .chain(stderr)
        .map(str::trim)
        .take(MAX_DIAGNOSTICS * 4)
        .peekable();

    while let Some(line) = lines.next() {
        if diagnostics.len() >= MAX_DIAGNOSTICS {
            break;
        }
        if let Some(msg) = line
            .strip_prefix("error:")
            .or_else(|| line.strip_prefix("fatal:"))
        {
            diagnostics.push(Diagnostic::new(
                diag_id,
                Severity::Error,
                DiagnosticSource::Tectonic,
                None,
                None,
                msg.trim(),
            ));
            diag_id += 1;
        } else if let Some(msg) = line.strip_prefix("warning:") {
            diagnostics.push(Diagnostic::new(
                diag_id,
                Severity::Warning,
                DiagnosticSource::Tectonic,
                None,
                None,
                msg.trim(),
            ));
            diag_id += 1;
        } else if let Some(msg) = line.strip_prefix('!') {
            let line_num = lines
                .peek()
                .and_then(|next| next.strip_prefix("l."))
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|num_str| num_str.parse::<usize>().ok());

            diagnostics.push(Diagnostic::new(
                diag_id,
                Severity::Error,
                DiagnosticSource::Tectonic,
                None,
                line_num,
                msg.trim(),
            ));
            diag_id += 1;
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::engine::test_assets;
    use std::path::Path;

    fn local_tectonic() -> Option<PathBuf> {
        resolve("tectonic", "GRAF_TECTONIC_PATH", TECTONIC_COMMON_PATHS).map(|engine| engine.path)
    }

    #[test]
    fn support_cache_dir_lands_in_application_support() {
        let dir = support_cache_dir_with(false, Some(Path::new("/Users/someone"))).unwrap();
        assert_eq!(
            dir,
            PathBuf::from(
                "/Users/someone/Library/Application Support/graf/compilers/tectonic-cache"
            )
        );
        assert!(support_cache_dir_with(true, Some(Path::new("/Users/someone"))).is_none());
        assert!(support_cache_dir_with(false, None).is_none());
    }

    #[test]
    fn reports_when_tectonic_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let engine = TectonicEngine {
            resolved: None,
            build_dir: crate::util::TemporarySessionDir::from_path(directory.path()),
        };
        let request = CompileRequest::simple("\\documentclass{article}", 1);
        let compile_id = request.compile_id;

        let error = engine.compile(request).unwrap_err();

        assert_eq!(error.compile_id, compile_id);
        assert_eq!(error.message, "Tectonic is not installed or configured");
        assert_eq!(error.diagnostics.len(), 1);
    }

    #[test]
    fn test_tectonic_compile_valid_latex() {
        let temp = tempfile::tempdir().unwrap();
        let Some(executable) = local_tectonic() else {
            eprintln!("tectonic not installed; skipping");
            return;
        };
        let engine = TectonicEngine::with_paths(executable, temp.path());
        let source = r#"\documentclass{article}
\begin{document}
Hello from Tectonic Engine Test.
\end{document}
"#;
        let request = CompileRequest::simple(source, 1);
        let result = engine.compile(request);

        assert!(
            result.is_ok(),
            "Expected compilation to succeed: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert_eq!(output.revision, 1);
        assert!(!output.artifact.is_empty());
        assert!(output.artifact.starts_with(b"%PDF-"));
    }

    #[test]
    fn test_tectonic_compile_invalid_latex() {
        let temp = tempfile::tempdir().unwrap();
        let Some(executable) = local_tectonic() else {
            eprintln!("tectonic not installed; skipping");
            return;
        };
        let engine = TectonicEngine::with_paths(executable, temp.path());
        let source = r#"\documentclass{article}
\begin{document}
\nonexistentcommandhere
\end{document}
"#;
        let request = CompileRequest::simple(source, 2);
        let result = engine.compile(request);

        assert!(
            result.is_err(),
            "Expected compilation to fail for invalid LaTeX"
        );
        let err = result.unwrap_err();
        assert_eq!(err.revision, 2);
        assert!(!err.diagnostics.is_empty());
        let has_error_diag = err
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
        assert!(has_error_diag);
    }

    #[test]
    fn test_tectonic_compile_with_image() {
        // Skip rather than fail on machines without tectonic; CI installs it.
        let Some(executable) = local_tectonic() else {
            eprintln!("tectonic not installed; skipping image test");
            return;
        };
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("chapters")).unwrap();
        fs::create_dir_all(project.path().join("assets")).unwrap();
        fs::write(
            project.path().join("assets/img.png"),
            test_assets::ONE_BY_ONE_PNG,
        )
        .unwrap();
        let main_tex = project.path().join("chapters/main.tex");
        fs::write(
            &main_tex,
            r#"\documentclass{article}
\usepackage{graphicx}
\begin{document}
Image: \includegraphics{assets/img.png}
\end{document}
"#,
        )
        .unwrap();

        let temp_build = tempfile::tempdir().unwrap();
        let engine = TectonicEngine::with_paths(executable, temp_build.path());
        let request = CompileRequest::with_project(
            fs::read_to_string(&main_tex).unwrap(),
            1,
            Some(project.path().to_path_buf()),
            Some(main_tex),
        );

        let output = engine
            .compile(request)
            .expect("compile with project asset should succeed");
        assert!(output.artifact.starts_with(b"%PDF-"));
    }

    #[test]
    fn test_tectonic_compile_multi_file_project() {
        let temp_proj = tempfile::tempdir().unwrap();
        let proj_dir = temp_proj.path();
        let temp_build = tempfile::tempdir().unwrap();

        fs::create_dir_all(proj_dir.join("sections")).unwrap();
        let main_tex = proj_dir.join("main.tex");
        fs::write(
            &main_tex,
            r#"\documentclass{article}
\begin{document}
\input{sections/intro.tex}
\end{document}
"#,
        )
        .unwrap();

        fs::write(
            proj_dir.join("sections/intro.tex"),
            "This is content from a multi-file LaTeX project subfolder.\n",
        )
        .unwrap();

        let Some(executable) = local_tectonic() else {
            eprintln!("tectonic not installed; skipping");
            return;
        };
        let engine = TectonicEngine::with_paths(executable, temp_build.path());

        let request = CompileRequest::with_project(
            fs::read_to_string(&main_tex).unwrap(),
            1,
            Some(proj_dir.to_path_buf()),
            Some(main_tex),
        );

        let result = engine.compile(request);
        assert!(
            result.is_ok(),
            "Expected multi-file compilation to succeed: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert_eq!(output.revision, 1);
        assert!(output.artifact.starts_with(b"%PDF-"));
    }

    #[test]
    fn test_diagnostic_parsing() {
        let log = r#"
! Undefined control sequence.
l.5 \invalidcmd
note: rerun with tectonic -X
warning: unused label
"#;
        let diags = parse_tectonic_diagnostics(log);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].line, Some(5));
        assert!(diags[0].message.contains("Undefined control sequence"));

        assert_eq!(diags[1].severity, Severity::Warning);
        assert!(diags[1].message.contains("unused label"));
    }

    #[test]
    fn test_diagnostic_parsing_fatal_and_multiple() {
        let log = r#"
fatal: file 'missing.sty' not found
warning: citation 'xyz' undefined
error: missing \begin{document}
"#;
        let diags = parse_tectonic_diagnostics(log);
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].message, "file 'missing.sty' not found");
        assert_eq!(diags[1].severity, Severity::Warning);
        assert_eq!(diags[1].message, "citation 'xyz' undefined");
        assert_eq!(diags[2].severity, Severity::Error);
        assert_eq!(diags[2].message, "missing \\begin{document}");
    }

    #[test]
    fn test_tectonic_parses_stderr_and_stdout_without_concatenation() {
        let stdout = "note: rerun with tectonic -X\n";
        let stderr = "! Undefined control sequence.\nl.5 \\invalidcmd\nwarning: unused label\n";

        let diags = parse_tectonic_diagnostics_from_streams(stdout.lines(), stderr.lines());
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].line, Some(5));
        assert_eq!(diags[1].severity, Severity::Warning);
    }

    #[test]
    fn test_tectonic_diagnostics_capped_at_limit() {
        let log: String = (0..250).map(|i| format!("error: problem {i}\n")).collect();

        let diags = parse_tectonic_diagnostics(&log);
        assert_eq!(diags.len(), MAX_DIAGNOSTICS);
    }
}
