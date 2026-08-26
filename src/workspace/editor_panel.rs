use gpui::{Context, CursorStyle, IntoElement, ParentElement, Role, Styled, div, prelude::*, px};
use std::path::Path;

use super::{ActiveViewKind, ResizingPanel, Workspace};
use crate::project::tree::FileKind;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

impl Workspace {
    pub fn render_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.show_welcome {
            return div()
                .flex()
                .flex_1()
                .min_h_0()
                .child(self.render_welcome(cx));
        }

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
            .w(px(3.0))
            .h_full()
            .bg(theme::color(theme::BG_BAR))
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|style| style.bg(theme::color(theme::ACCENT_BLUE)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.begin_panel_resize(panel, cx)),
            )
    }

    pub fn render_editor_and_diagnostics(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut area = div()
            .flex()
            .flex_1()
            .flex_col()
            .min_w_0()
            .bg(theme::color(theme::BG));

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
                .group("document-tab")
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
                        .id(format!("modified-tab-{idx}"))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(px(14.0))
                        .h(px(14.0))
                        .child(
                            div()
                                .w(px(6.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(theme::color(theme::TEXT_MUTED))
                                .group_hover("document-tab", |style| style.invisible()),
                        )
                        .child(
                            div()
                                .id(format!("close-dirty-tab-{idx}"))
                                .absolute()
                                .inset_0()
                                .invisible()
                                .group_hover("document-tab", |style| style.visible())
                                .flex()
                                .items_center()
                                .justify_center()
                                .role(Role::Button)
                                .aria_label("Close modified document")
                                .text_color(theme::color(theme::TEXT_MUTED))
                                .hover(|style| style.text_color(theme::color(theme::TEXT)))
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.close_tab(idx, cx);
                                    }),
                                )
                                .child(div().w(px(13.0)).h(px(13.0)).child(icon(Icon::Close))),
                        )
                } else {
                    div()
                        .id(format!("close-tab-{idx}"))
                        .w(px(14.0))
                        .text_center()
                        .role(Role::Button)
                        .aria_label("Close document")
                        .text_color(theme::color(theme::TEXT_MUTED))
                        .hover(|style| style.text_color(theme::color(theme::TEXT)))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.close_tab(idx, cx);
                            }),
                        )
                        .child(div().w(px(13.0)).h(px(13.0)).child(icon(Icon::Close)))
                });

            tab_bar = tab_bar.child(tab);
        }

        area = area.child(tab_bar);

        if self.active_view_kind == ActiveViewKind::Canvas {
            area = area.child(self.canvas.clone());
        } else {
            let mut editor_layer = div()
                .relative()
                .flex()
                .flex_1()
                .min_h_0()
                .child(self.editor.clone());

            if self.find_bar_open {
                editor_layer = editor_layer.child(self.render_find_bar(cx));
            }

            if self.completion_open && !self.completions.is_empty() {
                let (anchor_x, anchor_y) = self.editor.read(cx).completion_anchor();
                let mut comp_list = div()
                    .id("completion-popup")
                    .role(Role::ListBox)
                    .aria_label("Completions")
                    .absolute()
                    .left(px(anchor_x))
                    .top(px(anchor_y))
                    .flex()
                    .flex_col()
                    .w(px(320.0))
                    .max_h(px(220.0))
                    .rounded_md()
                    .bg(theme::color(theme::BG_SURFACE))
                    .border_1()
                    .border_color(theme::color(theme::BORDER))
                    .shadow_lg()
                    .overflow_scroll();

                for (index, item) in self.completions.iter().enumerate() {
                    let item_clone = item.clone();
                    let row = div()
                        .id(format!("comp-row-{}", item.label))
                        .role(Role::ListBoxOption)
                        .aria_label(format!("{}: {}", item.label, item.detail))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .border_l_2()
                        .border_color(theme::color(if index == self.completion_selected {
                            theme::ACCENT_BLUE
                        } else {
                            theme::BG_SURFACE
                        }))
                        .bg(if index == self.completion_selected {
                            theme::color(theme::HOVER_BG)
                        } else {
                            theme::color(theme::BG_SURFACE)
                        })
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
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(30.0))
                                        .flex_none()
                                        .text_color(theme::color(theme::TEXT_MUTED))
                                        .child(item.kind.label()),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(item.label.clone()),
                                ),
                        )
                        .child(
                            div()
                                .max_w(px(120.0))
                                .truncate()
                                .text_color(theme::color(theme::TEXT_MUTED))
                                .child(item.detail.clone()),
                        );

                    comp_list = comp_list.child(row);
                }

                editor_layer = editor_layer.child(comp_list);
            }

            area = area.child(editor_layer);

            if self.diagnostics_drawer_open && !self.latest_diagnostics.is_empty() {
                area = area.child(self.render_diagnostics_drawer(cx));
            }
        }

        area
    }

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
