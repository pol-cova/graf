use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{ActiveViewKind, Workspace};
use crate::compiler::controller::CompileState;
use crate::ui::theme;

impl Workspace {
    /// Word count for the status bar, recomputed only when the editor
    /// changes: the bar re-renders every frame, so the scan is cached by
    /// (editor revision, Typst-ness), the same pattern as `outline_cache`.
    fn cached_word_count(&self, cx: &Context<Self>) -> usize {
        let editor = self.editor.read(cx);
        let revision = editor.revision();
        let is_typst =
            self.active_document_kind() == Some(crate::project::document::DocumentKind::Typst);

        if let Some((cached_revision, cached_is_typst, count)) =
            self.word_count_cache.borrow().as_ref()
            && *cached_revision == revision
            && *cached_is_typst == is_typst
        {
            return *count;
        }

        let stats = crate::project::stats::DocumentStats::compute(editor.text(), is_typst);
        *self.word_count_cache.borrow_mut() = Some((revision, is_typst, stats.word_count));
        stats.word_count
    }
    pub fn render_status_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        // Text comes from the canonical `CompileState::status_text` so the
        // bar can never drift from the controller; only the tint is local.
        let (status_color, status_text) = if let Some(error) = &self.workspace_error {
            (theme::ACCENT_RED, error.clone())
        } else {
            let status_color = match self.controller.state() {
                CompileState::Idle | CompileState::Success { .. } => theme::TEXT_MUTED,
                CompileState::Waiting | CompileState::Compiling { .. } => theme::ACCENT_ORANGE,
                CompileState::Failed { .. } => theme::ACCENT_RED,
            };
            (status_color, self.controller.status_text().to_string())
        };

        let (line, col) = self.editor.read(cx).cursor_line_col();
        let language = match self.active_document_kind() {
            Some(crate::project::document::DocumentKind::Typst) => "Typst",
            Some(crate::project::document::DocumentKind::Latex) => "LaTeX",
            _ => "Plain Text",
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
                        format!("{} words", self.cached_word_count(cx))
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
