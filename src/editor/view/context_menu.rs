//! The editor's right-click context menu, extracted from the render
//! pipeline so its row-closure boilerplate stays in its own file.

use gpui::{Context, IntoElement, MouseButton, Window, div, prelude::*, px};

use super::EditorView;
use super::*;
use crate::ui::theme;

impl EditorView {
    pub(super) fn render_context_menu(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some((menu_x, menu_y)) = self.context_menu_position else {
            return div();
        };
        let mut root = div();
        let menu_row = || {
            div()
                .w_full()
                .px_3()
                .py_1p5()
                .text_xs()
                .text_color(theme::color(theme::TEXT))
                .cursor_pointer()
                .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
        };
        let separator = || div().h(px(1.0)).my_1().bg(theme::color(theme::BORDER));

        root = root.child(
            div()
                .id("editor-context-menu")
                .role(Role::Menu)
                .aria_label("Editor actions")
                .absolute()
                .left(px(menu_x))
                .top(px(menu_y))
                .w(px(180.0))
                .py_1()
                .rounded_xs()
                .border_1()
                .border_color(theme::color(theme::BORDER))
                .bg(theme::color(theme::BG_SURFACE))
                .shadow_lg()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(
                    menu_row()
                        .id("context-undo")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                this.on_undo(&Undo, window, cx);
                            }),
                        )
                        .child("Undo"),
                )
                .child(
                    menu_row()
                        .id("context-redo")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                this.on_redo(&Redo, window, cx);
                            }),
                        )
                        .child("Redo"),
                )
                .child(separator())
                .child(
                    menu_row()
                        .id("context-cut")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                this.on_cut(&Cut, window, cx);
                            }),
                        )
                        .child("Cut"),
                )
                .child(
                    menu_row()
                        .id("context-copy")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                this.on_copy(&Copy, window, cx);
                                cx.notify();
                            }),
                        )
                        .child("Copy"),
                )
                .child(
                    menu_row()
                        .id("context-paste")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                this.on_paste(&Paste, window, cx);
                            }),
                        )
                        .child("Paste"),
                )
                .child(
                    menu_row()
                        .id("context-select-all")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                this.on_select_all(&SelectAll, window, cx);
                            }),
                        )
                        .child("Select All"),
                )
                .child(separator())
                .child(
                    menu_row()
                        .id("context-find")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.context_menu_position = None;
                                window.dispatch_action(Box::new(crate::workspace::ToggleFind), cx);
                            }),
                        )
                        .child("Find"),
                )
                .child(
                    menu_row()
                        .id("context-find-references")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.context_menu_position = None;
                                cx.emit(EditorEvent::FindReferences);
                                cx.notify();
                            }),
                        )
                        .child("Find All References"),
                ),
        );

        root
    }
}
