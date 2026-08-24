use gpui::{Context, IntoElement, ParentElement, Role, Styled, div, prelude::*, px};

use super::Workspace;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

impl Workspace {
    pub fn render_find_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button = || {
            div()
                .flex()
                .items_center()
                .justify_center()
                .w(px(26.0))
                .h(px(24.0))
                .rounded_xs()
                .text_xs()
                .text_color(theme::color(theme::TEXT_MUTED))
                .cursor_pointer()
                .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
        };

        div()
            .id("find-bar")
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .flex()
            .items_center()
            .gap_1()
            .w(px(440.0))
            .h(px(36.0))
            .px_1()
            .rounded_xs()
            .bg(theme::color(theme::BG_SURFACE))
            .border_1()
            .border_color(theme::color(theme::BORDER))
            .shadow_lg()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .h(px(26.0))
                    .px_2()
                    .rounded_xs()
                    .bg(theme::color(theme::BG))
                    .border_1()
                    .border_color(theme::color(theme::BORDER))
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .child(self.prompt_editor.clone()),
                    ),
            )
            .child(
                div()
                    .min_w(px(48.0))
                    .text_center()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT_MUTED))
                    .child(self.find_state.count_label()),
            )
            .child(
                button()
                    .id("find-previous")
                    .role(Role::Button)
                    .aria_label("Previous match")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if let Some(matched) = this.find_state.prev_match().cloned() {
                                this.editor
                                    .update(cx, |editor, cx| editor.select_range(matched, cx));
                            }
                            cx.notify();
                        }),
                    )
                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::ChevronUp))),
            )
            .child(
                button()
                    .id("find-next")
                    .role(Role::Button)
                    .aria_label("Next match")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if let Some(matched) = this.find_state.next_match().cloned() {
                                this.editor
                                    .update(cx, |editor, cx| editor.select_range(matched, cx));
                            }
                            cx.notify();
                        }),
                    )
                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::ChevronDown))),
            )
            .child(
                button()
                    .id("find-case-sensitive")
                    .role(Role::Button)
                    .aria_label("Match case")
                    .bg(if self.find_state.case_sensitive {
                        theme::color(theme::TAB_ACTIVE)
                    } else {
                        theme::color(theme::BG_SURFACE)
                    })
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
                button()
                    .id("close-find")
                    .role(Role::Button)
                    .aria_label("Close find")
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.find_bar_open = false;
                            cx.notify();
                        }),
                    )
                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Close))),
            )
    }
}
