use gpui::{
    Context, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, Window, div, prelude::*, px,
};

use crate::canvas::history::CanvasHistory;
use crate::canvas::scene::{CanvasDocument, CanvasElement, ElementKind};
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

    fn next_element_id(&self) -> String {
        format!("elem-{}", self.document.elements.len() + 1)
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

        let x = event.position.x.as_f32();
        let y = event.position.y.as_f32();

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
            let dx = current_x - start_x;
            let dy = current_y - start_y;

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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.render_viewport())
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

        div()
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

    fn render_viewport(&self) -> impl IntoElement {
        let zoom = self.document.viewport.zoom;

        let mut viewport = div()
            .id("canvas-viewport")
            .relative()
            .flex_1()
            .size_full()
            .overflow_hidden();

        for elem in &self.document.elements {
            let is_selected = self.selected_element_id.as_deref() == Some(&elem.id);
            let left = px(elem.x * zoom);
            let top = px(elem.y * zoom);
            let width = px(elem.width * zoom);
            let height = px(elem.height * zoom);

            let node = match &elem.kind {
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
                    let min_x = start_x.min(*end_x) * zoom;
                    let min_y = start_y.min(*end_y) * zoom;
                    let w = ((end_x - start_x).abs() * zoom).max(4.0);
                    let h = ((end_y - start_y).abs() * zoom).max(4.0);

                    div()
                        .id(format!("shape-{}", elem.id))
                        .absolute()
                        .left(px(min_x))
                        .top(px(min_y))
                        .w(px(w))
                        .h(px(h))
                        .bg(theme::color(theme::ACCENT_BLUE))
                        .rounded_xs()
                }
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

        viewport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
