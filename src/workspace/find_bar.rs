//! In-editor Find & Replace bar component.

use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*};

use super::Workspace;
use crate::ui::theme;

impl Workspace {
    /// Floating in-editor Find and Replace toolbar.
    pub fn render_find_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count_label = self.find_state.count_label();

        div()
            .id("find-replace-bar")
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .py_1p5()
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
            .border_color(theme::color(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child("Find:"),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_SURFACE))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT))
                            .child(if self.find_state.query.is_empty() {
                                "Type to find...".to_string()
                            } else {
                                self.find_state.query.clone()
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child(count_label),
                    )
                    .child(
                        div()
                            .id("find-prev-btn")
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_SURFACE))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    if let Some(m) = this.find_state.prev_match() {
                                        let start = m.start;
                                        this.editor.update(cx, |ed, cx| {
                                            ed.jump_to_line(ed.cursor_line_col().0, cx);
                                            let _ = start;
                                        });
                                    }
                                    cx.notify();
                                }),
                            )
                            .child("▲"),
                    )
                    .child(
                        div()
                            .id("find-next-btn")
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_SURFACE))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    if let Some(m) = this.find_state.next_match() {
                                        let start = m.start;
                                        this.editor.update(cx, |ed, cx| {
                                            ed.jump_to_line(ed.cursor_line_col().0, cx);
                                            let _ = start;
                                        });
                                    }
                                    cx.notify();
                                }),
                            )
                            .child("▼"),
                    )
                    .child(
                        div()
                            .id("find-case-toggle-btn")
                            .px_1p5()
                            .py_0p5()
                            .rounded_xs()
                            .bg(if self.find_state.case_sensitive {
                                theme::color(theme::TAB_ACTIVE)
                            } else {
                                theme::color(theme::BG_SURFACE)
                            })
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    let text = this.editor.read(cx).text().to_string();
                                    this.find_state.toggle_case_sensitive(&text);
                                    cx.notify();
                                }),
                            )
                            .child("Aa"),
                    )
                    .child(
                        div()
                            .id("find-replace-btn")
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
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.editor.update(cx, |ed, cx| {
                                        this.find_state.replace_current(ed.buffer_mut());
                                        cx.notify();
                                    });
                                    cx.notify();
                                }),
                            )
                            .child("Replace"),
                    )
                    .child(
                        div()
                            .id("find-replace-all-btn")
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
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.editor.update(cx, |ed, cx| {
                                        this.find_state.replace_all(ed.buffer_mut());
                                        cx.notify();
                                    });
                                    cx.notify();
                                }),
                            )
                            .child("All"),
                    ),
            )
            .child(
                div()
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::color(theme::TEXT)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.find_bar_open = false;
                            cx.notify();
                        }),
                    )
                    .child("×"),
            )
    }
}
