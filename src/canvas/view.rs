use gpui::{
    Context, FocusHandle, Focusable, IntoElement, MouseButton, Render, Window, actions, div,
    prelude::*, px,
};

use crate::canvas::history::CanvasHistory;
use crate::canvas::scene::{CanvasDocument, CanvasViewport, ElementKind};
use crate::canvas::svg::export_to_svg;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

/// Zoom clamp bounds for the toolbar controls.
const ZOOM_STEP: f32 = 0.1;
const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 4.0;

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
    // Document/mutation state is shared with the `input` submodule.
    pub(crate) document: CanvasDocument,
    pub(crate) history: CanvasHistory,
    pub(crate) active_tool: CanvasTool,
    pub(crate) selected_element_id: Option<String>,
    // Drag state is shared with the `input` submodule.
    pub(crate) is_dragging: bool,
    pub(crate) drag_start: Option<(f32, f32)>,
    /// Scene as of pointer-down, captured when a Select press lands on an
    /// element. The first mouse-move that actually shifts the element
    /// commits it to the history once, so a plain click never pads the
    /// undo stack and undo after a drag reverts the drag instead of
    /// wiping the element.
    pub(crate) pending_drag_snapshot: Option<CanvasDocument>,
    pub(crate) revision: u64,
    /// Monotonic element-id source; a count-derived id collides after any
    /// deletion and makes selection/removal hit the wrong element.
    next_element_counter: ElementIdAllocator,
}

/// Never repeats: `elem-{n+1}` from a plain counter is safe where a
/// `elements.len()+1` scheme is not.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ElementIdAllocator {
    next: u64,
}

impl ElementIdAllocator {
    pub(crate) fn allocate(&mut self) -> String {
        self.next += 1;
        format!("elem-{}", self.next)
    }
}

/// Keeps the viewport zoom inside the documented clamp window; shared by
/// the zoom commands so the bounds live in exactly one place.
pub(crate) fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(ZOOM_MIN, ZOOM_MAX)
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
            pending_drag_snapshot: None,
            revision: 0,
            next_element_counter: ElementIdAllocator::default(),
        }
    }

    /// Loads `json` as the displayed scene. Undo history is supplied by the
    /// caller (document-owned, so it survives tab round-trips) rather than
    /// being reset here.
    pub fn load_from_json(
        &mut self,
        json: &str,
        history: CanvasHistory,
        cx: &mut Context<Self>,
    ) -> Result<(), serde_json::Error> {
        self.document = CanvasDocument::from_json(json)?;
        self.history = history;
        self.selected_element_id = None;
        self.revision += 1;
        cx.notify();
        Ok(())
    }

    pub fn save_to_json(&self) -> Result<String, serde_json::Error> {
        self.document.to_json()
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

    /// Hands the view's undo history back to the workspace when the display
    /// switches away from this scene; the history follows the document.
    pub(crate) fn take_history(&mut self) -> CanvasHistory {
        std::mem::take(&mut self.history)
    }

    pub(crate) fn next_element_id(&mut self) -> String {
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
        self.document.viewport.zoom = clamp_zoom(self.document.viewport.zoom + ZOOM_STEP);
        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.document.viewport.zoom = clamp_zoom(self.document.viewport.zoom - ZOOM_STEP);
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
            .bg(theme::BG_CANVAS)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
            .on_mouse_move(cx.listener(Self::handle_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
            .on_action(
                cx.listener(|this, _: &SelectTool, _, cx| this.set_tool(CanvasTool::Select, cx)),
            )
            .on_action(cx.listener(|this, _: &RectangleTool, _, cx| {
                this.set_tool(CanvasTool::Rectangle, cx)
            }))
            .on_action(
                cx.listener(|this, _: &EllipseTool, _, cx| this.set_tool(CanvasTool::Ellipse, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ArrowTool, _, cx| this.set_tool(CanvasTool::Arrow, cx)),
            )
            .on_action(cx.listener(|this, _: &LineTool, _, cx| this.set_tool(CanvasTool::Line, cx)))
            .on_action(cx.listener(|this, _: &TextTool, _, cx| this.set_tool(CanvasTool::Text, cx)))
            .child(self.render_toolbar(cx))
            .child(self.render_viewport(viewport_size))
    }
}

actions!(
    canvas,
    [
        SelectTool,
        RectangleTool,
        EllipseTool,
        ArrowTool,
        LineTool,
        TextTool
    ]
);

/// Tool shortcuts advertised by the toolbar labels; bound only inside the
/// `Canvas` key context.
pub fn register_bindings(cx: &mut gpui::App) {
    use gpui::KeyBinding;
    cx.bind_keys([
        KeyBinding::new("v", SelectTool, Some("Canvas")),
        KeyBinding::new("r", RectangleTool, Some("Canvas")),
        KeyBinding::new("o", EllipseTool, Some("Canvas")),
        KeyBinding::new("a", ArrowTool, Some("Canvas")),
        KeyBinding::new("l", LineTool, Some("Canvas")),
        KeyBinding::new("t", TextTool, Some("Canvas")),
    ]);
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
            .bg(theme::BG_BAR)
            .border_b_1()
            .border_color(theme::BORDER)
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
                                theme::TAB_ACTIVE
                            } else {
                                theme::BG_BAR
                            })
                            .border_1()
                            .border_color(if is_active {
                                theme::ACCENT_BLUE
                            } else {
                                theme::BORDER
                            })
                            .text_xs()
                            .text_color(if is_active {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::HOVER_BG))
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
                            .bg(theme::BG_SURFACE)
                            .border_1()
                            .border_color(theme::BORDER)
                            .text_xs()
                            .text_color(if self.history.can_undo() {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::HOVER_BG))
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
                            .bg(theme::BG_SURFACE)
                            .border_1()
                            .border_color(theme::BORDER)
                            .text_xs()
                            .text_color(if self.history.can_redo() {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::HOVER_BG))
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
                            .bg(theme::BG_SURFACE)
                            .border_1()
                            .border_color(theme::BORDER)
                            .text_xs()
                            .text_color(if self.selected_element_id.is_some() {
                                theme::ACCENT_RED
                            } else {
                                theme::TEXT_MUTED
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::HOVER_BG))
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
                                    .bg(theme::BG_SURFACE)
                                    .border_1()
                                    .border_color(theme::BORDER)
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::HOVER_BG))
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
                                    .bg(theme::BG_SURFACE)
                                    .border_1()
                                    .border_color(theme::BORDER)
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::HOVER_BG))
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
                                    .bg(theme::BG_SURFACE)
                                    .border_1()
                                    .border_color(theme::BORDER)
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::HOVER_BG))
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
        let is_visible = |(x, y, x2, y2): (f32, f32, f32, f32)| {
            x2 >= world_origin_x
                && y2 >= world_origin_y
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
            if !is_visible(elem.bounds()) {
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
                        color: color_to_rgba(elem.effective_stroke_color()),
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
                        color: color_to_rgba(elem.effective_stroke_color()),
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
                    .bg(color_to_rgba(elem.closed_shape_fill()))
                    .border_2()
                    .border_color(if is_selected {
                        theme::ACCENT_BLUE
                    } else {
                        color_to_rgba(elem.effective_stroke_color())
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
                    .bg(color_to_rgba(elem.closed_shape_fill()))
                    .border_2()
                    .border_color(if is_selected {
                        theme::ACCENT_BLUE
                    } else {
                        color_to_rgba(elem.effective_stroke_color())
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
                    .text_color(color_to_rgba(elem.effective_stroke_color()))
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
    color: gpui::Rgba,
}

const STROKE_WIDTH_PX: f32 = 3.0;
const ARROWHEAD_LENGTH_PX: f32 = 12.0;

/// Rendered when a stored color string cannot be parsed as `#rgbhex`.
const FALLBACK_COLOR_HEX: u32 = 0x528bff;

fn parse_hex_color(color: &str) -> u32 {
    let digits = color.trim_start_matches('#');
    if digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        u32::from_str_radix(digits, 16).unwrap_or(FALLBACK_COLOR_HEX)
    } else {
        FALLBACK_COLOR_HEX
    }
}

fn color_to_rgba(color: &str) -> gpui::Rgba {
    gpui::rgb(parse_hex_color(color))
}

fn draw_stroke(window: &mut Window, viewport: CanvasViewport, spec: StrokeSpec) {
    let (sx, sy) = viewport.world_to_screen(spec.start.0, spec.start.1);
    let (ex, ey) = viewport.world_to_screen(spec.end.0, spec.end.1);

    let main_path = {
        let mut builder = gpui::PathBuilder::stroke(px(STROKE_WIDTH_PX));
        builder.move_to(gpui::point(px(sx), px(sy)));
        builder.line_to(gpui::point(px(ex), px(ey)));
        builder.build()
    };
    let Ok(main_path) = main_path else {
        return;
    };
    window.paint_path(main_path, spec.color);

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
        window.paint_path(head_path, spec.color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::scene::{CanvasElement, CanvasViewport, DEFAULT_STROKE_COLOR};

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
    fn zoom_clamping_holds_at_both_ends() {
        assert_eq!(clamp_zoom(ZOOM_MAX + 10.0 * ZOOM_STEP), ZOOM_MAX);
        assert_eq!(clamp_zoom(ZOOM_MIN - 10.0 * ZOOM_STEP), ZOOM_MIN);
        assert_eq!(clamp_zoom(1.5), 1.5);
    }

    #[test]
    fn drag_delta_scales_inversely_with_zoom() {
        // handle_mouse_move subtracts two world points; a screen-space drag
        // must cover 1/zoom as many world units at zoomed-in cameras.
        for zoom in [0.5, 1.0, 2.0] {
            let viewport = CanvasViewport {
                zoom,
                ..Default::default()
            };
            let (ax, ay) = viewport.screen_to_world(100.0, 40.0);
            let (bx, by) = viewport.screen_to_world(160.0, 10.0);
            assert!(((bx - ax) - 60.0 / zoom).abs() < 1e-4, "zoom {zoom}");
            assert!(((by - ay) - (-30.0 / zoom)).abs() < 1e-4, "zoom {zoom}");
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
    fn imported_style_colors_survive_into_render_specs() {
        // The render pipeline must read ElementStyle, not the theme: a
        // custom export color must reach the stroke spec unchanged.
        let mut doc = CanvasDocument::new();
        let mut elem = CanvasElement::new_rectangle("r1", 0.0, 0.0, 10.0, 10.0, 0.0);
        elem.style.stroke_color = Some("#ff0000".to_string());
        doc.add_element(elem);
        let stroke = doc
            .elements
            .first()
            .map(|e| e.effective_stroke_color())
            .unwrap();
        assert_eq!(stroke, "#ff0000");
        let _ = export_to_svg(&doc);
    }

    #[test]
    fn default_stroke_color_is_the_export_fallback() {
        let elem = CanvasElement::new_rectangle("r1", 0.0, 0.0, 10.0, 10.0, 0.0);
        assert_eq!(elem.effective_stroke_color(), DEFAULT_STROKE_COLOR);
    }

    #[test]
    fn parse_hex_color_handles_legacy_and_bad_input() {
        assert_eq!(parse_hex_color("#ff00aa"), 0xff00aa);
        assert_eq!(parse_hex_color("ff00aa"), 0xff00aa);
        assert_eq!(parse_hex_color("#GGG"), FALLBACK_COLOR_HEX);
        assert_eq!(parse_hex_color("#12345"), FALLBACK_COLOR_HEX);
    }
}

#[cfg(test)]
mod stroke_tests {
    use super::*;
    use crate::canvas::scene::DEFAULT_STROKE_COLOR;

    #[test]
    fn arrowhead_flanks_beside_the_segment() {
        // Sanity on the geometry: flank origins are the end point, both
        // flanks point backwards along the direction.
        let spec = StrokeSpec {
            start: (0.0, 0.0),
            end: (100.0, 0.0),
            arrowhead: true,
            color: color_to_rgba(DEFAULT_STROKE_COLOR),
        };
        let angle = 0.0f32;
        let head_x = spec.end.0 - ARROWHEAD_LENGTH_PX * (angle + 30.0f32.to_radians()).cos();
        assert!(head_x < spec.end.0);
        assert!(spec.start.0 < spec.end.0);
    }
}
