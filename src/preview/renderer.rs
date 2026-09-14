use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::compiler::engine::run_with_cancel;
use crate::util::prune_numbered_dirs;

const PREVIEW_RASTER_WIDTH: &str = "1224";
const PAGE_PREFIX: &str = "page";

/// The rasterizer binary name, shared by the spawn, the probe, and the
/// failure labels so the three never drift to different binaries.
const PDFTOPPM: &str = "pdftoppm";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub page_index: usize,
    pub width: u32,
    pub height: u32,
    pub image_path: PathBuf,
}

/// What one successful render produced. The user-facing notice travels with
/// the result itself, so the renderer holds no cross-render state: two
/// overlapping renders cannot make notice state ambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOutcome {
    pub pages: Vec<RenderedPage>,
    /// Degraded-render note (e.g. the `sips` single-page fallback), or None
    /// when the full pipeline produced these pages.
    pub notice: Option<String>,
}

pub trait PdfRenderer: Send + Sync {
    fn render_document(
        &self,
        render_id: u64,
        pdf_bytes: &[u8],
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Result<RenderOutcome, String>;
}

const RENDER_RUNS_TO_KEEP: usize = 2;
const PRUNE_MIN_IDLE: Duration = Duration::from_secs(60);

/// How many distinct PDFs stay rasterized between compiles. One entry costs a
/// page-images directory on disk; 4 covers the docs a user bounces between.
const RASTER_CACHE_CAPACITY: usize = 4;

type RasterCache = std::collections::VecDeque<(u64, Arc<Vec<RenderedPage>>, bool)>;

pub struct NativePdfRenderer {
    cache_dir: crate::util::TemporarySessionDir,
    /// (content hash, pages, degraded-to-sips flag) ordered least- to
    /// most-recently used. Identical PDF bytes skip re-rasterization
    /// entirely; the flag keeps the fallback notice honest for hits, since
    /// both the full pipeline and the degraded sips path cache here.
    raster_cache: std::sync::Mutex<RasterCache>,
}

impl Default for NativePdfRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl NativePdfRenderer {
    pub fn new() -> Self {
        Self {
            cache_dir: crate::util::TemporarySessionDir::new("graf_pdf"),
            raster_cache: std::sync::Mutex::new(RasterCache::new()),
        }
    }

    fn cache_hit(&self, hash: u64) -> Option<(Arc<Vec<RenderedPage>>, bool)> {
        let mut cache = self
            .raster_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let position = cache.iter().position(|(key, _, _)| *key == hash)?;
        cache.rotate_left(position);
        let entry = cache.front()?.clone();
        // A pruned or reclaimed directory would leave the preview pointing
        // at missing images; re-rasterize in that case.
        if entry.1.first().is_some_and(|page| page.image_path.exists()) {
            Some((entry.1, entry.2))
        } else {
            None
        }
    }

    fn cache_insert(&self, hash: u64, pages: Arc<Vec<RenderedPage>>, degraded: bool) {
        let mut cache = self
            .raster_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(position) = cache.iter().position(|(key, _, _)| *key == hash) {
            cache.remove(position);
        }
        cache.push_front((hash, pages, degraded));
        while cache.len() > RASTER_CACHE_CAPACITY {
            cache.pop_back();
        }
    }
}

/// The note shown when pages came from the degraded one-page `sips` path.
const FALLBACK_NOTICE: &str = "Install poppler (pdftoppm) for a multipage preview. \
     Falling back to one page via sips.";

fn hash_pdf_bytes(pdf_bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    pdf_bytes.len().hash(&mut hasher);
    pdf_bytes.hash(&mut hasher);
    hasher.finish()
}

/// Decides the failure message produced by one rasterization subprocess:
/// success means `None`, a cancelled or failed run means a message, and —
/// unlike a bare `.ok()?` — a spawn error (missing binary, exec failure)
/// must also be a message rather than a silent pass-through.
fn rasterization_failure(
    result: std::io::Result<
        Result<std::process::Output, crate::compiler::engine::CompileCancelled>,
    >,
    tool: &'static str,
) -> Option<String> {
    match result {
        Err(error) => Some(format!("{tool} could not start: {error}")),
        Ok(Err(_)) => Some(format!("{tool} rasterization cancelled")),
        Ok(Ok(output)) => {
            if output.status.success() {
                None
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Some(format!("{tool} rasterization failed: {stderr}"))
            }
        }
    }
}

impl NativePdfRenderer {
    fn rasterize_with_pdftoppm(
        &self,
        pdf_file: &Path,
        run_dir: &Path,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Option<String> {
        let output_root = run_dir.join(PAGE_PREFIX);
        let mut command = Command::new(PDFTOPPM);
        command
            .arg("-png")
            .arg("-scale-to-x")
            .arg(PREVIEW_RASTER_WIDTH)
            .arg("-scale-to-y")
            .arg("-1")
            .arg(pdf_file)
            .arg(&output_root);
        rasterization_failure(run_with_cancel(command, cancel), PDFTOPPM)
    }

    #[cfg(target_os = "macos")]
    fn rasterize_with_sips(
        &self,
        pdf_file: &Path,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Option<String> {
        let png_file = pdf_file.parent()?.join(format!("{PAGE_PREFIX}-1.png"));
        let mut command = Command::new("/usr/bin/sips");
        command
            .arg("-s")
            .arg("format")
            .arg("png")
            .arg("--resampleWidth")
            .arg(PREVIEW_RASTER_WIDTH)
            .arg(pdf_file)
            .arg("--out")
            .arg(&png_file);
        rasterization_failure(run_with_cancel(command, cancel), "sips")
    }

    #[cfg(not(target_os = "macos"))]
    fn rasterize_with_sips(
        &self,
        _pdf_file: &Path,
        _cancel: Option<&Arc<AtomicBool>>,
    ) -> Option<String> {
        None
    }
}

impl PdfRenderer for NativePdfRenderer {
    fn render_document(
        &self,
        render_id: u64,
        pdf_bytes: &[u8],
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Result<RenderOutcome, String> {
        if pdf_bytes.is_empty() || !pdf_bytes.starts_with(b"%PDF-") {
            return Err("Invalid or empty PDF data".to_string());
        }

        let hash = hash_pdf_bytes(pdf_bytes);
        if let Some((pages, degraded)) = self.cache_hit(hash) {
            // The cache must not drop a notice by assumption: sips-path
            // results are cached too, so the notice that matches these
            // exact pages travels with them.
            return Ok(RenderOutcome {
                pages: (*pages).clone(),
                notice: degraded.then(|| FALLBACK_NOTICE.to_string()),
            });
        }

        // Each render gets a fresh directory, so the cache would grow by a
        // full PDF and page images on every compile. Age-guarded pruning
        // leaves in-flight renders and cache-referenced directories (within
        // the keep window) alone.
        prune_numbered_dirs(
            self.cache_dir.path(),
            "render_",
            RENDER_RUNS_TO_KEEP.max(RASTER_CACHE_CAPACITY),
            PRUNE_MIN_IDLE,
        );

        let run_dir = self.cache_dir.path().join(format!("render_{render_id}"));
        fs::create_dir_all(&run_dir)
            .map_err(|error| format!("Failed to create preview directory: {error}"))?;

        let pdf_file = run_dir.join("document.pdf");
        fs::write(&pdf_file, pdf_bytes).map_err(|error| format!("Failed to write PDF: {error}"))?;

        let failure = if pdftoppm_available() {
            self.rasterize_with_pdftoppm(&pdf_file, &run_dir, cancel)
        } else {
            self.rasterize_with_sips(&pdf_file, cancel)
        };

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

        // Only the degraded path produced a notice for these pages; the
        // full pipeline's notice is None by construction.
        let degraded = !pdftoppm_available();
        self.cache_insert(hash, Arc::new(pages.clone()), degraded);
        Ok(RenderOutcome {
            pages,
            notice: degraded.then(|| FALLBACK_NOTICE.to_string()),
        })
    }
}

fn pdftoppm_available() -> bool {
    // Command::output reports Ok even for a nonzero exit, which is enough:
    // only a missing binary results in Err. Cached with a TTL so poppler can
    // show up mid-session without paying a process spawn per compile.
    static PROBE: std::sync::Mutex<Option<(std::time::Instant, bool)>> =
        std::sync::Mutex::new(None);
    const PROBE_TTL_SECONDS: u64 = 30;

    let mut cached = PROBE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((checked_at, available)) = cached.as_ref()
        && checked_at.elapsed().as_secs() < PROBE_TTL_SECONDS
    {
        return *available;
    }
    let available = Command::new(PDFTOPPM)
        .arg("-v")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .is_ok();
    *cached = Some((std::time::Instant::now(), available));
    available
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
        let result = renderer.render_document(1, b"not a pdf", None);
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
        let result = renderer.render_document(1, &compile_output.artifact, None);
        assert!(result.is_ok(), "Rasterization failed: {:?}", result.err());

        let outcome = result.unwrap();
        let pages = outcome.pages;
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
            .render_document(3, &compile_output.artifact, None)
            .expect("rasterization must succeed")
            .pages;

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
            .render_document(2, &compile_output.artifact, None)
            .expect("rasterization must succeed")
            .pages;

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

    #[test]
    fn identical_pdf_is_rasterized_once() {
        let engine = TectonicEngine::new();
        let request = CompileRequest::simple(
            r#"\documentclass{article}\begin{document}Raster Cache\end{document}"#,
            1,
        );
        let compile_output = engine.compile(request).expect("compile must succeed");
        let bytes = compile_output.artifact;

        let renderer = NativePdfRenderer::new();
        let first = renderer
            .render_document(1, &bytes, None)
            .expect("first render")
            .pages;
        // Different render id, same bytes: must hit the cache and return the
        // same page images.
        let second = renderer
            .render_document(2, &bytes, None)
            .expect("cached render")
            .pages;
        assert_eq!(first, second);
    }

    #[test]
    fn cache_evicts_beyond_capacity() {
        let engine = TectonicEngine::new();
        let request = CompileRequest::simple(
            r#"\documentclass{article}\begin{document}Evict\end{document}"#,
            1,
        );
        let compile_output = engine.compile(request).expect("compile must succeed");
        let bytes = compile_output.artifact;

        let renderer = NativePdfRenderer::new();
        renderer.render_document(1, &bytes, None).ok();
        // Push RASTER_CACHE_CAPACITY + 1 distinct PDFs through; the original
        // entry must fall out. Padding byte suffixes keep hashes distinct...
        // but PDF validity requires the header only, so vary the tail.
        for id in 0..RASTER_CACHE_CAPACITY + 1 {
            let mut variant = Vec::with_capacity(bytes.len() + 1);
            variant.extend_from_slice(&bytes);
            variant.push(id as u8);
            renderer
                .render_document(100 + id as u64, &variant, None)
                .expect("render variant");
        }

        let mut cache = renderer
            .raster_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(cache.make_contiguous().len(), RASTER_CACHE_CAPACITY);
    }

    #[test]
    fn cache_hit_validates_images_exist() {
        let engine = TectonicEngine::new();
        let request = CompileRequest::simple(
            r#"\documentclass{article}\begin{document}Missing Files\end{document}"#,
            1,
        );
        let compile_output = engine.compile(request).expect("compile must succeed");
        let bytes = compile_output.artifact;

        let renderer = NativePdfRenderer::new();
        renderer.render_document(1, &bytes, None).expect("render");
        // Simulate the cache dir being reclaimed under us.
        let run = renderer.cache_dir.path().join("render_1");
        let _ = std::fs::remove_dir_all(&run);
        let result = renderer.render_document(2, &bytes, None);
        assert!(result.is_ok(), "re-render after eviction must succeed");
    }

    #[test]
    fn hash_distinguishes_prefix_lengths() {
        assert_ne!(hash_pdf_bytes(b"%PDF-x"), hash_pdf_bytes(b"%PDF-"));
        assert_eq!(hash_pdf_bytes(b"same"), hash_pdf_bytes(b"same"));
    }

    fn sample_pages(path: &Path) -> Vec<RenderedPage> {
        vec![RenderedPage {
            page_index: 0,
            width: 10,
            height: 10,
            image_path: path.join("page-1.png"),
        }]
    }

    #[test]
    fn degraded_cache_hit_keeps_the_fallback_notice() {
        // Regression: sips-fallback pages land in the raster cache like any
        // other render; a later hit used to clear the notice as if these
        // pages came from the full pipeline.
        let directory = tempfile::tempdir().expect("tempdir");
        std::fs::write(directory.path().join("page-1.png"), b"png").expect("page image");
        let renderer = NativePdfRenderer::new();
        let bytes = b"%PDF-cache-hit";
        renderer.cache_insert(
            hash_pdf_bytes(bytes),
            Arc::new(sample_pages(directory.path())),
            true,
        );

        let outcome = renderer.render_document(1, bytes, None).expect("hit");
        assert!(outcome.notice.is_some());
    }

    #[test]
    fn full_pipeline_cache_hit_clears_the_fallback_notice() {
        let directory = tempfile::tempdir().expect("tempdir");
        std::fs::write(directory.path().join("page-1.png"), b"png").expect("page image");
        let renderer = NativePdfRenderer::new();
        let bytes = b"%PDF-cache-hit full";
        renderer.cache_insert(
            hash_pdf_bytes(bytes),
            Arc::new(sample_pages(directory.path())),
            false,
        );

        let outcome = renderer.render_document(1, bytes, None).expect("hit");
        assert!(outcome.notice.is_none());
    }

    #[test]
    fn spawn_errors_are_failures_not_silent_success() {
        // A missing binary (Err from run_with_cancel) used to satisfy the
        // old `.ok()?` and continued as if nothing failed.
        let spawn_error = std::io::Result::Err(std::io::Error::other("not found"));
        let message = rasterization_failure(spawn_error, "pdftoppm").expect("failure");
        assert!(message.contains("could not start"), "{message}");
    }

    #[test]
    fn rasterization_failure_classifies_subprocess_results() {
        use crate::compiler::engine::run_with_cancel;

        // Cancelled run → message without blame on the tool's stderr.
        let cancel = Arc::new(AtomicBool::new(true));
        let command = Command::new("echo");
        let cancelled =
            rasterization_failure(run_with_cancel(command, Some(&cancel)), "pdftoppm").unwrap();
        assert!(cancelled.contains("cancelled"), "{cancelled}");

        // Successful run → no failure.
        let command = Command::new("echo");
        assert!(rasterization_failure(run_with_cancel(command, None), "pdftoppm").is_none(),);

        // Failed run → error text included.
        let mut command = Command::new("sh");
        command.arg("-c").arg("echo oops >&2; exit 3");
        let failed =
            rasterization_failure(run_with_cancel(command, None), "sips").expect("failure");
        assert!(failed.contains("oops"), "{failed}");
    }
}
