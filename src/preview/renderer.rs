use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

const PREVIEW_RASTER_WIDTH: &str = "1224";
const PAGE_PREFIX: &str = "page";

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

    fn rasterize_with_pdftoppm(&self, pdf_file: &Path, run_dir: &Path) -> Option<String> {
        let output_root = run_dir.join(PAGE_PREFIX);
        let output = Command::new("pdftoppm")
            .arg("-png")
            .arg("-scale-to-x")
            .arg(PREVIEW_RASTER_WIDTH)
            .arg("-scale-to-y")
            .arg("-1")
            .arg(pdf_file)
            .arg(&output_root)
            .output()
            .ok()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Some(format!("pdftoppm rasterization failed: {stderr}"));
        }
        None
    }

    #[cfg(target_os = "macos")]
    fn rasterize_with_sips(&self, pdf_file: &Path) -> Option<String> {
        let png_file = pdf_file.parent()?.join(format!("{PAGE_PREFIX}-1.png"));
        let output = Command::new("/usr/bin/sips")
            .arg("-s")
            .arg("format")
            .arg("png")
            .arg("--resampleWidth")
            .arg(PREVIEW_RASTER_WIDTH)
            .arg(pdf_file)
            .arg("--out")
            .arg(&png_file)
            .output()
            .ok()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Some(format!("sips rasterization failed: {stderr}"));
        }
        None
    }

    #[cfg(not(target_os = "macos"))]
    fn rasterize_with_sips(&self, _pdf_file: &Path) -> Option<String> {
        None
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
        fs::write(&pdf_file, pdf_bytes).map_err(|error| format!("Failed to write PDF: {error}"))?;

        let failure = self
            .rasterize_with_pdftoppm(&pdf_file, &run_dir)
            .or_else(|| self.rasterize_with_sips(&pdf_file));

        if let Some(message) = failure {
            return Err(message);
        }

        let mut page_numbers = page_numbers_in(&run_dir)?;
        if page_numbers.is_empty() {
            return Err("Rasterization produced no pages".to_string());
        }
        page_numbers.sort_unstable();

        let pages = page_numbers
            .into_iter()
            .enumerate()
            .map(|(index, number)| {
                let image_path = run_dir.join(format!("{PAGE_PREFIX}-{number}.png"));
                let (width, height) = png_dimensions(&image_path)
                    .map_err(|error| format!("Failed to read page image: {error}"))?;
                Ok(RenderedPage {
                    page_index: index,
                    width,
                    height,
                    image_path,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(pages)
    }
}

fn page_numbers_in(run_dir: &Path) -> Result<Vec<u32>, String> {
    let entries = fs::read_dir(run_dir)
        .map_err(|error| format!("Failed to list preview directory: {error}"))?;

    let mut numbers = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(rest) = name.strip_prefix(&format!("{PAGE_PREFIX}-")) else {
            continue;
        };
        let Some(number) = rest.strip_suffix(".png") else {
            continue;
        };
        if let Ok(number) = number.parse::<u32>() {
            numbers.push(number);
        }
    }
    Ok(numbers)
}

fn png_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let mut header = [0u8; 24];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| format!("Failed to read {path:?}: {error}"))?;

    const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if header[..8] != PNG_SIGNATURE || &header[12..16] != b"IHDR" {
        return Err(format!("{path:?} is not a PNG image"));
    }

    let width = u32::from_be_bytes(header[16..20].try_into().expect("width slice"));
    let height = u32::from_be_bytes(header[20..24].try_into().expect("height slice"));
    Ok((width, height))
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
        assert!(!pages.is_empty());
        assert_eq!(pages[0].page_index, 0);
        assert!(pages[0].width > 0);
        assert!(pages[0].height > 0);
        assert!(pages[0].image_path.exists());
    }

    #[test]
    fn test_multipage_pdf_rasterizes_all_pages() {
        let engine = TectonicEngine::new();
        let request = CompileRequest::simple(
            r#"
\documentclass{article}
\begin{document}
Page one.
\newpage
Page two.
\newpage
Page three.
\end{document}
"#,
            1,
        );
        let compile_output = engine.compile(request).expect("compile must succeed");

        let renderer = NativePdfRenderer::new();
        let pages = renderer
            .render_document(3, &compile_output.artifact)
            .expect("rasterization must succeed");

        let has_pdftoppm = Command::new("pdftoppm").arg("-v").output().is_ok();
        let expected = if has_pdftoppm { 3 } else { 1 };
        assert_eq!(pages.len(), expected);
        for (index, page) in pages.iter().enumerate() {
            assert_eq!(page.page_index, index);
            assert!(page.image_path.exists());
        }
    }

    #[test]
    fn test_png_dimensions_reads_ihdr() {
        let engine = TectonicEngine::new();
        let request = CompileRequest::simple(
            r#"\documentclass{article}\begin{document}Dimensions\end{document}"#,
            1,
        );
        let compile_output = engine.compile(request).expect("compile must succeed");

        let renderer = NativePdfRenderer::new();
        let pages = renderer
            .render_document(2, &compile_output.artifact)
            .expect("rasterization must succeed");

        let (width, height) = png_dimensions(&pages[0].image_path).expect("valid PNG");
        assert_eq!((width, height), (pages[0].width, pages[0].height));
    }

    #[test]
    fn test_png_dimensions_rejects_non_png() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("fake.png");
        std::fs::write(&path, b"definitely not a png").expect("write file");

        assert!(png_dimensions(&path).is_err());
    }
}
