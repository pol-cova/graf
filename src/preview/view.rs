//! Preview view for rendering compiled PDF pages.

use gpui::{Context, IntoElement, Render, Window, div, img, prelude::*, px};

use super::renderer::RenderedPage;
use crate::ui::theme;

/// The right-panel PDF preview view in the workspace.
pub struct PreviewView {
    pages: Vec<RenderedPage>,
    scale: f32,
    is_retained_stale: bool,
    last_error_summary: Option<String>,
}

impl Default for PreviewView {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewView {
    /// Creates a new empty `PreviewView`.
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            scale: 1.0,
            is_retained_stale: false,
            last_error_summary: None,
        }
    }

    /// Updates the view with newly rendered pages from a successful compilation.
    pub fn set_rendered_pages(&mut self, pages: Vec<RenderedPage>, cx: &mut Context<Self>) {
        self.pages = pages;
        self.is_retained_stale = false;
        self.last_error_summary = None;
        cx.notify();
    }

    /// Retains existing pages while marking that the latest compilation failed.
    pub fn set_compile_failed(&mut self, error_msg: Option<String>, cx: &mut Context<Self>) {
        self.is_retained_stale = true;
        self.last_error_summary = error_msg;
        cx.notify();
    }

    /// Zoom in by 10%.
    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.scale = (self.scale + 0.1).min(3.0);
        cx.notify();
    }

    /// Zoom out by 10%.
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.scale = (self.scale - 0.1).max(0.4);
        cx.notify();
    }

    /// Reset zoom to 100%.
    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.scale = 1.0;
        cx.notify();
    }
}

impl Render for PreviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page_count = self.pages.len();

        div()
            .flex()
            .flex_1()
            .flex_col()
            .size_full()
            .bg(theme::color(theme::BG_SURFACE))
            .child(self.render_toolbar(page_count, cx))
            .child(self.render_content())
    }
}

impl PreviewView {
    fn render_toolbar(&self, page_count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let page_label = if page_count == 0 {
            "No pages".to_string()
        } else {
            format!("Page 1 of {page_count}")
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(32.0))
            .px_3()
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
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
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::color(theme::TEXT))
                            .child("PDF Preview"),
                    )
                    .child(page_label),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .id("zoom-out-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.zoom_out(cx)),
                            )
                            .child("−"),
                    )
                    .child(
                        div()
                            .id("zoom-reset-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.reset_zoom(cx)),
                            )
                            .child(format!("{:.0}%", self.scale * 100.0)),
                    )
                    .child(
                        div()
                            .id("zoom-in-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.zoom_in(cx)),
                            )
                            .child("+"),
                    ),
            )
    }

    fn render_content(&self) -> impl IntoElement {
        let mut container = div()
            .id("preview-content")
            .flex()
            .flex_1()
            .flex_col()
            .overflow_scroll()
            .items_center()
            .py_4()
            .px_3()
            .gap_4();

        if self.is_retained_stale {
            let error_summary = self
                .last_error_summary
                .as_deref()
                .unwrap_or("Compilation error in source");

            container = container.child(
                div()
                    .flex()
                    .flex_col()
                    .max_w(px(520.0))
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .bg(theme::color(theme::BG_BAR))
                    .border_1()
                    .border_color(theme::color(theme::ACCENT_RED))
                    .text_xs()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(theme::color(theme::ACCENT_RED))
                                    .child("● Build failed"),
                            )
                            .child(
                                div()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child("— retaining last successful output"),
                            ),
                    )
                    .child(
                        div()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .text_xs()
                            .child(error_summary.to_string()),
                    ),
            );
        }

        if self.pages.is_empty() {
            container = container.child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .text_color(theme::color(theme::TEXT_MUTED))
                    .child("Rendering preview..."),
            );
        } else {
            for page in &self.pages {
                let page_w = (page.width as f32) * 0.75 * self.scale;
                let page_h = (page.height as f32) * 0.75 * self.scale;

                let page_card = div()
                    .flex()
                    .flex_none()
                    .w(px(page_w))
                    .h(px(page_h))
                    .bg(gpui::white())
                    .border_1()
                    .border_color(theme::color(theme::BORDER))
                    .rounded_xs()
                    .shadow_lg()
                    .overflow_hidden()
                    .child(img(page.image_path.clone()).size_full());

                container = container.child(page_card);
            }
        }

        container
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_view_initial_state() {
        let view = PreviewView::new();
        assert_eq!(view.scale, 1.0);
        assert!(view.pages.is_empty());
        assert!(!view.is_retained_stale);
    }
}
