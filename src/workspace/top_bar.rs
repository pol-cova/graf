//! Top bar component with breadcrumbs, Quick Open search pill, compile trigger, and layout toggles.

use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{SettingsTab, Workspace};
use crate::compiler::controller::CompileState;
use crate::ui::theme;

impl Workspace {
    /// Top bar with macOS traffic lights inset, breadcrumb, Quick Open trigger, compile, and panel buttons.
    pub fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_compiling = matches!(self.controller.state(), CompileState::Compiling { .. });
        let active_title = self
            .documents
            .get(self.active_doc_idx)
            .map(|d| d.title())
            .unwrap_or("document");

        let engine = self.active_engine();

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(38.0))
            .px_3()
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
            .border_color(theme::color(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().w(px(68.0)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::color(theme::TEXT))
                                    .child("Graf"),
                            )
                            .child(div().text_color(theme::color(theme::TEXT_MUTED)).child("›"))
                            .child(
                                div()
                                    .text_color(theme::color(theme::TEXT))
                                    .child(active_title.to_string()),
                            ),
                    ),
            )
            // Center Search / Quick Open pill
            .child(
                div()
                    .id("quick-open-pill")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .w(px(240.0))
                    .rounded_xs()
                    .bg(theme::color(theme::BG_SURFACE))
                    .border_1()
                    .border_color(theme::color(theme::BORDER))
                    .text_xs()
                    .text_color(theme::color(theme::TEXT_MUTED))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.open_quick_open(cx)),
                    )
                    .child("🔍 Search files...")
                    .child(
                        div()
                            .flex_1()
                            .text_right()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child("⌘P"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // AI Assist Trigger
                    .child(
                        div()
                            .id("ai-assist-btn")
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::ACCENT_BLUE))
                            .text_xs()
                            .text_color(theme::color(theme::ACCENT_BLUE))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.open_ai_assist(cx)),
                            )
                            .child("✨ AI ⌘I"),
                    )
                    // Settings Button
                    .child(
                        div()
                            .id("settings-btn")
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.open_settings(SettingsTab::General, cx);
                                }),
                            )
                            .child("⚙️ ⌘,"),
                    )
                    // Command Palette Trigger
                    .child(
                        div()
                            .id("cmd-palette-btn")
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.open_command_palette(cx)),
                            )
                            .child("⌘K"),
                    )
                    // Compile Button
                    .child(
                        div()
                            .id("compile-btn")
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px_2p5()
                            .py_1()
                            .rounded_xs()
                            .bg(if is_compiling {
                                theme::color(theme::BG_SURFACE)
                            } else {
                                theme::color(theme::LINE_HIGHLIGHT)
                            })
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.trigger_compile(cx)),
                            )
                            .child(if is_compiling {
                                div()
                                    .text_color(theme::color(theme::ACCENT_ORANGE))
                                    .child("● Building...")
                            } else {
                                div()
                                    .text_color(theme::color(theme::ACCENT_BLUE))
                                    .child(format!("▶ {} ⌘B", engine.display_name()))
                            }),
                    )
                    // Sidebar toggle
                    .child(
                        div()
                            .id("sidebar-toggle-btn")
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(if self.sidebar_visible {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)),
                            )
                            .child("◫ Sidebar"),
                    )
                    // Preview toggle
                    .child(
                        div()
                            .id("preview-toggle-btn")
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::BORDER))
                            .text_xs()
                            .text_color(if self.preview_visible {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_preview(cx)),
                            )
                            .child("◨ Preview"),
                    ),
            )
    }
}
