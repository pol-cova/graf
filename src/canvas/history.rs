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

        while self.undo_stack.len() > MAX_HISTORY_ENTRIES
            || self.estimate_bytes() > MAX_HISTORY_BYTES
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
}
