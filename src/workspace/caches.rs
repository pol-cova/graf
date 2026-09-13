use std::time::Duration;

use gpui::Context;

use super::{ActiveViewKind, Workspace};
use crate::project::document::DocumentId;
use crate::project::outline::{OutlineItem, parse_latex_outline};
use crate::project::stats::DocumentStats;

/// Docs larger than this are parsed off the UI thread. Smaller docs are
/// parsed inline but still cached by revision so render never re-parses.
pub const LARGE_DOC_THRESHOLD_BYTES: usize = 100 * 1024;

/// Debounce for stats recomputation after edits.
pub const STATS_DEBOUNCE: Duration = Duration::from_millis(500);

fn empty_stats() -> DocumentStats {
    DocumentStats {
        word_count: 0,
        char_count: 0,
        equation_count: 0,
        citation_count: 0,
        reading_time_mins: 0.1,
        estimated_pages: 0.1,
    }
}

pub fn should_offload_to_background(byte_len: usize) -> bool {
    byte_len > LARGE_DOC_THRESHOLD_BYTES
}

#[derive(Debug, Clone)]
pub(crate) struct OutlineCache {
    doc_id: Option<DocumentId>,
    revision: u64,
    items: Vec<OutlineItem>,
    initialized: bool,
    pending: Option<(Option<DocumentId>, u64)>,
}

impl OutlineCache {
    pub(crate) fn new() -> Self {
        Self {
            doc_id: None,
            revision: 0,
            items: Vec::new(),
            initialized: false,
            pending: None,
        }
    }

    pub(crate) fn items(&self) -> &[OutlineItem] {
        &self.items
    }

    pub(crate) fn is_fresh(&self, doc_id: Option<DocumentId>, revision: u64) -> bool {
        self.initialized && self.doc_id == doc_id && self.revision == revision
    }

    fn is_pending(&self, doc_id: Option<DocumentId>, revision: u64) -> bool {
        self.pending.is_some_and(|(pending_doc, pending_rev)| {
            pending_doc == doc_id && pending_rev == revision
        })
    }

    pub(crate) fn set(
        &mut self,
        doc_id: Option<DocumentId>,
        revision: u64,
        items: Vec<OutlineItem>,
    ) {
        self.doc_id = doc_id;
        self.revision = revision;
        self.items = items;
        self.initialized = true;
        if self.is_pending(doc_id, revision) {
            self.pending = None;
        }
    }

    pub(crate) fn clear(&mut self, doc_id: Option<DocumentId>, revision: u64) {
        self.set(doc_id, revision, Vec::new());
    }

    /// Synchronous cached refresh. Returns true when recomputed.
    /// Returns false when already fresh or when the caller should offload
    /// to the background executor.
    pub(crate) fn refresh_if_stale_inline(
        &mut self,
        doc_id: Option<DocumentId>,
        revision: u64,
        text: &str,
    ) -> bool {
        if self.is_fresh(doc_id, revision) {
            return false;
        }
        if should_offload_to_background(text.len()) {
            return false;
        }
        let items = parse_latex_outline(text);
        self.set(doc_id, revision, items);
        true
    }
}

impl Default for OutlineCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StatsCache {
    doc_id: Option<DocumentId>,
    revision: u64,
    is_typst: bool,
    stats: DocumentStats,
    initialized: bool,
    pending: Option<(Option<DocumentId>, u64, bool)>,
}

impl StatsCache {
    pub(crate) fn new() -> Self {
        Self {
            doc_id: None,
            revision: 0,
            is_typst: false,
            stats: empty_stats(),
            initialized: false,
            pending: None,
        }
    }

    pub(crate) fn stats(&self) -> &DocumentStats {
        &self.stats
    }

    pub(crate) fn is_fresh(
        &self,
        doc_id: Option<DocumentId>,
        revision: u64,
        is_typst: bool,
    ) -> bool {
        self.initialized
            && self.doc_id == doc_id
            && self.revision == revision
            && self.is_typst == is_typst
    }

    fn is_pending(&self, doc_id: Option<DocumentId>, revision: u64, is_typst: bool) -> bool {
        self.pending
            .is_some_and(|(pending_doc, pending_rev, pending_typst)| {
                pending_doc == doc_id && pending_rev == revision && pending_typst == is_typst
            })
    }

    pub(crate) fn set(
        &mut self,
        doc_id: Option<DocumentId>,
        revision: u64,
        is_typst: bool,
        stats: DocumentStats,
    ) {
        self.doc_id = doc_id;
        self.revision = revision;
        self.is_typst = is_typst;
        self.stats = stats;
        self.initialized = true;
        if self.is_pending(doc_id, revision, is_typst) {
            self.pending = None;
        }
    }

    pub(crate) fn clear(&mut self, doc_id: Option<DocumentId>, revision: u64, is_typst: bool) {
        self.set(doc_id, revision, is_typst, empty_stats());
    }

    /// Synchronous cached refresh for small docs. Returns true when recomputed.
    pub(crate) fn refresh_if_stale_inline(
        &mut self,
        doc_id: Option<DocumentId>,
        revision: u64,
        text: &str,
        is_typst: bool,
    ) -> bool {
        if self.is_fresh(doc_id, revision, is_typst) {
            return false;
        }
        if should_offload_to_background(text.len()) {
            return false;
        }
        let stats = DocumentStats::compute(text, is_typst);
        self.set(doc_id, revision, is_typst, stats);
        true
    }
}

impl Default for StatsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub(crate) fn cached_outline_items(&self) -> &[OutlineItem] {
        self.outline_cache.items()
    }

    pub(crate) fn cached_stats(&self) -> &DocumentStats {
        self.stats_cache.stats()
    }

    fn active_cache_doc_id(&self) -> Option<DocumentId> {
        self.documents.get(self.active_doc_idx).map(|doc| doc.id())
    }

    fn active_cache_is_typst(&self) -> bool {
        self.documents
            .get(self.active_doc_idx)
            .is_some_and(|doc| doc.title().ends_with(".typ"))
    }

    fn active_is_canvas_view(&self) -> bool {
        self.active_view_kind == ActiveViewKind::Canvas
    }

    /// Refresh outline cache after an edit. Small docs recompute inline;
    /// large docs recompute on the background executor while the last valid
    /// items stay visible. Stale background results are rejected by revision.
    pub(crate) fn refresh_outline_cache(&mut self, cx: &mut Context<Self>) {
        if self.active_is_canvas_view() {
            let doc_id = self.active_cache_doc_id();
            let revision = self.editor.read(cx).revision();
            self.outline_cache.clear(doc_id, revision);
            return;
        }
        let doc_id = self.active_cache_doc_id();
        let Some(expected_id) = doc_id else {
            self.outline_cache.clear(None, 0);
            return;
        };

        let revision = self.editor.read(cx).revision();
        if self.outline_cache.is_fresh(Some(expected_id), revision) {
            return;
        }
        if self.outline_cache.is_pending(Some(expected_id), revision) {
            return;
        }

        let text_len = self.editor.read(cx).text().len();
        if !should_offload_to_background(text_len) {
            let recomputed = {
                let editor = self.editor.read(cx);
                let text = editor.text();
                // Inline parse borrows `text`; no `to_string` copy.
                parse_latex_outline(text)
            };
            self.outline_cache
                .set(Some(expected_id), revision, recomputed);
            return;
        }

        let snapshot = self.editor.read(cx).text().to_string();
        self.outline_cache.pending = Some((Some(expected_id), revision));
        let task = cx.spawn(async move |this, cx| {
            let items = cx
                .background_executor()
                .spawn(async move { parse_latex_outline(&snapshot) })
                .await;
            this.update(cx, |this, cx| {
                let current_doc = this.active_cache_doc_id();
                let current_rev = this.editor.read(cx).revision();
                if current_doc != Some(expected_id) || current_rev != revision {
                    return;
                }
                if this.active_is_canvas_view() {
                    return;
                }
                this.outline_cache.set(Some(expected_id), revision, items);
                cx.notify();
            })
            .ok();
        });
        self.outline_task = Some(task);
    }

    /// Debounced background refresh for stats after edits. Keeps the last
    /// valid stats visible until the new value arrives.
    pub(crate) fn schedule_stats_refresh(&mut self, cx: &mut Context<Self>) {
        if self.active_is_canvas_view() {
            let doc_id = self.active_cache_doc_id();
            let revision = self.editor.read(cx).revision();
            self.stats_cache.clear(doc_id, revision, false);
            return;
        }
        let doc_id = self.active_cache_doc_id();
        let Some(expected_id) = doc_id else {
            self.stats_cache.clear(None, 0, false);
            return;
        };
        let revision = self.editor.read(cx).revision();
        let is_typst = self.active_cache_is_typst();
        if self
            .stats_cache
            .is_fresh(Some(expected_id), revision, is_typst)
        {
            return;
        }
        if self
            .stats_cache
            .is_pending(Some(expected_id), revision, is_typst)
        {
            return;
        }
        // Empty docs resolve inline so a fresh tab never flashes stale counts.
        if self.editor.read(cx).text().is_empty() {
            self.stats_cache
                .clear(Some(expected_id), revision, is_typst);
            return;
        }

        self.stats_cache.pending = Some((Some(expected_id), revision, is_typst));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STATS_DEBOUNCE).await;
            let snapshot: Option<(Option<DocumentId>, u64, bool, String)> = this
                .update(cx, |this, cx| {
                    if this.active_is_canvas_view() {
                        return None;
                    }
                    let current_doc = this.active_cache_doc_id();
                    let current_rev = this.editor.read(cx).revision();
                    let current_typst = this.active_cache_is_typst();
                    if this
                        .stats_cache
                        .is_fresh(current_doc, current_rev, current_typst)
                    {
                        return None;
                    }
                    let text = this.editor.read(cx).text().to_string();
                    Some((current_doc, current_rev, current_typst, text))
                })
                .ok()
                .flatten();
            let Some((snap_doc, snap_rev, snap_typst, text)) = snapshot else {
                return;
            };
            let computed = cx
                .background_executor()
                .spawn(async move { DocumentStats::compute(&text, snap_typst) })
                .await;
            this.update(cx, |this, cx| {
                if this.active_is_canvas_view() {
                    return;
                }
                let current_doc = this.active_cache_doc_id();
                let current_rev = this.editor.read(cx).revision();
                let current_typst = this.active_cache_is_typst();
                if current_doc != snap_doc || current_rev != snap_rev {
                    return;
                }
                if current_typst != snap_typst {
                    return;
                }
                this.stats_cache
                    .set(current_doc, current_rev, current_typst, computed);
                cx.notify();
            })
            .ok();
        });
        self.stats_task = Some(task);
    }

    /// Immediate refresh used on doc switch so the new tab shows correct
    /// values without waiting for the stats debounce. Small docs compute
    /// inline; large docs fall back to the debounced background path.
    pub(crate) fn refresh_caches_for_doc_switch(&mut self, cx: &mut Context<Self>) {
        if self.active_is_canvas_view() {
            let doc_id = self.active_cache_doc_id();
            let revision = self.editor.read(cx).revision();
            self.outline_cache.clear(doc_id, revision);
            self.stats_cache.clear(doc_id, revision, false);
            return;
        }
        let doc_id = self.active_cache_doc_id();
        let Some(expected_id) = doc_id else {
            self.outline_cache.clear(None, 0);
            self.stats_cache.clear(None, 0, false);
            return;
        };
        let revision = self.editor.read(cx).revision();
        let is_typst = self.active_cache_is_typst();

        if !self.outline_cache.is_fresh(Some(expected_id), revision)
            && !self.outline_cache.is_pending(Some(expected_id), revision)
        {
            let text_len = self.editor.read(cx).text().len();
            if !should_offload_to_background(text_len) {
                let items = {
                    let editor = self.editor.read(cx);
                    parse_latex_outline(editor.text())
                };
                self.outline_cache.set(Some(expected_id), revision, items);
            } else {
                self.refresh_outline_cache(cx);
            }
        }

        if self
            .stats_cache
            .is_fresh(Some(expected_id), revision, is_typst)
        {
            return;
        }
        let text_len = self.editor.read(cx).text().len();
        if !should_offload_to_background(text_len) {
            let stats = {
                let editor = self.editor.read(cx);
                DocumentStats::compute(editor.text(), is_typst)
            };
            self.stats_cache
                .set(Some(expected_id), revision, is_typst, stats);
        } else {
            self.schedule_stats_refresh(cx);
        }
    }

    /// Prime both caches from known content without touching the editor.
    /// Used once during startup where no background task exists yet.
    pub(crate) fn prime_caches_from_text(
        &mut self,
        doc_id: Option<DocumentId>,
        revision: u64,
        text: &str,
        is_typst: bool,
    ) {
        if !should_offload_to_background(text.len()) {
            self.outline_cache
                .refresh_if_stale_inline(doc_id, revision, text);
            self.stats_cache
                .refresh_if_stale_inline(doc_id, revision, text, is_typst);
        } else {
            // Large initial docs still get a synchronous outline so the first
            // frame is not empty; stats follow via the debounced path.
            let items = parse_latex_outline(text);
            self.outline_cache.set(doc_id, revision, items);
            let stats = DocumentStats::compute(text, is_typst);
            self.stats_cache.set(doc_id, revision, is_typst, stats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_cache_returns_false_for_uninitialized() {
        let cache = OutlineCache::new();
        assert!(!cache.is_fresh(Some(DocumentId(1)), 0));
        assert_eq!(cache.items().len(), 0);
    }

    #[test]
    fn outline_cache_recomputes_only_on_revision_change() {
        let mut cache = OutlineCache::new();
        let text = "\\section{Intro}\nbody\n";
        let doc = Some(DocumentId(1));

        assert!(cache.refresh_if_stale_inline(doc, 1, text));
        assert_eq!(cache.items().len(), 1);
        assert!(cache.is_fresh(doc, 1));

        // Same revision: no recompute.
        assert!(!cache.refresh_if_stale_inline(doc, 1, text));
        // New revision: recompute.
        assert!(cache.refresh_if_stale_inline(doc, 2, text));
    }

    #[test]
    fn outline_cache_invalidates_on_doc_switch() {
        let mut cache = OutlineCache::new();
        let text = "\\section{A}\n";
        cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, text);
        assert!(cache.is_fresh(Some(DocumentId(1)), 0));
        // Same revision but different doc must be stale.
        assert!(!cache.is_fresh(Some(DocumentId(2)), 0));
        assert!(cache.refresh_if_stale_inline(Some(DocumentId(2)), 0, text));
        assert!(cache.is_fresh(Some(DocumentId(2)), 0));
    }

    #[test]
    fn outline_cache_handles_empty_doc() {
        let mut cache = OutlineCache::new();
        assert!(cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, ""));
        assert!(cache.items().is_empty());
        assert!(cache.is_fresh(Some(DocumentId(1)), 0));
    }

    #[test]
    fn outline_cache_defers_large_docs_to_background() {
        let mut cache = OutlineCache::new();
        let large = "x".repeat(LARGE_DOC_THRESHOLD_BYTES + 1);
        assert!(!cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, &large));
        assert!(!cache.is_fresh(Some(DocumentId(1)), 0));
        assert!(should_offload_to_background(large.len()));
        assert!(!should_offload_to_background(10));
        assert!(!should_offload_to_background(LARGE_DOC_THRESHOLD_BYTES));
    }

    #[test]
    fn outline_clear_marks_canvas_empty_but_fresh() {
        let mut cache = OutlineCache::new();
        cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, "\\section{A}\n");
        cache.clear(Some(DocumentId(2)), 0);
        assert!(cache.items().is_empty());
        assert!(cache.is_fresh(Some(DocumentId(2)), 0));
    }

    #[test]
    fn stats_cache_tracks_typst_flag() {
        let mut cache = StatsCache::new();
        let text = "hello world";
        assert!(cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, text, false));
        assert!(cache.is_fresh(Some(DocumentId(1)), 0, false));
        // Language flip invalidates even with same revision.
        assert!(!cache.is_fresh(Some(DocumentId(1)), 0, true));
        assert!(cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, text, true));
        assert!(cache.is_fresh(Some(DocumentId(1)), 0, true));
        assert_eq!(cache.stats().word_count, 2);
    }

    #[test]
    fn stats_cache_handles_empty_doc() {
        let mut cache = StatsCache::new();
        assert!(cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, "", false));
        assert_eq!(cache.stats().word_count, 0);
        assert!(cache.is_fresh(Some(DocumentId(1)), 0, false));
    }

    #[test]
    fn stats_cache_defers_large_docs_to_background() {
        let mut cache = StatsCache::new();
        let large = "word ".repeat(LARGE_DOC_THRESHOLD_BYTES);
        assert!(!cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, &large, false));
        assert!(!cache.is_fresh(Some(DocumentId(1)), 0, false));
    }

    #[test]
    fn stats_clear_resets_counts() {
        let mut cache = StatsCache::new();
        cache.refresh_if_stale_inline(Some(DocumentId(1)), 0, "hello world", false);
        assert_eq!(cache.stats().word_count, 2);
        cache.clear(Some(DocumentId(1)), 1, false);
        assert_eq!(cache.stats().word_count, 0);
        assert!(cache.is_fresh(Some(DocumentId(1)), 1, false));
    }
}
