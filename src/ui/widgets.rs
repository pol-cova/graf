use gpui::{
    AnyElement, App, InteractiveElement, MouseDownEvent, ParentElement, Stateful, Styled, Window,
    div, px,
};

use crate::ui::theme;

/// Shared skeleton for selectable list rows in popups (quick open, command
/// palette, completion); extras such as roles or spacing overrides chain
/// onto the returned element.
pub fn list_row(
    id: impl Into<gpui::ElementId>,
    on_click: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    children: impl IntoIterator<Item = AnyElement>,
) -> Stateful<gpui::Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .px_3()
        .py_1p5()
        .text_xs()
        .text_color(theme::TEXT)
        .hover(|style| style.bg(theme::HOVER_BG))
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, on_click)
        .children(children)
}

/// Fixed-width accent label at the top of a list row (kind tag, TEX/TYP…).
pub fn list_row_lead(label: impl Into<String>) -> gpui::Div {
    div()
        .flex_none()
        .w(px(32.0))
        .text_color(theme::ACCENT_BLUE)
        .child(label.into())
}
