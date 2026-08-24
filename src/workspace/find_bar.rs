//! In-editor search bar component.

use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*};

use super::Workspace;
use crate::ui::theme;

impl Workspace {
    /// In-editor search toolbar.
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
                            .w(gpui::px(240.0))
                            .h(gpui::px(26.0))
                            .rounded_xs()
                            .bg(theme::color(theme::BG))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .overflow_hidden()
                            .child(self.prompt_editor.clone()),
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
                                    if let Some(matched) = this.find_state.prev_match() {
                                        this.editor.update(cx, |editor, cx| {
                                            editor.select_range(matched.start..matched.end, cx);
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
                                    if let Some(matched) = this.find_state.next_match() {
                                        this.editor.update(cx, |editor, cx| {
                                            editor.select_range(matched.start..matched.end, cx);
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
