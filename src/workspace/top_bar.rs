use gpui::{Context, Focusable, IntoElement, ParentElement, Role, Styled, div, prelude::*, px};

use super::{SettingsTab, Workspace};
use crate::compiler::controller::CompileState;
use crate::ui::icons::{Icon, icon, icon_colored};
use crate::ui::theme;

impl Workspace {
    pub fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_compiling = matches!(self.controller.state(), CompileState::Compiling { .. });
        let can_compile = self.active_document_is_compilable();
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(38.0))
            .pl(px(78.0))
            .pr_2()
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
            .border_color(theme::color(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .w(px(150.0))
                    .child(
                        div()
                            .id("toolbar-project")
                            .aria_label("Toggle project")
                            .role(Role::Button)
                            .w(px(26.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_xs()
                            .text_sm()
                            .text_color(if self.sidebar_visible {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)),
                            )
                            .child(div().w(px(15.0)).h(px(15.0)).child(icon_colored(
                                Icon::PanelLeft,
                                theme::color(if self.sidebar_visible {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_MUTED
                                }),
                            ))),
                    )
                    .child(
                        div()
                            .id("toolbar-preview")
                            .aria_label("Toggle preview")
                            .role(Role::Button)
                            .w(px(26.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_xs()
                            .text_sm()
                            .text_color(if self.preview_visible {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_preview(cx)),
                            )
                            .child(div().w(px(15.0)).h(px(15.0)).child(icon_colored(
                                Icon::PanelRight,
                                theme::color(if self.preview_visible {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_MUTED
                                }),
                            ))),
                    )
                    .child(
                        div()
                            .id("toolbar-problems")
                            .aria_label("Toggle problems")
                            .role(Role::Button)
                            .w(px(26.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_xs()
                            .text_xs()
                            .text_color(if !self.latest_diagnostics.is_empty() {
                                theme::color(theme::ACCENT_RED)
                            } else if self.diagnostics_drawer_open {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_diagnostics(cx)),
                            )
                            .child(div().w(px(15.0)).h(px(15.0)).child(icon_colored(
                                Icon::PanelBottom,
                                theme::color(if !self.latest_diagnostics.is_empty() {
                                    theme::ACCENT_RED
                                } else if self.diagnostics_drawer_open {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_MUTED
                                }),
                            ))),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .w(px(150.0))
                    .child(
                        div()
                            .id("compile-btn")
                            .aria_label("Compile document")
                            .role(Role::Button)
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
                            } else if can_compile {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.trigger_compile(cx)),
                            )
                            .child(if is_compiling { "Compiling" } else { "Compile" }),
                    )
                    .child(
                        div()
                            .id("workspace-menu-btn")
                            .aria_label("Workspace menu")
                            .role(Role::Button)
                            .w(px(26.0))
                            .h(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
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
                            .child(
                                div()
                                    .w(px(15.0))
                                    .h(px(15.0))
                                    .child(icon(Icon::MoreHorizontal)),
                            ),
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
                            .child("Project")
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
                            .child("Preview")
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
                            .child("Frame Timings")
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
                                    this.open_about(cx);
                                }),
                            )
                            .child("About"),
                    ),
            )
    }
}
