use std::fs;
use std::path::PathBuf;
use std::process::Command;

const PREVIEW_RASTER_WIDTH: &str = "1224";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub image_path: PathBuf,
}

pub trait PdfRenderer: Send + Sync {
    fn render_document(
        &self,
        render_id: u64,
        pdf_bytes: &[u8],
    ) -> Result<Vec<RenderedPage>, String>;
}

pub struct NativePdfRenderer {
    cache_dir: PathBuf,
}

impl Default for NativePdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePdfRenderer {
    pub fn new() -> Self {
        Self {
            cache_dir: std::env::temp_dir().join("graf_pdf_cache"),
        }
    }
}

impl PdfRenderer for NativePdfRenderer {
    fn render_document(
        &self,
        render_id: u64,
        pdf_bytes: &[u8],
    ) -> Result<Vec<RenderedPage>, String> {
        if pdf_bytes.is_empty() || !pdf_bytes.starts_with(b"%PDF-") {
            return Err("Invalid or empty PDF data".to_string());
        }

        let run_dir = self.cache_dir.join(format!("render_{render_id}"));
        fs::create_dir_all(&run_dir)
            .map_err(|error| format!("Failed to create preview directory: {error}"))?;

        let pdf_file = run_dir.join("document.pdf");
        let png_file = run_dir.join("page_1.png");

        fs::write(&pdf_file, pdf_bytes).map_err(|e| format!("Failed to write PDF: {e}"))?;

        #[cfg(target_os = "macos")]
        let (tool, output) = (
            "sips",
            Command::new("/usr/bin/sips")
                .arg("-s")
                .arg("format")
                .arg("png")
                .arg("--resampleWidth")
                .arg(PREVIEW_RASTER_WIDTH)
                .arg(&pdf_file)
                .arg("--out")
                .arg(&png_file)
                .output(),
        );

        #[cfg(not(target_os = "macos"))]
        let (tool, output) = {
            let output_root = run_dir.join("page_1");
            (
                "pdftoppm",
                Command::new("pdftoppm")
                    .arg("-png")
                    .arg("-f")
                    .arg("1")
                    .arg("-singlefile")
                    .arg("-scale-to-x")
                    .arg(PREVIEW_RASTER_WIDTH)
                    .arg("-scale-to-y")
                    .arg("-1")
                    .arg(&pdf_file)
                    .arg(output_root)
                    .output(),
            )
        };

        let output = output.map_err(|error| format!("Failed to execute {tool}: {error}"))?;
        if !output.status.success() || !png_file.exists() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("{tool} rasterization failed: {stderr}"));
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
