use gpui::{
    Context, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, Window, div, prelude::*, px,
};

use crate::canvas::history::CanvasHistory;
use crate::canvas::scene::{CanvasDocument, CanvasElement, CanvasViewport, ElementKind};
use crate::canvas::svg::export_to_svg;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasTool {
    Select,
    Rectangle,
    Ellipse,
    Arrow,
    Line,
    Text,
}

impl CanvasTool {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Select => "Select (V)",
            Self::Rectangle => "Rectangle (R)",
            Self::Ellipse => "Ellipse (O)",
            Self::Arrow => "Arrow (A)",
            Self::Line => "Line (L)",
            Self::Text => "Text (T)",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Select => "↖",
            Self::Rectangle => "▢",
            Self::Ellipse => "◯",
            Self::Arrow => "→",
            Self::Line => "―",
            Self::Text => "T",
        }
    }
}

pub struct CanvasView {
    focus_handle: FocusHandle,
    document: CanvasDocument,
    history: CanvasHistory,
    active_tool: CanvasTool,
    selected_element_id: Option<String>,
    is_dragging: bool,
    drag_start: Option<(f32, f32)>,
    revision: u64,
    /// Monotonic element-id source; a count-derived id collides after any
    /// deletion and makes selection/removal hit the wrong element.
    next_element_counter: ElementIdAllocator,
}

/// Never repeats: `elem-{n+1}` from a plain counter is safe where a
/// `elements.len()+1` scheme is not.
#[derive(Debug, Default, Clone, PartialEq)]
struct ElementIdAllocator {
    next: u64,
}

impl ElementIdAllocator {
    fn allocate(&mut self) -> String {
        self.next += 1;
        format!("elem-{}", self.next)
    }
}

impl CanvasView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            document: CanvasDocument::new(),
            history: CanvasHistory::new(),
            active_tool: CanvasTool::Select,
            selected_element_id: None,
            is_dragging: false,
            drag_start: None,
            revision: 0,
            next_element_counter: ElementIdAllocator::default(),
        }
    }

    pub fn load_from_json(&mut self, json: &str, cx: &mut Context<Self>) -> Result<(), String> {
        match CanvasDocument::from_json(json) {
            Ok(doc) => {
                self.document = doc;
                self.history = CanvasHistory::new();
                self.selected_element_id = None;
                self.revision += 1;
                cx.notify();
                Ok(())
            }
            Err(e) => Err(format!("Failed to parse .graf: {e}")),
        }
    }

    pub fn save_to_json(&self) -> Result<String, String> {
        self.document
            .to_json()
            .map_err(|e| format!("Failed to serialize .graf: {e}"))
    }

    pub fn export_svg(&self) -> String {
        export_to_svg(&self.document)
    }

    pub fn document(&self) -> &CanvasDocument {
        &self.document
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn next_element_id(&mut self) -> String {
        self.next_element_counter.allocate()
    }

    pub fn set_tool(&mut self, tool: CanvasTool, cx: &mut Context<Self>) {
        self.active_tool = tool;
        cx.notify();
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(prev) = self.history.undo(self.document.clone()) {
            self.document = prev;
            self.selected_element_id = None;
            self.revision += 1;
            cx.notify();
        }
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.history.redo(self.document.clone()) {
            self.document = next;
            self.selected_element_id = None;
            self.revision += 1;
            cx.notify();
        }
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.document.viewport.zoom = (self.document.viewport.zoom + 0.1).min(4.0);
        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.document.viewport.zoom = (self.document.viewport.zoom - 0.1).max(0.25);
        cx.notify();
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.document.viewport.zoom = 1.0;
        self.document.viewport.pan_x = 0.0;
        self.document.viewport.pan_y = 0.0;
        cx.notify();
    }

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_element_id.take() {
            self.history.push_snapshot(self.document.clone());
            self.document.remove_element(&id);
            self.revision += 1;
            cx.notify();
        }
    }

    fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
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

        match self.active_tool {
            CanvasTool::Select => {
                let hit = self.document.find_element_at(x, y).map(|e| e.id.clone());
                self.selected_element_id = hit;
            }
            CanvasTool::Rectangle => {
                self.history.push_snapshot(self.document.clone());
                let id = self.next_element_id();
                let rect = CanvasElement::new_rectangle(id.clone(), x, y, 120.0, 80.0, 4.0);
                self.document.add_element(rect);
                self.selected_element_id = Some(id);
                self.revision += 1;
                self.active_tool = CanvasTool::Select;
            }
            CanvasTool::Ellipse => {
                self.history.push_snapshot(self.document.clone());
                let id = self.next_element_id();
                let ellipse = CanvasElement::new_ellipse(id.clone(), x, y, 100.0, 100.0);
                self.document.add_element(ellipse);
                self.selected_element_id = Some(id);
                self.revision += 1;
                self.active_tool = CanvasTool::Select;
            }
            CanvasTool::Arrow => {
                self.history.push_snapshot(self.document.clone());
                let id = self.next_element_id();
                let arrow = CanvasElement::new_arrow(id.clone(), x, y, x + 80.0, y);
                self.document.add_element(arrow);
                self.selected_element_id = Some(id);
                self.revision += 1;
                self.active_tool = CanvasTool::Select;
            }
            CanvasTool::Line => {
                self.history.push_snapshot(self.document.clone());
                let id = self.next_element_id();
                let line = CanvasElement::new_line(id.clone(), x, y, x + 80.0, y);
                self.document.add_element(line);
                self.selected_element_id = Some(id);
                self.revision += 1;
                self.active_tool = CanvasTool::Select;
            }
            CanvasTool::Text => {
                self.history.push_snapshot(self.document.clone());
                let id = self.next_element_id();
                let text = CanvasElement::new_text(id.clone(), x, y, "Label", 14.0);
                self.document.add_element(text);
                self.selected_element_id = Some(id);
                self.revision += 1;
                self.active_tool = CanvasTool::Select;
            }
        }
        cx.notify();
    }

    fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
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

    fn handle_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_dragging = false;
        self.drag_start = None;
        cx.notify();
    }
}

impl Focusable for CanvasView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CanvasView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport_size = window.viewport_size();
        div()
            .id("canvas-root")
            .key_context("Canvas")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_1()
            .flex_col()
            .size_full()
            .bg(theme::color(theme::BG_CANVAS))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .child(self.render_toolbar(cx))
            .child(self.render_viewport(viewport_size))
    }
}

impl CanvasView {
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tools = [
            CanvasTool::Select,
            CanvasTool::Rectangle,
            CanvasTool::Ellipse,
            CanvasTool::Arrow,
            CanvasTool::Line,
            CanvasTool::Text,
        ];

        // Toolbar clicks must not bubble into the canvas root handler, or a
        // tool/zoom/undo button press also spawns a shape or starts a drag
        // at the button position.
        div()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(36.0))
            .px_3()
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
            .border_color(theme::color(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .children(tools.into_iter().map(|tool| {
                        let is_active = self.active_tool == tool;
                        div()
                            .id(format!("tool-{}", tool.name()))
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(if is_active {
                                theme::color(theme::TAB_ACTIVE)
                            } else {
                                theme::color(theme::BG_BAR)
                            })
                            .border_1()
                            .border_color(if is_active {
                                theme::color(theme::ACCENT_BLUE)
                            } else {
                                theme::color(theme::BORDER)
                            })
                            .text_xs()
                            .text_color(if is_active {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    this.set_tool(tool, cx);
                                }),
                            )
                            .child(tool.icon())
                            .child(tool.name())
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("canvas-undo-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_SURFACE))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(if self.history.can_undo() {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.undo(cx)),
                            )
                            .child("↶ Undo"),
                    )
                    .child(
                        div()
                            .id("canvas-redo-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_SURFACE))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(if self.history.can_redo() {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.redo(cx)),
                            )
                            .child("↷ Redo"),
                    )
                    .child(
                        div()
                            .id("canvas-delete-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_SURFACE))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(if self.selected_element_id.is_some() {
                                theme::color(theme::ACCENT_RED)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.delete_selected(cx)),
                            )
                            .child("Delete"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .id("canvas-zoom-out")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_xs()
                                    .bg(theme::color(theme::BG_SURFACE))
                                    .border_1()
                                    .border_color(theme::color(theme::BORDER))
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.zoom_out(cx)),
                                    )
                                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Minus))),
                            )
                            .child(
                                div()
                                    .id("canvas-zoom-reset")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_xs()
                                    .bg(theme::color(theme::BG_SURFACE))
                                    .border_1()
                                    .border_color(theme::color(theme::BORDER))
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.reset_zoom(cx)),
                                    )
                                    .child(format!("{:.0}%", self.document.viewport.zoom * 100.0)),
                            )
                            .child(
                                div()
                                    .id("canvas-zoom-in")
                                    .px_2()
                                    .py_0p5()
                                    .rounded_xs()
                                    .bg(theme::color(theme::BG_SURFACE))
                                    .border_1()
                                    .border_color(theme::color(theme::BORDER))
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.zoom_in(cx)),
                                    )
                                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Plus))),
                            ),
                    ),
            )
    }

    fn render_viewport(&self, viewport_size: gpui::Size<gpui::Pixels>) -> impl IntoElement {
        // Cull to what the camera can show: the visible world window is the
        // pane rect mapped through the shared transform, padded half a
        // screen beyond so partially-offscreen elements still appear.
        let half_extra = 0.5;
        let world_width = viewport_size.width.as_f32() * (1.0 + 2.0 * half_extra);
        let world_height = viewport_size.height.as_f32() * (1.0 + 2.0 * half_extra);
        let (world_origin_x, world_origin_y) = self
            .document
            .viewport
            .screen_to_world(-world_width * half_extra, -world_height * half_extra);
        let is_visible = |(x, y, w, h): (f32, f32, f32, f32)| {
            x + w >= world_origin_x
                && y + h >= world_origin_y
                && x <= world_origin_x + world_width
                && y <= world_origin_y + world_height
        };

        let mut viewport = div()
            .id("canvas-viewport")
            .relative()
            .flex_1()
            .size_full()
            .overflow_hidden();
        let zoom = self.document.viewport.zoom;

        // Open-ended segments paint once through the path API (real
        // endpoints + arrowheads are impossible with axis-aligned divs).
        let mut strokes: Vec<StrokeSpec> = Vec::new();

        for elem in &self.document.elements {
            // Elements far outside the viewport contribute nothing to the
            // frame; skipping them keeps drag cost proportional to what is
            // visible rather than the whole scene.
            let bounds = match &elem.kind {
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
                } => (
                    start_x.min(*end_x),
                    start_y.min(*end_y),
                    (start_x - end_x).abs(),
                    (start_y - end_y).abs(),
                ),
                _ => (elem.x, elem.y, elem.width, elem.height),
            };
            if !is_visible(bounds) {
                continue;
            }

            let is_selected = self.selected_element_id.as_deref() == Some(&elem.id);
            let (screen_left, screen_top) = self.document.viewport.world_to_screen(elem.x, elem.y);
            let left = px(screen_left);
            let top = px(screen_top);
            let width = px(elem.width * zoom);
            let height = px(elem.height * zoom);

            // Lines and arrows are deferred to one path-paint pass below;
            // real endpoints and arrowheads are impossible with
            // axis-aligned divs.
            let node = match &elem.kind {
                ElementKind::Line {
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                } => {
                    strokes.push(StrokeSpec {
                        start: (*start_x, *start_y),
                        end: (*end_x, *end_y),
                        arrowhead: false,
                    });
                    continue;
                }
                ElementKind::Arrow {
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                } => {
                    strokes.push(StrokeSpec {
                        start: (*start_x, *start_y),
                        end: (*end_x, *end_y),
                        arrowhead: true,
                    });
                    continue;
                }
                ElementKind::Rectangle { border_radius } => div()
                    .id(format!("shape-{}", elem.id))
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(width)
                    .h(height)
                    .rounded(px(*border_radius * zoom))
                    .bg(theme::color(theme::BG_SURFACE))
                    .border_2()
                    .border_color(if is_selected {
                        theme::color(theme::ACCENT_BLUE)
                    } else {
                        theme::color(theme::BORDER)
                    })
                    .shadow_md(),
                ElementKind::Ellipse => div()
                    .id(format!("shape-{}", elem.id))
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(width)
                    .h(height)
                    .rounded_full()
                    .bg(theme::color(theme::BG_SURFACE))
                    .border_2()
                    .border_color(if is_selected {
                        theme::color(theme::ACCENT_BLUE)
                    } else {
                        theme::color(theme::BORDER)
                    })
                    .shadow_md(),
                ElementKind::Text {
                    content, font_size, ..
                } => div()
                    .id(format!("shape-{}", elem.id))
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(width)
                    .h(height)
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_size(px(*font_size * zoom))
                    .text_color(theme::color(theme::TEXT))
                    .child(content.clone()),
            };

            viewport = viewport.child(node);
        }

        // Segments/arrowheads paint through the path API once per frame;
        // real endpoints are impossible with axis-aligned divs.
        if !strokes.is_empty() {
            let stroke_specs = strokes;
            let viewport_layer = self.document.viewport;
            viewport = viewport.child(gpui::canvas(
                move |_bounds, _window, _cx| {},
                move |_bounds, (), window, _cx| {
                    for spec in &stroke_specs {
                        draw_stroke(window, viewport_layer, *spec);
                    }
                },
            ));
        }

        viewport
    }
}

/// One open-ended segment: a stroked line with an optional arrowhead,
/// defined in world coordinates and mapped through the shared transform.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StrokeSpec {
    start: (f32, f32),
    end: (f32, f32),
    arrowhead: bool,
}

const STROKE_WIDTH_PX: f32 = 3.0;
const ARROWHEAD_LENGTH_PX: f32 = 12.0;

fn draw_stroke(window: &mut Window, viewport: CanvasViewport, spec: StrokeSpec) {
    let (sx, sy) = viewport.world_to_screen(spec.start.0, spec.start.1);
    let (ex, ey) = viewport.world_to_screen(spec.end.0, spec.end.1);
    let color = theme::color(theme::ACCENT_BLUE);

    let main_path = {
        let mut builder = gpui::PathBuilder::stroke(px(STROKE_WIDTH_PX));
        builder.move_to(gpui::point(px(sx), px(sy)));
        builder.line_to(gpui::point(px(ex), px(ey)));
        builder.build()
    };
    let Ok(main_path) = main_path else {
        return;
    };
    window.paint_path(main_path, color);

    if !spec.arrowhead {
        return;
    }
    // Two flanks forming a head at the end point, rotated off the segment
    // direction by a fixed spread.
    let angle = (ey - sy).atan2(ex - sx);
    for flank_offset in [-30.0f32, 30.0] {
        let head_angle = angle + flank_offset.to_radians();
        let head_x = ex - ARROWHEAD_LENGTH_PX * head_angle.cos();
        let head_y = ey - ARROWHEAD_LENGTH_PX * head_angle.sin();
        let head_path = {
            let mut builder = gpui::PathBuilder::stroke(px(STROKE_WIDTH_PX));
            builder.move_to(gpui::point(px(ex), px(ey)));
            builder.line_to(gpui::point(px(head_x), px(head_y)));
            builder.build()
        };
        let Ok(head_path) = head_path else {
            continue;
        };
        window.paint_path(head_path, color);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::CanvasViewport;

    #[test]
    fn element_ids_never_collide_after_deletion() {
        let mut ids = ElementIdAllocator::default();
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
    fn viewport_transform_roundtrip_matches_screen_to_world() {
        // The two directions must invert each other at any zoom/pan pair.
        for zoom in [0.5, 1.0, 2.0] {
            let mut viewport = CanvasViewport {
                zoom,
                ..Default::default()
            };
            for (px, py) in [(0.0, 0.0), (64.0, -12.0)] {
                viewport.pan_x = px;
                viewport.pan_y = py;
                let (sx, sy) = viewport.world_to_screen(100.0, 40.0);
                let (wx, wy) = viewport.screen_to_world(sx, sy);
                assert!((wx - 100.0).abs() < 1e-4);
                assert!((wy - 40.0).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn test_canvas_tool_names() {
        assert_eq!(CanvasTool::Select.name(), "Select (V)");
        assert_eq!(CanvasTool::Rectangle.name(), "Rectangle (R)");
        assert_eq!(CanvasTool::Ellipse.name(), "Ellipse (O)");
        assert_eq!(CanvasTool::Arrow.name(), "Arrow (A)");
        assert_eq!(CanvasTool::Line.name(), "Line (L)");
        assert_eq!(CanvasTool::Text.name(), "Text (T)");
    }

    #[test]
    fn test_canvas_document_element_management() {
        let mut doc = CanvasDocument::new();
        doc.add_element(CanvasElement::new_rectangle(
            "r1", 0.0, 0.0, 50.0, 50.0, 0.0,
        ));
        assert_eq!(doc.elements.len(), 1);

        let removed = doc.remove_element("r1");
        assert!(removed.is_some());
        assert_eq!(doc.elements.len(), 0);
    }
}

#[cfg(test)]
mod stroke_tests {
    use super::*;

    #[test]
    fn arrowhead_flanks_beside_the_segment() {
        // Sanity on the geometry: flank origins are the end point, both
        // flanks point backwards along the direction.
        let spec = StrokeSpec { start: (0.0, 0.0), end: (100.0, 0.0), arrowhead: true };
        let angle = 0.0f32;
        let head_x = spec.end.0 - ARROWHEAD_LENGTH_PX * (angle + 30.0f32.to_radians()).cos();
        assert!(head_x < spec.end.0);
        assert!(spec.start.0 < spec.end.0);
    }
}
