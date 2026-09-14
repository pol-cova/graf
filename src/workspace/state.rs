//! Pure-logic home for the workspace: the index and naming arithmetic runs
//! without GPUI so it can be unit-tested directly.

use std::collections::HashMap;

use crate::canvas::history::CanvasHistory;
use crate::project::document::DocumentId;

/// Undo/redo history for `.graf` documents, keyed by document identity.
///
/// One shared `Entity<CanvasView>` renders every canvas document, so the
/// history cannot simply live in the view: `load_from_json` on each tab
/// switch used to wipe it, making undo a one-way door after a round-trip.
/// The history instead follows the document: the workspace swaps a
/// document's history out of the view on deactivation and back in on
/// activation.
#[derive(Debug, Default)]
pub(crate) struct CanvasHistoryStore {
    owner: Option<DocumentId>,
    entries: HashMap<DocumentId, CanvasHistory>,
}

impl CanvasHistoryStore {
    /// Claims the history belonging to `id` as the new viewer of the shared
    /// canvas; returns the history (or a fresh one) to hand into the view.
    pub(crate) fn activate(&mut self, id: DocumentId) -> CanvasHistory {
        self.owner = Some(id);
        self.entries.remove(&id).unwrap_or_default()
    }

    /// Parks the view's current history under its active owner before the
    /// display switches to another document.
    pub(crate) fn settle(&mut self, history: CanvasHistory) {
        if let Some(owner) = self.owner.take() {
            self.entries.insert(owner, history);
        }
    }

    /// Re-associates the history the view currently holds with `id` without
    /// disturbing the view; used when a new canvas document adopts the
    /// currently displayed scene as its starting content.
    pub(crate) fn retitle(&mut self, id: DocumentId) {
        self.owner = Some(id);
    }

    /// Discards any stashed history for `id`; called when its tab closes.
    pub(crate) fn drop_document(&mut self, id: DocumentId) {
        self.entries.remove(&id);
        if self.owner == Some(id) {
            self.owner = None;
        }
    }

    /// Depth of the undo stack stashed for `id`, for tests.
    #[cfg(test)]
    pub(crate) fn undo_len(&self, id: DocumentId) -> usize {
        self.entries
            .get(&id)
            .map(|history| history.undo_len())
            .unwrap_or(0)
    }
}

/// Visible QuickOpen rows; results are capped so rendering stays cheap.
pub(crate) const QUICK_OPEN_LIMIT: usize = 50;

/// Index of the active tab after removing `removed_idx` from a list that had
/// `len_before` tabs. Mirrors `force_close_tab`.
pub(crate) fn active_index_after_close(
    active_idx: usize,
    removed_idx: usize,
    len_before: usize,
) -> usize {
    let len_after = len_before.saturating_sub(1);
    if removed_idx < active_idx {
        active_idx.saturating_sub(1)
    } else if active_idx >= len_after {
        // The removed tab was at/after the active one (or last): clamp onto
        // the surviving tail.
        len_after.saturating_sub(1)
    } else {
        active_idx
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DraftKind {
    Typst,
    Diagram,
}

impl DraftKind {
    fn basename(self) -> &'static str {
        match self {
            Self::Typst => "document",
            Self::Diagram => "diagram",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Typst => "typ",
            Self::Diagram => "graf",
        }
    }
}

pub(crate) fn next_draft_title(kind: DraftKind, existing: &[String]) -> String {
    let occupied = |name: &str| existing.iter().any(|title| title == name);
    let mut counter = 1;
    loop {
        let candidate = format!("{}-{counter}.{}", kind.basename(), kind.extension());
        if !occupied(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

/// Uniquifies a template's suggested file name among the open documents:
/// `paper.tex` stays `paper.tex` unless taken, then becomes `paper-1.tex`.
pub(crate) fn unique_title(file_name: &str, existing: &[String]) -> String {
    let occupied = |name: &str| existing.iter().any(|title| title == name);
    if !occupied(file_name) {
        return file_name.to_string();
    }
    let (stem, extension) = match file_name.rsplit_once('.') {
        Some((stem, extension)) => (stem, Some(extension)),
        None => (file_name, None),
    };
    let mut counter = 1;
    loop {
        let candidate = match extension {
            Some(extension) => format!("{stem}-{counter}.{extension}"),
            None => format!("{stem}-{counter}"),
        };
        if !occupied(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::CanvasDocument;
    #[test]
    fn history_follows_its_document_across_tab_round_trips() {
        let mut store = CanvasHistoryStore::default();
        let diagram = DocumentId(7);

        // Tab A: user creates; history survives the switch away and back.
        let mut activated = store.activate(diagram);
        activated.push_snapshot(CanvasDocument::new());
        store.settle(activated);

        let restored = store.activate(diagram);
        assert!(restored.can_undo(), "undo must survive a tab round-trip");
    }

    #[test]
    fn activating_a_never_edited_document_starts_fresh() {
        let mut store = CanvasHistoryStore::default();
        let history = store.activate(DocumentId(3));
        assert!(!history.can_undo());
    }

    #[test]
    fn closing_the_tab_discards_its_history() {
        let mut store = CanvasHistoryStore::default();
        let diagram = DocumentId(9);
        let mut activated = store.activate(diagram);
        activated.push_snapshot(CanvasDocument::new());
        store.settle(activated);

        store.drop_document(diagram);
        let fresh = store.activate(diagram);
        assert!(!fresh.can_undo());
    }

    #[test]
    fn settle_keeps_the_active_owner_and_retitles_change_it() {
        let mut store = CanvasHistoryStore::default();
        let first = DocumentId(1);
        let second = DocumentId(2);
        let mut trail = CanvasHistory::new();
        trail.push_snapshot(CanvasDocument::new());

        // A live canvas document settles its trail on deactivation...
        store.retitle(first);
        store.settle(trail);
        assert_eq!(store.undo_len(first), 1);

        // ...and a new canvas document adopting the displayed scene takes
        // the view's history with it, not `first`'s.
        store.retitle(second);
        store.settle(CanvasHistory::new());
        assert_eq!(store.undo_len(first), 1);
        assert_eq!(store.undo_len(second), 0);
    }
}

#[cfg(test)]
mod index_after_close_tests {
    use super::*;

    #[test]
    fn closing_before_the_active_tab_shifts_the_active_index() {
        assert_eq!(active_index_after_close(3, 0, 5), 2);
    }

    #[test]
    fn closing_after_the_active_tab_keeps_the_active_index() {
        assert_eq!(active_index_after_close(1, 3, 5), 1);
    }

    #[test]
    fn closing_the_last_tab_activates_the_new_last() {
        assert_eq!(active_index_after_close(4, 4, 5), 3);
    }

    #[test]
    fn closing_the_only_remaining_tab_clamps_to_zero() {
        assert_eq!(active_index_after_close(0, 0, 1), 0);
    }

    #[test]
    fn draft_titles_survive_tab_churn() {
        let mut titles: Vec<String> = vec!["document-1.typ".into()];
        let first = next_draft_title(DraftKind::Typst, &titles);
        assert_eq!(first, "document-2.typ");
        titles.push(first.clone());

        // The user closes "document-1" and "document-3" appeared meanwhile;
        // reuse of "document-1" is fine, but a fresh title must be free.
        titles.push("document-3.typ".into());
        titles.retain(|t| t != "document-1.typ");
        let next = next_draft_title(DraftKind::Typst, &titles);
        assert_ne!(next, "document-3.typ");
        assert!(!titles.contains(&next));
    }

    #[test]
    fn draft_titles_start_at_one_and_use_the_kind_extension() {
        assert_eq!(next_draft_title(DraftKind::Typst, &[]), "document-1.typ");
        assert!(next_draft_title(DraftKind::Diagram, &[]).ends_with(".graf"));
    }

    #[test]
    fn template_titles_pass_through_when_free() {
        assert_eq!(unique_title("paper.tex", &[]), "paper.tex");
        assert_eq!(unique_title("paper.tex", &["main.tex".into()]), "paper.tex");
    }

    #[test]
    fn template_titles_uniquify_taken_names() {
        assert_eq!(unique_title("main.tex", &["main.tex".into()]), "main-1.tex");
        assert_eq!(
            unique_title("main.tex", &["main.tex".into(), "main-1.tex".into()]),
            "main-2.tex"
        );
        assert_eq!(unique_title("notes", &["notes".into()]), "notes-1");
    }
}

/// Which persona the shared prompt editor serves at the moment; entering a
/// persona is explicit, so routing in `on_prompt_changed` keys off this
/// state instead of sniffing `active_modal` matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PromptTarget {
    #[default]
    Idle,
    Find,
    QuickOpen,
    Palette,
    TemplatePicker,
}

#[cfg(test)]
mod prompt_target_tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        assert_eq!(PromptTarget::default(), PromptTarget::Idle);
        assert_ne!(PromptTarget::Idle, PromptTarget::Find);
        assert_ne!(PromptTarget::QuickOpen, PromptTarget::Palette);
        assert_ne!(PromptTarget::Idle, PromptTarget::TemplatePicker);
    }
}
