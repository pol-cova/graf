//! Status bar component showing compile state, active engine, and document telemetry.

use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{ActiveViewKind, Workspace};
use crate::compiler::controller::CompileState;
use crate::ui::theme;

impl Workspace {
    /// Bottom status bar showing compile status, active engine, errors (clickable), and cursor position.
    pub fn render_status_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let (status_icon, status_color, status_text) = match self.controller.state() {
            CompileState::Idle => ("○", theme::TEXT_MUTED, "ready".to_string()),
            CompileState::Waiting => ("●", theme::ACCENT_ORANGE, "typing...".to_string()),
            CompileState::Compiling { revision, .. } => (
                "●",
                theme::ACCENT_ORANGE,
                format!("compiling rev {revision}..."),
            ),
            CompileState::Success {
                revision, duration, ..
            } => (
                "●",
                theme::ACCENT_GREEN,
                format!(
                    "ready (rev {revision}, {:.0}ms)",
                    duration.as_secs_f64() * 1000.0
                ),
            ),
            CompileState::Failed { diagnostics, .. } => {
                let count = diagnostics.len();
                (
                    "●",
                    theme::ACCENT_RED,
                    if count == 1 {
                        "1 error (click to view)".to_string()
                    } else {
                        format!("{count} errors (click to view)")
                    },
                )
            }
        };

        let (line, col) = self.editor.read(cx).cursor_line_col();
        let engine = self.active_engine();

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(26.0))
            .px_3()
            .bg(theme::color(theme::BG_BAR))
            .border_t_1()
            .border_color(theme::color(theme::BORDER))
            .text_xs()
            .text_color(theme::color(theme::TEXT_MUTED))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("status-toggle-diag")
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::color(theme::TEXT)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.toggle_diagnostics(cx)),
                            )
                            .child(
                                div()
                                    .text_color(theme::color(status_color))
                                    .child(status_icon),
                            )
                            .child(
                                div()
                                    .text_color(if status_color == theme::ACCENT_RED {
                                        theme::color(theme::ACCENT_RED)
                                    } else {
                                        theme::color(theme::TEXT_MUTED)
                                    })
                                    .child(status_text),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_color(theme::color(theme::ACCENT_BLUE))
                            .child(engine.icon())
                            .child(engine.display_name()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(if self.active_view_kind == ActiveViewKind::Canvas {
                        format!("Canvas (rev {})", self.canvas.read(cx).revision())
                    } else {
                        let text = self.editor.read(cx).text();
                        let is_typst = self.documents[self.active_doc_idx]
                            .title()
                            .ends_with(".typ");
                        let stats = crate::project::stats::DocumentStats::compute(text, is_typst);
                        format!(
                            "Ln {line}, Col {col} • {} words • {:.1} pgs",
                            stats.word_count, stats.estimated_pages
                        )
                    })
                    .child("UTF-8")
                    .child(
                        div()
                            .text_color(theme::color(theme::ACCENT_GREEN))
                            .child("⚡ Hot Reload"),
                    ),
            )
    }
}
