//! Compact workspace toolbar inspired by native code editors.

use gpui::{Context, Focusable, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{SettingsTab, Workspace};
use crate::compiler::controller::CompileState;
use crate::ui::theme;

impl Workspace {
    pub fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_compiling = matches!(self.controller.state(), CompileState::Compiling { .. });
        let active_title = self
            .documents
            .get(self.active_doc_idx)
            .map(|document| document.title())
            .unwrap_or("document");

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(36.0))
            .px_2()
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
            .border_color(theme::color(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w(px(220.0))
                    .child(div().w(px(68.0)))
                    .child(
                        div()
                            .max_w(px(180.0))
                            .truncate()
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child(active_title.to_string()),
                    ),
            )
            .child(
                div()
                    .id("quick-open-pill")
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(26.0))
                    .w(px(260.0))
                    .rounded_xs()
                    .bg(theme::color(theme::BG_SURFACE))
                    .border_1()
                    .border_color(theme::color(theme::BORDER))
                    .text_xs()
                    .text_color(theme::color(theme::TEXT_MUTED))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.open_quick_open(cx);
                            window.focus(&this.prompt_editor.read(cx).focus_handle(cx), cx);
                        }),
                    )
                    .child("Search project")
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
                    .justify_end()
                    .gap_1()
                    .min_w(px(220.0))
                    .child(
                        div()
                            .id("compile-btn")
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(if is_compiling {
                                theme::color(theme::HOVER_BG)
                            } else {
                                theme::color(theme::BG_BAR)
                            })
                            .text_xs()
                            .text_color(if is_compiling {
                                theme::color(theme::ACCENT_ORANGE)
                            } else {
                                theme::color(theme::TEXT)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.trigger_compile(cx)),
                            )
                            .child(if is_compiling { "Building" } else { "Build" }),
                    )
                    .child(
                        div()
                            .id("workspace-menu-btn")
                            .px_2()
                            .py_1()
                            .rounded_xs()
                            .bg(if self.workspace_menu_open {
                                theme::color(theme::HOVER_BG)
                            } else {
                                theme::color(theme::BG_BAR)
                            })
                            .text_sm()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_workspace_menu(cx)),
                            )
                            .child("..."),
                    ),
            )
    }

    pub fn render_workspace_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_row = || {
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .px_3()
                .py_1p5()
                .text_xs()
                .text_color(theme::color(theme::TEXT))
                .cursor_pointer()
                .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
        };

        div()
            .id("workspace-menu-backdrop")
            .absolute()
            .size_full()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.workspace_menu_open = false;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("workspace-menu")
                    .absolute()
                    .top(px(38.0))
                    .right(px(8.0))
                    .w(px(210.0))
                    .py_1()
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
                        menu_row()
                            .id("menu-command-palette")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.workspace_menu_open = false;
                                    this.open_command_palette(cx);
                                    window.focus(&this.prompt_editor.read(cx).focus_handle(cx), cx);
                                }),
                            )
                            .child("Command Palette")
                            .child("⌘K"),
                    )
                    .child(
                        menu_row()
                            .id("menu-project-panel")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.workspace_menu_open = false;
                                    this.toggle_sidebar(cx);
                                }),
                            )
                            .child("Project Panel")
                            .child(if self.sidebar_visible { "On" } else { "Off" }),
                    )
                    .child(
                        menu_row()
                            .id("menu-preview-panel")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.workspace_menu_open = false;
                                    this.toggle_preview(cx);
                                }),
                            )
                            .child("PDF Preview")
                            .child(if self.preview_visible { "On" } else { "Off" }),
                    )
                    .child(
                        menu_row()
                            .id("menu-problems-panel")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.workspace_menu_open = false;
                                    this.toggle_diagnostics(cx);
                                }),
                            )
                            .child("Problems")
                            .child(if self.diagnostics_drawer_open {
                                "On"
                            } else {
                                "Off"
                            }),
                    )
                    .child(div().h(px(1.0)).my_1().bg(theme::color(theme::BORDER)))
                    .child(
                        menu_row()
                            .id("menu-performance-overlay")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.workspace_menu_open = false;
                                    this.toggle_performance_overlay(window, cx);
                                }),
                            )
                            .child("Performance Overlay")
                            .child("⌘⇧D"),
                    )
                    .child(
                        menu_row()
                            .id("menu-settings")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.workspace_menu_open = false;
                                    this.open_settings(SettingsTab::Editor, cx);
                                }),
                            )
                            .child("Settings")
                            .child("⌘,"),
                    )
                    .child(
                        menu_row()
                            .id("menu-about")
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.workspace_menu_open = false;
                                    this.open_settings(SettingsTab::Licenses, cx);
                                }),
                            )
                            .child("About and Licenses"),
                    ),
            )
    }
}
