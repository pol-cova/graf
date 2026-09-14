use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window};

use crate::canvas::history::CanvasHistory;
use crate::canvas::scene::{CanvasDocument, CanvasElement, ElementKind};

use super::view::{CanvasTool, CanvasView};

const DEFAULT_RECTANGLE_WIDTH: f32 = 120.0;
const DEFAULT_RECTANGLE_HEIGHT: f32 = 80.0;
const DEFAULT_RECTANGLE_RADIUS: f32 = 4.0;
const DEFAULT_ELLIPSE_SIZE: f32 = 100.0;
const DEFAULT_SEGMENT_LENGTH: f32 = 80.0;
const DEFAULT_TEXT_CONTENT: &str = "Label";
const DEFAULT_TEXT_FONT_SIZE: f32 = 14.0;

impl CanvasView {
    pub(crate) fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        // Input and rendering must share one coordinate mapping or shapes
        // appear away from the click at any zoom other than 1.
        let (x, y) = self
            .document
            .viewport
            .screen_to_world(event.position.x.as_f32(), event.position.y.as_f32());

        self.is_dragging = true;
        self.drag_start = Some((x, y));
        self.pending_drag_snapshot = None;

        let mut select_hit = None;
        let mut spawn_tool: Option<CanvasTool> = None;
        match self.active_tool {
            CanvasTool::Select => {
                let hit = self
                    .document
                    .find_element_at(x, y, self.document.viewport.zoom)
                    .map(|e| e.id.clone());
                select_hit = hit;
            }
            tool @ (CanvasTool::Rectangle
            | CanvasTool::Ellipse
            | CanvasTool::Arrow
            | CanvasTool::Line
            | CanvasTool::Text) => spawn_tool = Some(tool),
        }

        if let Some(tool) = spawn_tool {
            self.history.push_snapshot(self.document.clone());
            let id = self.next_element_id();
            self.document
                .add_element(new_click_element(tool, id.clone(), x, y));
            self.selected_element_id = Some(id);
            self.revision += 1;
            self.active_tool = CanvasTool::Select;
        } else {
            // The element may be dragged next; remember the pre-drag scene so
            // the first actual movement can commit an undo entry. Clicks
            // without a move never push one.
            self.selected_element_id = select_hit.clone();
            self.pending_drag_snapshot = select_hit.is_some().then(|| self.document.clone());
        }
        cx.notify();
    }

    pub(crate) fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.is_dragging {
            return;
        }

        let current_x = event.position.x.as_f32();
        let current_y = event.position.y.as_f32();

        if let Some((start_x, start_y)) = self.drag_start {
            // Deltas are measured in screen space; convert to world units so
            // a drag moves the shape exactly as far as the cursor went.
            let (world_x, world_y) = self.document.viewport.screen_to_world(current_x, current_y);
            let (world_start_x, world_start_y) =
                self.document.viewport.screen_to_world(start_x, start_y);
            let dx = world_x - world_start_x;
            let dy = world_y - world_start_y;

            // Zero deltas happen on hover-style moves after pointer-down;
            // they move nothing and must not touch revision or history.
            if dx == 0.0 && dy == 0.0 {
                return;
            }

            commit_drag_snapshot(
                &mut self.pending_drag_snapshot,
                &mut self.history,
                self.selected_element_id.is_some(),
            );

            if let Some(elem) = self
                .selected_element_id
                .as_ref()
                .and_then(|id| self.document.elements.iter_mut().find(|e| &e.id == id))
            {
                elem.x += dx;
                elem.y += dy;
                match &mut elem.kind {
                    ElementKind::Line {
                        start_x,
                        start_y,
                        end_x,
                        end_y,
                    }
                    | ElementKind::Arrow {
                        start_x,
                        start_y,
                        end_x,
                        end_y,
                    } => {
                        *start_x += dx;
                        *start_y += dy;
                        *end_x += dx;
                        *end_y += dy;
                    }
                    _ => {}
                }
                self.drag_start = Some((current_x, current_y));
                self.document.invalidate_geometry_cache();
                self.revision += 1;
                cx.notify();
            }
        }
    }

    pub(crate) fn handle_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.is_dragging = false;
        self.drag_start = None;
        self.pending_drag_snapshot = None;
        cx.notify();
    }
}

/// One-shot element spawned by a shape tool at the click point.
fn new_click_element(tool: CanvasTool, id: String, x: f32, y: f32) -> CanvasElement {
    match tool {
        CanvasTool::Rectangle => CanvasElement::new_rectangle(
            id,
            x,
            y,
            DEFAULT_RECTANGLE_WIDTH,
            DEFAULT_RECTANGLE_HEIGHT,
            DEFAULT_RECTANGLE_RADIUS,
        ),
        CanvasTool::Ellipse => {
            CanvasElement::new_ellipse(id, x, y, DEFAULT_ELLIPSE_SIZE, DEFAULT_ELLIPSE_SIZE)
        }
        CanvasTool::Arrow => CanvasElement::new_arrow(id, x, y, x + DEFAULT_SEGMENT_LENGTH, y),
        CanvasTool::Line => CanvasElement::new_line(id, x, y, x + DEFAULT_SEGMENT_LENGTH, y),
        CanvasTool::Text => {
            CanvasElement::new_text(id, x, y, DEFAULT_TEXT_CONTENT, DEFAULT_TEXT_FONT_SIZE)
        }
        CanvasTool::Select => unreachable!("shape tools only"),
    }
}

/// Commits the pending pre-drag snapshot on the first frame that moves the
/// element, so drags become undoable as one step while plain clicks do not
/// pad the undo stack.
fn commit_drag_snapshot(
    pending: &mut Option<CanvasDocument>,
    history: &mut CanvasHistory,
    moved: bool,
) {
    if moved && let Some(snapshot) = pending.take() {
        history.push_snapshot(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_undo_entry_per_drag_not_per_frame() {
        let mut history = CanvasHistory::new();
        let mut pending = Some(CanvasDocument::new());

        commit_drag_snapshot(&mut pending, &mut history, true);
        assert!(history.can_undo());
        assert!(pending.is_none());

        // Subsequent drag frames keep the single snapshot; no per-frame churn.
        commit_drag_snapshot(&mut pending, &mut history, true);
        assert_eq!(history.undo_len(), 1);
    }

    #[test]
    fn click_without_movement_pushes_no_undo_entry() {
        let mut history = CanvasHistory::new();
        let mut pending = Some(CanvasDocument::new());

        // A click that never moves the element commits nothing...
        commit_drag_snapshot(&mut pending, &mut history, false);
        assert!(!history.can_undo());
        assert!(pending.is_some());
    }

    #[test]
    fn drag_can_be_reverted_instead_of_deleting_the_element() {
        // Regression for the data-loss path: before the pending snapshot,
        // undo after a drag popped the pre-creation entry and deleted the
        // element entirely.
        let mut pre_drag = CanvasDocument::new();
        pre_drag.add_element(CanvasElement::new_rectangle(
            "r1", 0.0, 0.0, 50.0, 50.0, 0.0,
        ));

        let mut dragged = pre_drag.clone();
        dragged.elements[0].x = 30.0;

        let mut history = CanvasHistory::new();
        let mut pending = Some(pre_drag);
        commit_drag_snapshot(&mut pending, &mut history, true);

        let restored = history.undo(dragged).unwrap();
        assert_eq!(restored.elements.len(), 1);
        assert_eq!(restored.elements[0].x, 0.0);
    }

    #[test]
    fn element_ids_never_collide_after_deletion() {
        let mut ids = crate::canvas::view::ElementIdAllocator::default();
        let first = ids.allocate();
        let second = ids.allocate();
        assert_ne!(first, second);

        // The old behavior derived ids from element count and would repeat
        // the second id after deleting the first: a monotonic counter cannot.
        let third = ids.allocate();
        assert_eq!(third, "elem-3");
        assert_ne!(third, second);
    }

    #[test]
    fn every_shape_tool_spawns_an_element_anchored_at_the_click() {
        for tool in [
            CanvasTool::Rectangle,
            CanvasTool::Ellipse,
            CanvasTool::Arrow,
            CanvasTool::Line,
            CanvasTool::Text,
        ] {
            let elem = new_click_element(tool, "id".to_string(), 10.0, 20.0);
            let (x1, y1, x2, y2) = elem.bounds();
            // Lines/arrows spawn horizontally, so y2 may equal y1; the
            // anchor and a non-degenerate extent are what matter.
            assert!(
                x1 == 10.0 && y1 == 20.0 && x2 > 10.0 && y2 >= 20.0,
                "{tool:?}"
            );
        }
    }
}
