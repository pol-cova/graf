use gpui::{Context, FontWeight, IntoElement, ParentElement, Role, Styled, div, prelude::*, px};

use super::Workspace;
use crate::ui::theme;

impl Workspace {
    pub fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(560.0))
                    .max_w_full()
                    .px_8()
                    .pb(px(72.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .mb_8()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(58.0))
                                    .rounded_lg()
                                    .bg(theme::BG_SURFACE)
                                    .border_1()
                                    .border_color(theme::BORDER)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(20.0))
                                    .text_color(theme::TEXT)
                                    .child("\\ I"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Welcome to graf"),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(theme::TEXT_MUTED)
                                            .child("A native workspace for LaTeX and Typst"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT_MUTED)
                            .mb_2()
                            .child("GET STARTED"),
                    )
                    .child(self.render_welcome_actions(cx)),
            )
    }

    fn render_welcome_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::BORDER)
            .child(
                div()
                    .id("welcome-template")
                    .flex()
                    .items_center()
                    .h(px(44.0))
                    .px_2()
                    .gap_3()
                    .role(Role::Button)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::HOVER_BG))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.open_template_picker(None, false, cx)),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_xs()
                            .text_color(theme::ACCENT_BLUE)
                            .child("T"),
                    )
                    .child(div().flex_1().child("New from template"))
                    .child(div().text_xs().text_color(theme::TEXT_MUTED).child("⌘⇧N")),
            )
            .child(
                div()
                    .id("welcome-new-project")
                    .flex()
                    .items_center()
                    .h(px(44.0))
                    .px_2()
                    .gap_3()
                    .role(Role::Button)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::HOVER_BG))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.new_project(cx)),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_lg()
                            .text_color(theme::TEXT_MUTED)
                            .child("+"),
                    )
                    .child(div().flex_1().child("New project")),
            )
            .child(
                div()
                    .id("welcome-open-file")
                    .flex()
                    .items_center()
                    .h(px(44.0))
                    .px_2()
                    .gap_3()
                    .role(Role::Button)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::HOVER_BG))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.open_file_picker(cx)),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_lg()
                            .text_color(theme::TEXT_MUTED)
                            .child("+"),
                    )
                    .child(div().flex_1().child("Open document"))
                    .child(div().text_xs().text_color(theme::TEXT_MUTED).child("⌘O")),
            )
            .child(
                div()
                    .id("welcome-command-palette")
                    .flex()
                    .items_center()
                    .h(px(44.0))
                    .px_2()
                    .gap_3()
                    .role(Role::Button)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::HOVER_BG))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.open_command_palette(cx)),
                    )
                    .child(
                        div()
                            .w(px(34.0))
                            .text_sm()
                            .text_color(theme::TEXT_MUTED)
                            .child(">"),
                    )
                    .child(div().flex_1().child("Open command palette"))
                    .child(div().text_xs().text_color(theme::TEXT_MUTED).child("⌘⇧P")),
            )
    }
}
