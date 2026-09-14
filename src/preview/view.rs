use gpui::{
    Context, ImgResourceLoader, IntoElement, Render, Resource, Window, div, img, prelude::*, px,
};

use super::renderer::RenderedPage;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

pub struct PreviewView {
    pages: Vec<RenderedPage>,
    scale: f32,
    is_retained_stale: bool,
    is_rendering: bool,
    last_error_summary: Option<String>,
    render_notice: Option<String>,
}

impl Default for PreviewView {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewView {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            scale: 1.0,
            is_retained_stale: false,
            is_rendering: false,
            last_error_summary: None,
            render_notice: None,
        }
    }

    pub fn set_rendered_pages(
        &mut self,
        pages: Vec<RenderedPage>,
        notice: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.release_page_assets(cx);
        self.adopt_rendered_pages(pages, notice);
        cx.notify();
    }

    pub fn set_compile_failed(&mut self, error_msg: Option<String>, cx: &mut Context<Self>) {
        self.retain_stale(error_msg);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.release_page_assets(cx);
        self.reset_state();
        cx.notify();
    }

    pub fn set_rendering(&mut self, cx: &mut Context<Self>) {
        self.begin_rendering();
        cx.notify();
    }

    /// A fresh render replaces everything: stale markers and the previous
    /// error summary are gone once valid pages exist.
    fn adopt_rendered_pages(&mut self, pages: Vec<RenderedPage>, notice: Option<String>) {
        self.pages = pages;
        self.is_retained_stale = false;
        self.is_rendering = false;
        self.last_error_summary = None;
        self.render_notice = notice;
    }

    /// A failed compile keeps the last valid pages on screen (the
    /// preserve-last-valid-preview invariant) and flags them as stale.
    fn retain_stale(&mut self, error_msg: Option<String>) {
        self.is_retained_stale = true;
        self.is_rendering = false;
        self.last_error_summary = error_msg;
    }

    fn begin_rendering(&mut self) {
        self.is_rendering = true;
    }

    fn reset_state(&mut self) {
        self.pages.clear();
        self.is_retained_stale = false;
        self.is_rendering = false;
        self.last_error_summary = None;
        self.render_notice = None;
    }

    /// GPUI retains every decoded image in its asset cache for the life of the
    /// app, and each compile writes new page paths. Drop the assets of pages
    /// this preview is replacing, or every render stays in memory forever.
    fn release_page_assets(&self, cx: &mut Context<Self>) {
        for page in &self.pages {
            cx.remove_asset::<ImgResourceLoader>(&Resource::Path(page.image_path.clone().into()));
        }
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.zoom_by(0.1);
        cx.notify();
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.zoom_by(-0.1);
        cx.notify();
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.scale = 1.0;
        cx.notify();
    }

    fn zoom_by(&mut self, delta: f32) {
        self.scale = (self.scale + delta).clamp(0.4, 3.0);
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
            .bg(theme::BG_SURFACE)
            .child(self.render_toolbar(page_count, cx))
            .child(self.render_content())
    }
}

impl PreviewView {
    fn render_toolbar(&self, page_count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let page_label = match page_count {
            0 => "No preview".to_string(),
            1 => "1 page".to_string(),
            n => format!("{n} pages"),
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(px(32.0))
            .px_3()
            .bg(theme::BG_BAR)
            .border_b_1()
            .border_color(theme::BORDER)
            .text_xs()
            .text_color(theme::TEXT_MUTED)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("Preview"),
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
                            .text_color(theme::TEXT_MUTED)
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::HOVER_BG))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.zoom_out(cx)),
                            )
                            .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Minus))),
                    )
                    .child(
                        div()
                            .id("zoom-reset-btn")
                            .px_2()
                            .py_0p5()
                            .rounded_xs()
                            .text_color(theme::TEXT_MUTED)
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::HOVER_BG))
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
                            .text_color(theme::TEXT_MUTED)
                            .cursor_pointer()
                            .hover(|style| style.bg(theme::HOVER_BG))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| this.zoom_in(cx)),
                            )
                            .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Plus))),
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
                .and_then(|message| message.lines().find(|line| !line.trim().is_empty()))
                .unwrap_or("Open Problems for compile details.");
            let has_previous_preview = !self.pages.is_empty();

            container = container.child(
                div()
                    .flex()
                    .w_full()
                    .max_w(px(520.0))
                    .items_start()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .bg(theme::BG_BAR)
                    .border_l_2()
                    .border_color(theme::ACCENT_RED)
                    .text_xs()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::TEXT)
                                    .child(if has_previous_preview {
                                        "Preview out of date"
                                    } else {
                                        "Preview unavailable"
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(error_summary.to_string()),
                            )
                            .when(has_previous_preview, |message| {
                                message.child(
                                    div()
                                        .text_color(theme::TEXT_MUTED)
                                        .child("Showing the last successful compile."),
                                )
                            }),
                    ),
            );
        }

        if let Some(notice) = self
            .render_notice
            .as_deref()
            .filter(|_| !self.pages.is_empty())
        {
            container = container.child(
                div()
                    .flex()
                    .w_full()
                    .max_w(px(520.0))
                    .items_start()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .rounded_sm()
                    .bg(theme::BG_BAR)
                    .border_l_2()
                    .border_color(theme::TEXT_MUTED)
                    .text_xs()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(theme::TEXT_MUTED)
                            .child(notice.to_string()),
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
                    .text_color(theme::TEXT_MUTED)
                    .child(if self.is_rendering {
                        "Rendering preview..."
                    } else if self.is_retained_stale {
                        "Resolve the compile errors to create a preview."
                    } else {
                        "No preview"
                    }),
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
                    .border_color(theme::BORDER)
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
    use std::path::PathBuf;

    fn sample_page(index: usize) -> RenderedPage {
        RenderedPage {
            page_index: index,
            width: 100,
            height: 140,
            image_path: PathBuf::from(format!("/tmp/graf-test/page-{index}.png")),
        }
    }

    #[test]
    fn test_preview_view_initial_state() {
        let view = PreviewView::new();
        assert_eq!(view.scale, 1.0);
        assert!(view.pages.is_empty());
        assert!(!view.is_retained_stale);
    }

    #[test]
    fn compile_failure_retains_the_last_valid_preview() {
        let mut view = PreviewView::new();
        view.adopt_rendered_pages(vec![sample_page(0)], None);

        view.retain_stale(Some("undefined control sequence".to_string()));

        // The preserve-last-valid-preview invariant: pages stay on screen,
        // flagged stale, with the error summary for the banner.
        assert!(view.is_retained_stale);
        assert_eq!(view.pages.len(), 1);
        assert!(!view.is_rendering);
        assert_eq!(
            view.last_error_summary.as_deref(),
            Some("undefined control sequence")
        );
    }

    #[test]
    fn a_fresh_render_clears_stale_state_and_errors() {
        let mut view = PreviewView::new();
        view.retain_stale(Some("boom".to_string()));
        view.begin_rendering();
        assert!(view.is_rendering);
        assert!(view.is_retained_stale);

        view.adopt_rendered_pages(vec![sample_page(0)], Some("sips fallback".to_string()));

        assert!(!view.is_retained_stale);
        assert!(!view.is_rendering);
        assert!(view.last_error_summary.is_none());
        assert_eq!(view.render_notice.as_deref(), Some("sips fallback"));
    }

    #[test]
    fn rendering_flag_clears_on_either_outcome() {
        let mut view = PreviewView::new();
        view.begin_rendering();

        view.retain_stale(None);
        assert!(!view.is_rendering);

        view.begin_rendering();
        view.adopt_rendered_pages(vec![sample_page(0)], None);
        assert!(!view.is_rendering);
    }

    #[test]
    fn reset_state_clears_everything() {
        let mut view = PreviewView::new();
        view.adopt_rendered_pages(vec![sample_page(0)], Some("notice".to_string()));
        view.retain_stale(Some("err".to_string()));

        view.reset_state();

        assert!(view.pages.is_empty());
        assert!(!view.is_retained_stale);
        assert!(!view.is_rendering);
        assert!(view.last_error_summary.is_none());
        assert!(view.render_notice.is_none());
    }

    #[test]
    fn zoom_stays_within_bounds() {
        let mut view = PreviewView::new();
        for _ in 0..40 {
            view.zoom_by(0.1);
        }
        assert!((view.scale - 3.0).abs() < 1e-4);
        for _ in 0..40 {
            view.zoom_by(-0.1);
        }
        assert!((view.scale - 0.4).abs() < 1e-4);
        view.scale = 1.0;
        assert_eq!(view.scale, 1.0);
    }
}
