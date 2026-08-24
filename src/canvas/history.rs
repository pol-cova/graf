use crate::canvas::scene::CanvasDocument;

const MAX_HISTORY_ENTRIES: usize = 50;

#[derive(Debug, Clone, Default)]
pub struct CanvasHistory {
    undo_stack: Vec<CanvasDocument>,
    redo_stack: Vec<CanvasDocument>,
}

impl CanvasHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn push_snapshot(&mut self, state: CanvasDocument) {
        if self.undo_stack.len() >= MAX_HISTORY_ENTRIES {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(state);
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, current: CanvasDocument) -> Option<CanvasDocument> {
        let prev = self.undo_stack.pop()?;
        self.redo_stack.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: CanvasDocument) -> Option<CanvasDocument> {
        let next = self.redo_stack.pop()?;
        self.undo_stack.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
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
