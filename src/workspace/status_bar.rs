use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{ActiveViewKind, Workspace};
use crate::compiler::controller::CompileState;
use crate::ui::theme;

impl Workspace {
    pub fn render_status_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let (status_color, status_text) = if let Some(error) = &self.workspace_error {
            (theme::ACCENT_RED, error.clone())
        } else {
            match self.controller.state() {
                CompileState::Idle => (theme::TEXT_MUTED, "Ready".to_string()),
                CompileState::Waiting => (theme::ACCENT_ORANGE, "Compile queued".to_string()),
                CompileState::Compiling { .. } => {
                    (theme::ACCENT_ORANGE, "Compiling...".to_string())
                }
                CompileState::Success { duration, .. } => (
                    theme::TEXT_MUTED,
                    format!("Compiled in {:.0} ms", duration.as_secs_f64() * 1000.0),
                ),
                CompileState::Failed { diagnostics, .. } => {
                    let count = diagnostics.len();
                    let label = if count == 1 {
                        "1 error".to_string()
                    } else {
                        format!("{count} errors")
                    };
                    (theme::ACCENT_RED, label)
                }
            }
        };

        let (line, col) = self.editor.read(cx).cursor_line_col();
        let title = self.documents[self.active_doc_idx].title();
        let is_typst = title.ends_with(".typ");
        let language = if is_typst {
            "Typst"
        } else if title.ends_with(".tex") {
            "LaTeX"
        } else {
            "Plain Text"
        };

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
                div().flex().items_center().gap_2().child(
                    div()
                        .id("status-toggle-diag")
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::color(theme::TEXT)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, _, cx| this.toggle_diagnostics(cx)),
                        )
                        .child(
                            div()
                                .max_w(px(520.0))
                                .truncate()
                                .text_color(theme::color(status_color))
                                .child(status_text),
                        ),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(if self.active_view_kind == ActiveViewKind::Canvas {
                        format!("Canvas r{}", self.canvas.read(cx).revision())
                    } else {
                        format!("{line}:{col}")
                    })
                    .child(language)
                    .child(if self.active_view_kind == ActiveViewKind::Canvas {
                        String::new()
                    } else {
                        let text = self.editor.read(cx).text();
                        let stats = crate::project::stats::DocumentStats::compute(text, is_typst);
                        format!("{} words", stats.word_count)
                    })
                    .child("UTF-8")
                    .when(self.active_document_is_compilable(), |status| {
                        status.child(
                            div()
                                .text_color(theme::color(theme::ACCENT_GREEN))
                                .child("Auto compile"),
                        )
                    }),
            )
    }
}
