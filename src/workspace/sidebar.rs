use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::{SidebarTab, Workspace};
use crate::project::outline::{OutlineItem, parse_latex_outline};
use crate::project::tree::FileNode;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

impl Workspace {
    pub fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_files = self.sidebar_tab == SidebarTab::Files;

        let mut sidebar = div()
            .flex()
            .flex_none()
            .flex_col()
            .w(px(self.sidebar_width))
            .bg(theme::color(theme::BG_SURFACE))
            .border_r_1()
            .border_color(theme::color(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(32.0))
                    .border_b_1()
                    .border_color(theme::color(theme::BORDER))
                    .child(
                        div()
                            .id("sidebar-tab-files")
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .h_full()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .bg(if is_files {
                                theme::color(theme::BG_SURFACE)
                            } else {
                                theme::color(theme::BG_BAR)
                            })
                            .text_color(if is_files {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.sidebar_tab = SidebarTab::Files;
                                    cx.notify();
                                }),
                            )
                            .child("Project"),
                    )
                    .child(
                        div()
                            .id("sidebar-tab-outline")
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .h_full()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .bg(if !is_files {
                                theme::color(theme::BG_SURFACE)
                            } else {
                                theme::color(theme::BG_BAR)
                            })
                            .text_color(if !is_files {
                                theme::color(theme::TEXT)
                            } else {
                                theme::color(theme::TEXT_MUTED)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.sidebar_tab = SidebarTab::Outline;
                                    cx.notify();
                                }),
                            )
                            .child("Outline"),
                    ),
            );

        if is_files {
            let mut file_list = div()
                .id("project-file-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();
            let root_node = self.project_tree.root_node();
            if let FileNode::Directory { children, .. } = root_node {
                for child in children {
                    file_list = file_list.child(self.render_file_node(child, 0, cx));
                }
            }
            sidebar = sidebar.child(file_list);
        } else {
            sidebar = sidebar.child(self.render_outline_list(cx));
        }

        sidebar
    }

    pub fn render_outline_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let text = self.editor.read(cx).text().to_string();
        let items: Vec<OutlineItem> = parse_latex_outline(&text);

        let mut list = div()
            .id("outline-list")
            .flex()
            .flex_col()
            .py_1()
            .overflow_scroll();

        if items.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT_MUTED))
                    .child("No headings"),
            );
        } else {
            for item in items {
                let line_num = item.line_number;
                let indent = px(12.0 * item.level as f32 + 8.0);

                let row = div()
                    .id(format!("outline-row-{}", line_num))
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .pl(indent)
                    .pr_3()
                    .py_1()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT))
                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.jump_to_line(line_num, cx);
                        }),
                    )
                    .child(
                        div()
                            .text_color(theme::color(theme::ACCENT_BLUE))
                            .child(item.display_prefix()),
                    )
                    .child(item.title);

                list = list.child(row);
            }
        }

        list
    }

    pub fn render_file_node(
        &self,
        node: &FileNode,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let indent = px(12.0 * depth as f32 + 12.0);

        match node {
            FileNode::Directory {
                path,
                name,
                children,
                is_expanded,
            } => {
                let directory_path = path.clone();
                let mut dir_div = div().flex().flex_col().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .pl(indent)
                        .pr_3()
                        .py_1()
                        .text_xs()
                        .text_color(theme::color(theme::TEXT_MUTED))
                        .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                        .cursor_pointer()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.project_tree.toggle_directory(&directory_path);
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(theme::color(theme::TEXT_MUTED))
                                .child(icon(if *is_expanded {
                                    Icon::ChevronDown
                                } else {
                                    Icon::ChevronRight
                                })),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(name.clone())),
                );

                if *is_expanded {
                    for child in children {
                        dir_div = dir_div.child(self.render_file_node(child, depth + 1, cx));
                    }
                }
                dir_div
            }
            FileNode::File { path, name, kind } => {
                let path_buf = path.clone();
                let is_active = self
                    .documents
                    .get(self.active_doc_idx)
                    .and_then(|d| d.path())
                    == Some(path);

                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl(indent)
                    .pr_3()
                    .py_1()
                    .bg(if is_active {
                        theme::color(theme::TAB_ACTIVE)
                    } else {
                        theme::color(theme::BG_SURFACE)
                    })
                    .text_xs()
                    .text_color(if is_active {
                        theme::color(theme::TEXT)
                    } else {
                        theme::color(theme::TEXT_MUTED)
                    })
                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.open_file(path_buf.clone(), cx);
                        }),
                    )
                    .child(
                        div()
                            .w(px(28.0))
                            .text_color(theme::color(theme::ACCENT_BLUE))
                            .child(kind.label()),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(name.clone()))
            }
        }
    }
}
