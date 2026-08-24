//! Central editor viewport and document tabs component.

use gpui::{Context, CursorStyle, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::path::Path;

use super::{ActiveViewKind, ResizingPanel, Workspace};
use crate::project::tree::FileKind;
use crate::ui::theme;

impl Workspace {
    /// Three-column panel area: Files/Outline Sidebar | Center Canvas / Editor | Preview.
    pub fn render_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut body = div().flex().flex_1().min_h_0();

        if self.sidebar_visible {
            body = body
                .child(self.render_sidebar(cx))
                .child(self.render_vertical_resize_handle(ResizingPanel::Sidebar, cx));
        }

        body = body.child(self.render_editor_and_diagnostics(cx));

        if self.preview_visible {
            body = body
                .child(self.render_vertical_resize_handle(ResizingPanel::Preview, cx))
                .child(self.render_preview());
        }

        body
    }

    fn render_vertical_resize_handle(
        &self,
        panel: ResizingPanel,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(match panel {
                ResizingPanel::Sidebar => "sidebar-resize-handle",
                ResizingPanel::Preview => "preview-resize-handle",
                ResizingPanel::Diagnostics => "diagnostics-vertical-resize-handle",
            })
            .flex_none()
            .w(px(5.0))
            .h_full()
            .bg(theme::color(theme::BORDER))
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|style| style.bg(theme::color(theme::ACCENT_BLUE)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.begin_panel_resize(panel, cx)),
            )
    }

    /// Center area with multi-document tab bar, Canvas view or Text editor, and problems drawer.
    pub fn render_editor_and_diagnostics(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut area = div()
            .flex()
            .flex_1()
            .flex_col()
            .min_w_0()
            .bg(theme::color(theme::BG));

        // Tab bar
        let mut tab_bar = div()
            .flex()
            .flex_none()
            .items_center()
            .h(px(34.0))
            .bg(theme::color(theme::BG_BAR))
            .border_b_1()
            .border_color(theme::color(theme::BORDER));

        for (idx, doc) in self.documents.iter().enumerate() {
            let is_active = idx == self.active_doc_idx;
            let is_dirty = doc.is_dirty();
            let title = doc.title().to_string();

            let tab = div()
                .id(format!("tab-{}", doc.id().0))
                .flex()
                .items_center()
                .gap_2()
                .h_full()
                .max_w(px(220.0))
                .min_w_0()
                .px_3()
                .bg(if is_active {
                    theme::color(theme::TAB_ACTIVE)
                } else {
                    theme::color(theme::BG_BAR)
                })
                .border_r_1()
                .border_t_1()
                .border_color(if is_active {
                    theme::color(theme::ACCENT_BLUE)
                } else {
                    theme::color(theme::BORDER)
                })
                .text_xs()
                .text_color(if is_active {
                    theme::color(theme::TEXT)
                } else {
                    theme::color(theme::TEXT_MUTED)
                })
                .cursor_pointer()
                .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.switch_tab(idx, cx);
                    }),
                )
                .child(
                    div()
                        .text_color(theme::color(theme::ACCENT_BLUE))
                        .child(FileKind::from_path(Path::new(&title)).label()),
                )
                .child(div().flex_1().min_w_0().truncate().child(title))
                .child(if is_dirty {
                    div()
                        .text_color(theme::color(theme::ACCENT_ORANGE))
                        .child("●")
                } else {
                    div()
                        .text_color(theme::color(theme::TEXT_MUTED))
                        .hover(|s| s.text_color(theme::color(theme::TEXT)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.close_tab(idx, cx);
                            }),
                        )
                        .child("×")
                });

            tab_bar = tab_bar.child(tab);
        }

        area = area.child(tab_bar);

        // Center Viewport: Vector Canvas vs Text Editor
        if self.active_view_kind == ActiveViewKind::Canvas {
            area = area.child(self.canvas.clone());
        } else {
            // Find and Replace Bar (if open)
            if self.find_bar_open {
                area = area.child(self.render_find_bar(cx));
            }

            // Editor canvas
            area = area.child(div().flex().flex_1().min_h_0().child(self.editor.clone()));

            // Autocompletion Candidate List (if active)
            if self.completion_open && !self.completions.is_empty() {
                let mut comp_list = div()
                    .id("completion-popup")
                    .flex()
                    .flex_col()
                    .max_h(px(180.0))
                    .bg(theme::color(theme::BG_SURFACE))
                    .border_t_1()
                    .border_color(theme::color(theme::BORDER))
                    .overflow_scroll();

                for item in &self.completions {
                    let item_clone = item.clone();
                    let row = div()
                        .id(format!("comp-row-{}", item.label))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1p5()
                        .text_xs()
                        .text_color(theme::color(theme::TEXT))
                        .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                        .cursor_pointer()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.apply_completion(&item_clone, cx);
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(32.0))
                                        .text_color(theme::color(theme::ACCENT_BLUE))
                                        .child(item.kind.label()),
                                )
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(item.label.clone()),
                                ),
                        )
                        .child(
                            div()
                                .text_color(theme::color(theme::TEXT_MUTED))
                                .child(item.detail.clone()),
                        );

                    comp_list = comp_list.child(row);
                }

                area = area.child(comp_list);
            }

            // Diagnostics Drawer (if open)
            if self.diagnostics_drawer_open && !self.latest_diagnostics.is_empty() {
                area = area.child(self.render_diagnostics_drawer(cx));
            }
        }

        area
    }

    /// Right preview pane showing rendered PDF document pages.
    pub fn render_preview(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .flex_col()
            .w(px(self.preview_width))
            .min_w(px(320.0))
            .bg(theme::color(theme::BG_SURFACE))
            .border_l_1()
            .border_color(theme::color(theme::BORDER))
            .child(self.preview.clone())
    }
}
