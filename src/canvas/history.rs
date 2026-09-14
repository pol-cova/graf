use std::collections::VecDeque;

use crate::canvas::scene::CanvasDocument;

const MAX_HISTORY_ENTRIES: usize = 50;

/// Serialized-size budget for undo/redo memory beyond the entry count. A
/// scene of a few dozen shapes costs a few KB; a few KB * 50 is harmless,
/// but thousands of elements would not be.
const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct CanvasHistory {
    undo_stack: VecDeque<CanvasDocument>,
    redo_stack: VecDeque<CanvasDocument>,
}

impl CanvasHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
        }
    }

    pub fn push_snapshot(&mut self, state: CanvasDocument) {
        self.undo_stack.push_back(state);
        self.redo_stack.clear();

        // Evict from the front until under budget, but never drop the last
        // remaining snapshot: even a single oversized scene must stay
        // undoable, otherwise undo silently becomes a no-op right after a
        // big edit.
        while self.undo_stack.len() > 1
            && (self.undo_stack.len() > MAX_HISTORY_ENTRIES
                || self.estimate_bytes() > MAX_HISTORY_BYTES)
        {
            if self.undo_stack.pop_front().is_none() {
                break;
            }
        }
    }

    /// Cheap size estimate: element count scaled by a measured average
    /// serialized cost per element (kept in sync with `push_snapshot`'s
    /// budget intent without serializing on every call).
    fn estimate_bytes(&self) -> usize {
        const ELEMENT_ESTIMATE_BYTES: usize = 260;
        self.undo_stack
            .iter()
            .map(|doc| doc.elements.len() * ELEMENT_ESTIMATE_BYTES)
            .sum()
    }

    pub fn undo(&mut self, current: CanvasDocument) -> Option<CanvasDocument> {
        let prev = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: CanvasDocument) -> Option<CanvasDocument> {
        let next = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Depth of the undo stack, for tests.
    #[cfg(test)]
    pub(crate) fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::CanvasElement;

    #[test]
    fn test_canvas_history_undo_redo() {
        let mut history = CanvasHistory::new();
        let doc0 = CanvasDocument::new();

        let mut doc1 = doc0.clone();
        doc1.add_element(CanvasElement::new_rectangle(
            "r1", 0.0, 0.0, 50.0, 50.0, 0.0,
        ));

        let mut doc2 = doc1.clone();
        doc2.add_element(CanvasElement::new_ellipse("e1", 60.0, 0.0, 40.0, 40.0));

        history.push_snapshot(doc0.clone());
        history.push_snapshot(doc1.clone());

        assert_eq!(history.undo_stack.len(), 2);
        assert!(history.can_undo());
        assert!(!history.can_redo());

        let undone1 = history.undo(doc2.clone()).unwrap();
        assert_eq!(undone1.elements.len(), 1);
        assert_eq!(undone1.elements[0].id, "r1");
        assert!(history.can_redo());

        let undone0 = history.undo(undone1.clone()).unwrap();
        assert_eq!(undone0.elements.len(), 0);

        let redone1 = history.redo(undone0).unwrap();
        assert_eq!(redone1.elements.len(), 1);
        assert_eq!(redone1.elements[0].id, "r1");
    }

    #[test]
    fn byte_budget_evicts_oldest_snapshots() {
        // One element costs ~ELEMENT_ESTIMATE_BYTES of the serialized-size
        // budget, so a stack of huge scenes must shed its oldest entries
        // even though the entry count stays well under MAX_HISTORY_ENTRIES.
        let big = || {
            let mut doc = CanvasDocument::new();
            const BIG_ELEMENT_COUNT: usize = 20_000;
            for i in 0..BIG_ELEMENT_COUNT {
                doc.add_element(CanvasElement::new_rectangle(
                    format!("r{i}"),
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                    0.0,
                ));
            }
            doc
        };

        let mut history = CanvasHistory::new();
        for _ in 0..3 {
            history.push_snapshot(big());
        }

        // 3 * 20_000 * 260 bytes far exceeds MAX_HISTORY_BYTES: only the
        // newest snapshots that fit the budget survive.
        assert!(history.undo_len() < 3, "oversized stack must be evicted");
        assert!(history.undo_len() > 0, "the newest snapshot must survive");
        assert!(history.can_undo());
    }

    #[test]
    fn push_snapshot_always_keeps_the_newest_entry() {
        // Even a single snapshot over budget must not be dropped outright:
        // eviction pops from the front, never discards the push.
        let mut doc = CanvasDocument::new();
        for i in 0..50_000 {
            doc.add_element(CanvasElement::new_text(
                format!("t{i}"),
                0.0,
                0.0,
                "filler",
                12.0,
            ));
        }
        let mut history = CanvasHistory::new();
        history.push_snapshot(doc);
        assert_eq!(history.undo_len(), 1);
        assert!(history.can_undo());
    }
}
