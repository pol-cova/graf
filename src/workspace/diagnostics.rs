//! Diagnostics problems drawer component for compiler errors and linter warnings.

use gpui::{Context, CursorStyle, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{DiagnosticsFilter, ResizingPanel, Workspace};
use crate::compiler::diagnostics::{Diagnostic, Severity};
use crate::ui::theme;

impl Workspace {
    /// Diagnostics drawer listing compilation errors with clickable jump-to-line links.
    pub fn render_diagnostics_drawer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut drawer = div()
            .id("diagnostics-drawer")
            .flex()
            .flex_none()
            .flex_col()
            .h(px(self.diagnostics_height))
            .bg(theme::color(theme::BG_BAR))
            .border_t_1()
            .border_color(theme::color(theme::BORDER))
            .overflow_scroll();

        let error_count = self
            .latest_diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warn_count = self
            .latest_diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();

        drawer = drawer.child(
            div()
                .id("diagnostics-resize-handle")
                .flex_none()
                .h(px(5.0))
                .w_full()
                .bg(theme::color(theme::BORDER))
                .cursor(CursorStyle::ResizeUpDown)
                .hover(|style| style.bg(theme::color(theme::ACCENT_BLUE)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.begin_panel_resize(ResizingPanel::Diagnostics, cx);
                    }),
                ),
        );

        drawer = drawer.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(theme::color(theme::BORDER))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .cursor_pointer()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(if self.diagnostics_filter == DiagnosticsFilter::All {
                                    theme::color(theme::TEXT)
                                } else {
                                    theme::color(theme::TEXT_MUTED)
                                })
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.diagnostics_filter = DiagnosticsFilter::All;
                                        cx.notify();
                                    }),
                                )
                                .child(format!("ALL ({})", self.latest_diagnostics.len())),
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(
                                    if self.diagnostics_filter == DiagnosticsFilter::Errors {
                                        theme::color(theme::ACCENT_RED)
                                    } else {
                                        theme::color(theme::TEXT_MUTED)
                                    },
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.diagnostics_filter = DiagnosticsFilter::Errors;
                                        cx.notify();
                                    }),
                                )
                                .child(format!("ERRORS ({error_count})")),
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(
                                    if self.diagnostics_filter == DiagnosticsFilter::Warnings {
                                        theme::color(theme::ACCENT_ORANGE)
                                    } else {
                                        theme::color(theme::TEXT_MUTED)
                                    },
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.diagnostics_filter = DiagnosticsFilter::Warnings;
                                        cx.notify();
                                    }),
                                )
                                .child(format!("WARNINGS ({warn_count})")),
                        ),
                )
                .child(
                    div()
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::color(theme::TEXT)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, _, cx| this.toggle_diagnostics(cx)),
                        )
                        .child("×"),
                ),
        );

        // Filtered diagnostic list
        let filtered_diags: Vec<&Diagnostic> = self
            .latest_diagnostics
            .iter()
            .filter(|d| match self.diagnostics_filter {
                DiagnosticsFilter::All => true,
                DiagnosticsFilter::Errors => d.severity == Severity::Error,
                DiagnosticsFilter::Warnings => d.severity == Severity::Warning,
            })
            .collect();

        for diag in filtered_diags {
            let is_error = diag.severity == Severity::Error;
            let line_num = diag.line.unwrap_or(1);
            let msg = diag.message.clone();

            let row = div()
                .id(format!("diag-row-{}", diag.id.0))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .text_xs()
                .cursor_pointer()
                .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.jump_to_line(line_num, cx);
                    }),
                )
                .child(
                    div()
                        .text_color(if is_error {
                            theme::color(theme::ACCENT_RED)
                        } else {
                            theme::color(theme::ACCENT_ORANGE)
                        })
                        .child(if is_error { "● Error" } else { "▲ Warning" }),
                )
                .child(
                    div()
                        .text_color(theme::color(theme::ACCENT_BLUE))
                        .child(format!("Ln {line_num}:")),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(theme::color(theme::TEXT))
                        .child(msg),
                );

            drawer = drawer.child(row);
        }

        drawer
    }
}
