use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use super::diagnostics::{Diagnostic, DiagnosticId, DiagnosticSource, Severity};
use super::engine::{ArtifactKind, CompileError, CompileOutput, CompileRequest, DocumentEngine};

static NEXT_DIAG_ID: AtomicU64 = AtomicU64::new(1);

pub struct TypstEngine {
    executable: Option<PathBuf>,
    build_dir: PathBuf,
}

impl Default for TypstEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TypstEngine {
    pub fn new() -> Self {
        let executable = which_typst();
        let build_dir = std::env::temp_dir().join("graf_typst_session");
        Self {
            executable,
            build_dir,
        }
    }
}

impl DocumentEngine for TypstEngine {
    fn compile(&self, request: CompileRequest) -> Result<CompileOutput, CompileError> {
        let start = Instant::now();
        let compile_id = request.compile_id;
        let revision = request.revision;

        let build_path = self.build_dir.join(format!("job_{}", compile_id.0));
        fs::create_dir_all(&build_path).map_err(|err| CompileError {
            compile_id,
            revision,
            diagnostics: Vec::new(),
            message: format!("Failed to create Typst build directory: {err}"),
            duration: start.elapsed(),
        })?;

        let (input_file, cwd, output_pdf_name) = if let Some(root_doc) = &request.root_document {
            let file_stem = root_doc
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("main");
            let pdf_name = format!("{file_stem}.pdf");
            let cwd = request.project_root.as_deref().unwrap_or(&build_path);
            (root_doc.clone(), cwd, pdf_name)
        } else {
            let input_file = build_path.join("document.typ");
            fs::write(&input_file, &request.source).map_err(|err| CompileError {
                compile_id,
                revision,
                diagnostics: Vec::new(),
                message: format!("Failed to write Typst source: {err}"),
                duration: start.elapsed(),
            })?;
            (input_file, build_path.as_path(), "document.pdf".to_string())
        };

        let output_pdf = build_path.join(&output_pdf_name);

        let Some(executable) = &self.executable else {
            let message = "Typst is not installed or configured".to_string();
            return Err(CompileError {
                compile_id,
                revision,
                diagnostics: vec![Diagnostic {
                    id: DiagnosticId(NEXT_DIAG_ID.fetch_add(1, Ordering::Relaxed)),
                    severity: Severity::Error,
                    source: DiagnosticSource::Typst,
                    message: message.clone(),
                    file: request.root_document,
                    line: None,
                }],
                message,
                duration: start.elapsed(),
            });
        };

        let output = Command::new(executable)
            .arg("compile")
            .arg(&input_file)
            .arg(&output_pdf)
            .arg("--diagnostic-format")
            .arg("short")
            .current_dir(cwd)
            .output()
            .map_err(|error| CompileError {
                compile_id,
                revision,
                diagnostics: Vec::new(),
                message: format!("Failed to execute Typst: {error}"),
                duration: start.elapsed(),
            })?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostics = parse_typst_diagnostics(&format!("{stderr}\n{stdout}"));

        if !output.status.success() || !output_pdf.exists() {
            return Err(CompileError {
                compile_id,
                revision,
                diagnostics,
                message: if stderr.trim().is_empty() {
                    "Typst compilation failed".to_string()
                } else {
                    stderr.trim().to_string()
                },
                duration: start.elapsed(),
            });
        }

        let artifact = fs::read(&output_pdf).map_err(|error| CompileError {
            compile_id,
            revision,
            diagnostics: diagnostics.clone(),
            message: format!("Failed to read Typst PDF output: {error}"),
            duration: start.elapsed(),
        })?;

        Ok(CompileOutput {
            compile_id,
            revision,
            artifact,
            artifact_kind: ArtifactKind::Pdf,
            diagnostics,
            duration: start.elapsed(),
        })
    }
}

pub fn which_typst() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("GRAF_TYPST_PATH") {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }

    if let Ok(output) = Command::new("which").arg("typst").output()
        && output.status.success()
    {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path_str.is_empty() {
            let p = PathBuf::from(path_str);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    let common_paths = [
        "/opt/homebrew/bin/typst",
        "/usr/local/bin/typst",
        "/usr/bin/typst",
        "~/.cargo/bin/typst",
    ];

    for path in common_paths {
        let expanded = if let Some(stripped) = path.strip_prefix("~/") {
            if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(stripped)
            } else {
                PathBuf::from(path)
            }
        } else {
            PathBuf::from(path)
        };

        if expanded.is_file() {
            return Some(expanded);
        }
    }

    None
}

pub fn parse_typst_diagnostics(output: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("error:") || trimmed.starts_with("warning:") {
            let is_error = trimmed.starts_with("error:");
            let severity = if is_error {
                Severity::Error
            } else {
                Severity::Warning
            };

            let message = if is_error {
                trimmed.trim_start_matches("error:").trim()
            } else {
                trimmed.trim_start_matches("warning:").trim()
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
            let parts: Vec<&str> = loc_part.split(':').collect();
            if parts.len() >= 2 {
                let file_name = PathBuf::from(parts[0]);
                let line_num: Option<usize> = parts[1].parse().ok();

                if let Some(last) = diagnostics.last_mut() {
                    last.file = Some(file_name);
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
    fn reports_when_typst_is_unavailable() {
        let directory = tempfile::tempdir().unwrap();
        let engine = TypstEngine {
            executable: None,
            build_dir: directory.path().to_path_buf(),
        };
        let request = CompileRequest::simple("= Document", 1);
        let compile_id = request.compile_id;

        let error = engine.compile(request).unwrap_err();

        assert_eq!(error.compile_id, compile_id);
        assert_eq!(error.message, "Typst is not installed or configured");
        assert_eq!(error.diagnostics.len(), 1);
    }
}
