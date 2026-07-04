//! PDF rasterization and rendering abstraction.
//!
//! Provides the `PdfRenderer` trait and a native rasterizer for turning
//! PDF bytes into renderable image pages.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Rendered page metadata and image artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub image_path: PathBuf,
}

/// Abstraction for rendering PDF documents into image pages.
pub trait PdfRenderer: Send + Sync {
    /// Renders a PDF document given by its raw bytes to one or more image pages.
    fn render_document(&self, revision: u64, pdf_bytes: &[u8])
    -> Result<Vec<RenderedPage>, String>;
}

/// Native macOS PDF rasterizer utilizing system image tools.
pub struct NativePdfRenderer {
    cache_dir: PathBuf,
}

impl Default for NativePdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePdfRenderer {
    /// Creates a new `NativePdfRenderer` with an isolated cache directory.
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("graf_pdf_cache");
        fs::create_dir_all(&temp_dir).ok();
        Self {
            cache_dir: temp_dir,
        }
    }
}

impl PdfRenderer for NativePdfRenderer {
    fn render_document(
        &self,
        revision: u64,
        pdf_bytes: &[u8],
    ) -> Result<Vec<RenderedPage>, String> {
        if pdf_bytes.is_empty() || !pdf_bytes.starts_with(b"%PDF-") {
            return Err("Invalid or empty PDF data".to_string());
        }

        let run_dir = self.cache_dir.join(format!("rev_{revision}"));
        fs::create_dir_all(&run_dir).map_err(|e| format!("Failed to create rev dir: {e}"))?;

        let pdf_file = run_dir.join("document.pdf");
        let png_file = run_dir.join("page_1.png");

        fs::write(&pdf_file, pdf_bytes).map_err(|e| format!("Failed to write PDF: {e}"))?;
        let _ = fs::remove_file(&png_file);

        let output = Command::new("/usr/bin/sips")
            .arg("-s")
            .arg("format")
            .arg("png")
            .arg("--resampleWidth")
            .arg("1224")
            .arg(&pdf_file)
            .arg("--out")
            .arg(&png_file)
            .output()
            .map_err(|e| format!("Failed to execute sips: {e}"))?;

        if !output.status.success() || !png_file.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("sips rasterization failed: {stderr}"));
        }

        Ok(vec![RenderedPage {
            page_index: 0,
            width: 612,
            height: 792,
            image_path: png_file,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::engine::{CompileRequest, DocumentEngine};
    use crate::compiler::tectonic::TectonicEngine;

    #[test]
    fn test_invalid_pdf_bytes() {
        let renderer = NativePdfRenderer::new();
        let result = renderer.render_document(1, b"not a pdf");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_pdf_rasterization() {
        let engine = TectonicEngine::new();
        let request = CompileRequest::simple(
            r#"\documentclass{article}\begin{document}Hello Preview Renderer Test\end{document}"#,
            1,
        );
        let compile_output = engine.compile(request).expect("compile must succeed");

        let renderer = NativePdfRenderer::new();
        let result = renderer.render_document(1, &compile_output.artifact);
        assert!(result.is_ok(), "Rasterization failed: {:?}", result.err());

        let pages = result.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page_index, 0);
        assert!(pages[0].image_path.exists());
    }
}
