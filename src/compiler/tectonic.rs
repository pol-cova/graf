use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use log::{info, warn};

use super::diagnostics::{Diagnostic, DiagnosticSource, Severity};
use super::engine::{
    CompileError, CompileOutput, CompileRequest, DocumentEngine, next_diagnostic_id,
};
use super::resolve::{LazyEngine, tectonic as tectonic_spec};

const WARM_UP_SOURCE: &str =
    "\\documentclass{article}\n\\begin{document}\nWarm-up.\n\\end{document}\n";

const KEEP_JOB_DIRS: usize = 2;
const PRUNE_MIN_IDLE: Duration = Duration::from_secs(60);

/// Engine spec that resolves once, lazily, off the UI thread: the first
/// warm-up or compile call performs the `which`/path probing and caches it.
pub struct TectonicEngine {
    resolved: LazyEngine,
    build_dir: crate::util::TemporarySessionDir,
}

impl Default for TectonicEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TectonicEngine {
    pub fn new() -> Self {
        Self {
            resolved: tectonic_spec(),
            build_dir: crate::util::TemporarySessionDir::new("graf_tectonic"),
        }
    }

    /// Test-only constructor; production engines resolve their own paths.
    #[cfg(test)]
    pub fn with_paths(executable: impl Into<PathBuf>, build_dir: impl Into<PathBuf>) -> Self {
        Self {
            resolved: LazyEngine::with_resolution(Some(super::resolve::ResolvedEngine {
                path: executable.into(),
                source: super::resolve::EngineSource::System,
            })),
            build_dir: crate::util::TemporarySessionDir::from_path(build_dir.into()),
        }
    }
}

impl DocumentEngine for TectonicEngine {
    fn warm_up(&self) {
        // First touch of `resolved` runs here, on the background warm-up
        // thread; the UI thread never waits on engine probing.
        let Some(engine) = self.resolved.get() else {
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

        let Some(engine) = self.resolved.get() else {
            let identity = super::engine::EngineIdentity {
                label: "tectonic",
                display_name: "Tectonic",
                diagnostic_source: DiagnosticSource::Tectonic,
            };
            return Err(identity.unavailable_error(
                compile_id,
                revision,
                request.source_document().map(Path::to_path_buf),
                start,
            ));
        };
        let request = &request;
        let identity = super::engine::EngineIdentity {
            label: "tectonic",
            display_name: "Tectonic",
            diagnostic_source: DiagnosticSource::Tectonic,
        };

        let job = super::engine::prepare_job(
            &super::engine::JobDirs {
                build_root: &self.build_dir,
                keep_dirs: KEEP_JOB_DIRS,
                prune_min_idle: PRUNE_MIN_IDLE,
            },
            request,
            "input",
            "tex",
            identity,
        )?;

        let mut command = Command::new(&engine.path);
        command
            .arg("--keep-intermediates")
            .arg("-o")
            .arg(&job.build_path)
            .arg(&job.input_file)
            .current_dir(&job.cwd);
        // Tectonic resolves \includegraphics and \input relative to the
        // input file's directory. Also search the project root so assets
        // referenced from nested documents resolve.
        if let Some(root) = request.project_root.as_deref() {
            command
                .arg("-Z")
                .arg(format!("search-path={}", root.display()));
        }
        if let Some(cache_dir) = support_cache_dir() {
            command.env("TECTONIC_CACHE_DIR", cache_dir);
        }

        let output = super::engine::run_compile_subprocess(command, request, start, identity)?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let diagnostics = parse_tectonic_diagnostics_from_streams(stdout.lines(), stderr.lines());

        super::engine::finalize_output(
            request,
            &job,
            start,
            identity,
            super::engine::RunOutcome {
                status: output.status,
                diagnostics,
                raw_failure_message: if stderr.trim().is_empty() {
                    Some("Compilation failed with no error output".to_string())
                } else {
                    Some(stderr.trim().to_string())
                },
            },
        )
    }
}

/// Tectonic downloads TeX support files on demand into a cache directory. Its
/// macOS default, ~/Library/Caches/Tectonic, reads as disposable, and a wiped
/// cache forces a full re-download on the next compile. Keep the cache in
/// the app's per-user data directory (no macOS assumption in the engine
/// module itself: the per-OS routing lives in `util`). An explicit
/// TECTONIC_CACHE_DIR wins.
fn support_cache_dir() -> Option<PathBuf> {
    support_cache_dir_with(
        std::env::var_os("TECTONIC_CACHE_DIR").is_some(),
        crate::util::app_data_dir().as_deref(),
    )
}

fn support_cache_dir_with(user_override: bool, app_data_dir: Option<&Path>) -> Option<PathBuf> {
    if user_override {
        return None;
    }
    Some(app_data_dir?.join("compilers/tectonic-cache"))
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
    // Same cap policy as the typst backend: stop once the cap is reached
    // inside the loop, without truncating the stream up front (a truncated
    // prefix could miss errors that arrive later in the stream).
    let mut lines = stdout.chain(stderr).peekable();

    while diagnostics.len() < MAX_DIAGNOSTICS
        && let Some(line) = lines.next()
    {
        let trimmed = line.trim();
        if let Some(msg) = trimmed
            .strip_prefix("error:")
            .or_else(|| trimmed.strip_prefix("fatal:"))
        {
            diagnostics.push(Diagnostic::new(
                next_diagnostic_id().0,
                Severity::Error,
                DiagnosticSource::Tectonic,
                None,
                None,
                msg.trim(),
            ));
        } else if let Some(msg) = trimmed.strip_prefix("warning:") {
            diagnostics.push(Diagnostic::new(
                next_diagnostic_id().0,
                Severity::Warning,
                DiagnosticSource::Tectonic,
                None,
                None,
                msg.trim(),
            ));
        } else if let Some(msg) = trimmed.strip_prefix('!') {
            // The line number trails immediately after the bang line.
            let line_num = lines
                .peek()
                .map(|next| next.trim())
                .unwrap_or_default()
                .strip_prefix("l.")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|num_str| num_str.parse::<usize>().ok());

            diagnostics.push(Diagnostic::new(
                next_diagnostic_id().0,
                Severity::Error,
                DiagnosticSource::Tectonic,
                None,
                line_num,
                msg.trim(),
            ));
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::engine::test_assets;
    use crate::compiler::resolve::{TECTONIC_COMMON_PATHS, resolve};
    use std::fs;
    use std::path::Path;

    fn local_tectonic() -> Option<PathBuf> {
        resolve("tectonic", "GRAF_TECTONIC_PATH", TECTONIC_COMMON_PATHS).map(|engine| engine.path)
    }

    #[test]
    fn support_cache_dir_lands_in_the_app_data_directory() {
        let dir = support_cache_dir_with(
            false,
            Some(Path::new("/Users/someone/Library/Application Support/graf")),
        )
        .unwrap();
        assert_eq!(
            dir,
            PathBuf::from(
                "/Users/someone/Library/Application Support/graf/compilers/tectonic-cache"
            )
        );
        assert!(support_cache_dir_with(true, None).is_none());
        assert!(support_cache_dir_with(false, None).is_none());
    }

    #[test]
    fn reports_when_tectonic_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let engine = TectonicEngine {
            resolved: LazyEngine::with_resolution(None),
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
