use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::diagnostics::{Diagnostic, DiagnosticSource, Severity};
use super::engine::{
    ArtifactKind, CompileError, CompileId, CompileOutput, CompileRequest, DocumentEngine,
};

static NEXT_COMPILE_ID: AtomicU64 = AtomicU64::new(1);

pub struct TectonicEngine {
    executable: PathBuf,
    build_dir: PathBuf,
}

impl Default for TectonicEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TectonicEngine {
    pub fn new() -> Self {
        let executable = which_tectonic().unwrap_or_else(|| PathBuf::from("tectonic"));
        let build_dir = std::env::temp_dir().join("graf_tectonic_session");
        let _ = fs::create_dir_all(&build_dir);
        Self {
            executable,
            build_dir,
        }
    }

    pub fn with_executable(path: impl Into<PathBuf>) -> Self {
        let build_dir = std::env::temp_dir().join("graf_tectonic_session");
        let _ = fs::create_dir_all(&build_dir);
        Self {
            executable: path.into(),
            build_dir,
        }
    }

    pub fn with_paths(executable: impl Into<PathBuf>, build_dir: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            build_dir: build_dir.into(),
        }
    }
}

impl DocumentEngine for TectonicEngine {
    fn compile(&self, request: CompileRequest) -> Result<CompileOutput, CompileError> {
        let start = Instant::now();
        let compile_id = CompileId(NEXT_COMPILE_ID.fetch_add(1, Ordering::Relaxed));
        let revision = request.revision;

        let build_path = self.build_dir.join(format!("job_{}", compile_id.0));
        fs::create_dir_all(&build_path).map_err(|err| CompileError {
            compile_id,
            revision,
            diagnostics: Vec::new(),
            message: format!("Failed to create build directory: {err}"),
            duration: start.elapsed(),
        })?;

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
        let _ = fs::remove_file(&output_pdf);

        let output = Command::new(&self.executable)
            .arg("--keep-intermediates")
            .arg("-o")
            .arg(&build_path)
            .arg(&input_file)
            .current_dir(cwd)
            .output()
            .map_err(|err| CompileError {
                compile_id,
                revision,
                diagnostics: Vec::new(),
                message: format!("Failed to execute tectonic: {err}"),
                duration: start.elapsed(),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined_log = format!("{stdout}\n{stderr}");

        let duration = start.elapsed();
        let diagnostics = parse_tectonic_diagnostics(&combined_log);
        let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

        if output.status.success() && !has_errors && output_pdf.exists() {
            let artifact = fs::read(&output_pdf).map_err(|err| CompileError {
                compile_id,
                revision,
                diagnostics: diagnostics.clone(),
                message: format!("Failed to read compiled PDF output: {err}"),
                duration,
            })?;

            Ok(CompileOutput {
                compile_id,
                revision,
                artifact,
                artifact_kind: ArtifactKind::Pdf,
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

fn which_tectonic() -> Option<PathBuf> {
    [
        "/opt/homebrew/bin/tectonic",
        "/usr/local/bin/tectonic",
        "/usr/bin/tectonic",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|p| p.exists())
}

pub fn parse_tectonic_diagnostics(log: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut diag_id = 1u64;
    let mut lines = log.lines().map(str::trim).peekable();

    while let Some(line) = lines.next() {
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

    #[test]
    fn test_tectonic_compile_valid_latex() {
        let temp = tempfile::tempdir().unwrap();
        let engine = TectonicEngine::with_paths(
            which_tectonic().unwrap_or_else(|| PathBuf::from("tectonic")),
            temp.path(),
        );
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
        assert_eq!(output.artifact_kind, ArtifactKind::Pdf);
        assert!(!output.artifact.is_empty());
        assert!(output.artifact.starts_with(b"%PDF-"));
    }

    #[test]
    fn test_tectonic_compile_invalid_latex() {
        let temp = tempfile::tempdir().unwrap();
        let engine = TectonicEngine::with_paths(
            which_tectonic().unwrap_or_else(|| PathBuf::from("tectonic")),
            temp.path(),
        );
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

        let engine = TectonicEngine::with_paths(
            which_tectonic().unwrap_or_else(|| PathBuf::from("tectonic")),
            temp_build.path(),
        );

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
}
