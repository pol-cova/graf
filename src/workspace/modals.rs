use gpui::{
    ClipboardItem, Context, Focusable, IntoElement, ParentElement, Role, Styled, div, prelude::*,
    px,
};
use std::path::Path;

use super::commands::all_commands;
use super::{ActiveModal, SettingsTab, Workspace};
use crate::ai::operations::AiOperationKind;
use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

impl Workspace {
    pub fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let is_quick_open = matches!(self.active_modal, ActiveModal::QuickOpen(_));
        let is_cmd_palette = matches!(self.active_modal, ActiveModal::CommandPalette(_));
        let is_ai_assist = matches!(self.active_modal, ActiveModal::AiAssist(_));
        let is_diff_review = matches!(self.active_modal, ActiveModal::DiffReview(_));
        let is_confirm_close = matches!(self.active_modal, ActiveModal::ConfirmClose(_));
        let is_settings = matches!(self.active_modal, ActiveModal::Settings(_));
        let is_about = matches!(self.active_modal, ActiveModal::About);

        if !is_quick_open
            && !is_cmd_palette
            && !is_ai_assist
            && !is_diff_review
            && !is_confirm_close
            && !is_settings
            && !is_about
        {
            return None;
        }

        let title = if is_quick_open {
            "Open file"
        } else if is_cmd_palette {
            "Commands"
        } else if is_ai_assist {
            "Writing assistant"
        } else if is_settings {
            "Settings"
        } else if is_about {
            "About graf"
        } else if is_confirm_close {
            "Unsaved changes"
        } else {
            "Review changes"
        };

        let mut modal_content = div()
            .flex()
            .flex_col()
            .w(px(580.0))
            .max_h(px(460.0))
            .bg(theme::color(theme::BG_SURFACE))
            .rounded_md()
            .border_1()
            .border_color(theme::color(theme::BORDER))
            .shadow_lg()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::color(theme::BORDER))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::color(theme::TEXT))
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child("Esc"),
                            )
                            .child(
                                div()
                                    .id("close-modal")
                                    .px_1()
                                    .rounded_xs()
                                    .text_sm()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.close_modal(cx)),
                                    )
                                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Close))),
                            ),
                    ),
            );

        if is_quick_open || is_cmd_palette {
            let input = div()
                .flex()
                .items_center()
                .h(px(34.0))
                .mx_3()
                .my_2()
                .px_2()
                .rounded_xs()
                .bg(theme::color(theme::BG))
                .border_1()
                .border_color(theme::color(theme::BORDER))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .h_full()
                        .flex_1()
                        .min_w_0()
                        .child(self.prompt_editor.clone()),
                );
            modal_content = modal_content.child(input);
        }

        if is_settings {
            if let ActiveModal::Settings(tab) = self.active_modal {
                let tabs = [
                    (SettingsTab::Editor, "Editor"),
                    (SettingsTab::Build, "Build"),
                ];

                let tab_header = div()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme::color(theme::BORDER))
                    .bg(theme::color(theme::BG_BAR))
                    .children(tabs.into_iter().map(|(t, label)| {
                        let is_active = t == tab;
                        div()
                            .id(format!("settings-tab-{}", label))
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .py_1p5()
                            .text_xs()
                            .font_weight(if is_active {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .bg(if is_active {
                                theme::color(theme::BG_SURFACE)
                            } else {
                                theme::color(theme::BG_BAR)
                            })
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
                                    this.open_settings(t, cx);
                                }),
                            )
                            .child(label)
                    }));

                let mut settings_body = div()
                    .id("settings-modal-body")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .overflow_scroll();

                match tab {
                    SettingsTab::Build => {
                        let setting_button = || {
                            div()
                                .px_2()
                                .py_1()
                                .rounded_xs()
                                .bg(theme::color(theme::BG_BAR))
                                .border_1()
                                .border_color(theme::color(theme::BORDER))
                                .text_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                        };

                        settings_body = settings_body
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(div().text_xs().child("Compile while editing"))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme::color(theme::TEXT_MUTED))
                                                    .child("Rebuild the active document after changes."),
                                            ),
                                    )
                                    .child(
                                        setting_button()
                                            .id("auto-compile-setting")
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.toggle_auto_compile_setting(cx);
                                                }),
                                            )
                                            .child(if self.settings.editor.auto_compile {
                                                "On"
                                            } else {
                                                "Off"
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(div().text_xs().child("Compile delay"))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme::color(theme::TEXT_MUTED))
                                                    .child("Wait before rebuilding after a keystroke."),
                                            ),
                                    )
                                    .child(
                                        setting_button()
                                            .id("compile-delay-setting")
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.cycle_compile_debounce(cx);
                                                }),
                                            )
                                            .child(format!(
                                                "{} ms",
                                                self.settings.editor.compile_debounce_ms
                                            )),
                                    ),
                            );
                    }
                    SettingsTab::Editor => {
                        let setting_button = || {
                            div()
                                .px_2()
                                .py_1()
                                .rounded_xs()
                                .bg(theme::color(theme::BG_BAR))
                                .border_1()
                                .border_color(theme::color(theme::BORDER))
                                .text_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                        };

                        settings_body = settings_body
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Font size"))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                setting_button()
                                                    .id("font-size-down")
                                                    .on_mouse_down(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            this.adjust_editor_font_size(-1.0, cx);
                                                        }),
                                                    )
                                                    .child(
                                                        div()
                                                            .w(px(14.0))
                                                            .h(px(14.0))
                                                            .child(icon(Icon::Minus)),
                                                    ),
                                            )
                                            .child(div().w(px(64.0)).text_center().text_xs().child(
                                                format!("{:.0} px", self.settings.editor.font_size),
                                            ))
                                            .child(
                                                setting_button()
                                                    .id("font-size-up")
                                                    .on_mouse_down(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            this.adjust_editor_font_size(1.0, cx);
                                                        }),
                                                    )
                                                    .child(
                                                        div()
                                                            .w(px(14.0))
                                                            .h(px(14.0))
                                                            .child(icon(Icon::Plus)),
                                                    ),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Tab width"))
                                    .child(
                                        setting_button()
                                            .id("tab-size-setting")
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.cycle_tab_size(cx);
                                                }),
                                            )
                                            .child(format!(
                                                "{} spaces",
                                                self.settings.editor.tab_size
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Line numbers"))
                                    .child(
                                        setting_button()
                                            .id("line-numbers-setting")
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(|this, _, _, cx| {
                                                    this.toggle_line_numbers_setting(cx);
                                                }),
                                            )
                                            .child(if self.settings.editor.line_numbers {
                                                "On"
                                            } else {
                                                "Off"
                                            }),
                                    ),
                            );
                    }
                }

                modal_content = modal_content.child(tab_header).child(settings_body);
            }
        } else if is_about {
            let version = env!("CARGO_PKG_VERSION");
            modal_content = modal_content.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .px_6()
                    .py_5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(px(56.0))
                            .h(px(56.0))
                            .rounded_md()
                            .bg(theme::color(theme::ACCENT_BLUE))
                            .text_size(px(28.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(gpui::white())
                            .child("g"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::color(theme::TEXT))
                                    .child("graf"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child(format!("Version {version}")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child("A native workspace for technical writing"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .gap_2()
                            .child(
                                div()
                                    .id("about-copy-details")
                                    .role(Role::Button)
                                    .aria_label("Copy version details")
                                    .flex_1()
                                    .py_1p5()
                                    .rounded_xs()
                                    .border_1()
                                    .border_color(theme::color(theme::BORDER))
                                    .text_center()
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |_, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                format!(
                                                    "graf {version}\n{} {}",
                                                    std::env::consts::OS,
                                                    std::env::consts::ARCH
                                                ),
                                            ));
                                        }),
                                    )
                                    .child("Copy details"),
                            )
                            .child(
                                div()
                                    .id("about-close")
                                    .role(Role::Button)
                                    .aria_label("Close About")
                                    .flex_1()
                                    .py_1p5()
                                    .rounded_xs()
                                    .bg(theme::color(theme::ACCENT_BLUE))
                                    .text_center()
                                    .text_xs()
                                    .text_color(gpui::white())
                                    .cursor_pointer()
                                    .hover(|style| style.opacity(0.9))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.close_modal(cx)),
                                    )
                                    .child("OK"),
                            ),
                    ),
            );
        } else if is_confirm_close {
            if let ActiveModal::ConfirmClose(index) = self.active_modal {
                let title = self
                    .documents
                    .get(index)
                    .map(|document| document.title().to_string())
                    .unwrap_or_else(|| "document".to_string());
                modal_content = modal_content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .p_4()
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme::color(theme::TEXT))
                                .child(format!("Save changes to {title}?")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap_2()
                                .child(
                                    div()
                                        .id("cancel-close")
                                        .px_3()
                                        .py_1()
                                        .rounded_xs()
                                        .text_xs()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.active_modal = ActiveModal::None;
                                                cx.notify();
                                            }),
                                        )
                                        .child("Cancel"),
                                )
                                .child(
                                    div()
                                        .id("discard-close")
                                        .px_3()
                                        .py_1()
                                        .rounded_xs()
                                        .text_xs()
                                        .text_color(theme::color(theme::ACCENT_RED))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _, _, cx| {
                                                this.active_modal = ActiveModal::None;
                                                this.force_close_tab(index, cx);
                                            }),
                                        )
                                        .child("Discard"),
                                )
                                .child(
                                    div()
                                        .id("save-before-close")
                                        .px_3()
                                        .py_1()
                                        .rounded_xs()
                                        .bg(theme::color(theme::ACCENT_BLUE))
                                        .text_xs()
                                        .text_color(theme::color(theme::BG))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(move |this, _, _, cx| {
                                                this.active_modal = ActiveModal::None;
                                                if index != this.active_doc_idx {
                                                    this.switch_tab(index, cx);
                                                }
                                                this.save_active_document(cx);
                                            }),
                                        )
                                        .child("Save"),
                                ),
                        ),
                );
            }
        } else if is_diff_review {
            if let ActiveModal::DiffReview(review) = &self.active_modal {
                let metrics = review.diff_summary();
                let original = review.original.clone();
                let replacement = review.replacement.clone();

                let diff_body = div()
                    .id("diff-modal-body")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .overflow_scroll()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::color(theme::ACCENT_BLUE))
                                    .child(review.title.clone()),
                            )
                            .child(
                                div()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child(metrics),
                            ),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::ACCENT_RED))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child("Original")
                            .child(div().mt_1().child(original)),
                    )
                    .child(
                        div()
                            .p_2()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::ACCENT_GREEN))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT))
                            .child("Proposed")
                            .child(div().mt_1().child(replacement)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .mt_2()
                            .child(
                                div()
                                    .id("diff-reject-btn")
                                    .px_3()
                                    .py_1()
                                    .rounded_xs()
                                    .bg(theme::color(theme::BG_SURFACE))
                                    .border_1()
                                    .border_color(theme::color(theme::BORDER))
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.active_modal = ActiveModal::None;
                                            cx.notify();
                                        }),
                                    )
                                    .child("Reject"),
                            )
                            .child(
                                div()
                                    .id("diff-accept-btn")
                                    .px_3()
                                    .py_1()
                                    .rounded_xs()
                                    .bg(theme::color(theme::ACCENT_BLUE))
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::color(theme::BG))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.accept_diff_review(cx);
                                        }),
                                    )
                                    .child("Apply changes"),
                            ),
                    );

                modal_content = modal_content.child(diff_body);
            }
        } else if is_ai_assist {
            let ai_ops = [
                (
                    AiOperationKind::RewriteAcademic,
                    "Rewrite buffer with formal tone & mathematical rigor",
                ),
                (
                    AiOperationKind::Shorten,
                    "Tighten prose while retaining formulas and key claims",
                ),
                (
                    AiOperationKind::Explain,
                    "Generate clear walkthrough of selected technical concepts",
                ),
                (
                    AiOperationKind::FixDiagnostic {
                        message: "Auto-detected diagnostic".to_string(),
                        line: None,
                    },
                    "Analyze errors and apply automated syntax patch",
                ),
                (
                    AiOperationKind::GenerateDiagram {
                        prompt: "System Architecture Pipeline".to_string(),
                    },
                    "Create structured vector scene from description",
                ),
            ];

            let mut list = div()
                .id("ai-assist-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();
            for (op, desc) in ai_ops {
                let name = op.label();
                let row = div()
                    .id(format!("ai-op-{}", name))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.run_ai_operation(op.clone(), cx);
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme::color(theme::TEXT))
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child(desc),
                            ),
                    )
                    .child(
                        div()
                            .text_color(theme::color(theme::ACCENT_BLUE))
                            .child("Run"),
                    );
                list = list.child(row);
            }
            modal_content = modal_content.child(list);
        } else if is_cmd_palette {
            let filter = self.prompt_editor.read(cx).text().to_lowercase();

            let commands = all_commands();
            let mut list = div()
                .id("cmd-palette-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();

            for item in commands {
                if !filter.is_empty()
                    && !item.title.to_lowercase().contains(&filter)
                    && !item.category.to_lowercase().contains(&filter)
                {
                    continue;
                }

                let id = item.id;
                let name = item.title;
                let shortcut = item.shortcut;
                let category = item.category;

                let row = div()
                    .id(format!("cmd-row-{}", id))
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
                        cx.listener(move |this, _, window, cx| {
                            this.active_modal = ActiveModal::None;
                            this.dispatch_command_action(id, cx);
                            window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_xs()
                                    .bg(theme::color(theme::BG_BAR))
                                    .border_1()
                                    .border_color(theme::color(theme::BORDER))
                                    .text_xs()
                                    .text_color(theme::color(theme::TEXT_MUTED))
                                    .child(category),
                            )
                            .child(name),
                    )
                    .child(
                        div()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child(shortcut),
                    );
                list = list.child(row);
            }
            modal_content = modal_content.child(list);
        } else {
            let filter = self.prompt_editor.read(cx).text().to_lowercase();

            let mut list = div()
                .id("quick-open-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();

            for path in self.project_tree.file_paths() {
                let title = path
                    .strip_prefix(self.project_tree.root_path())
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                if !filter.is_empty() && !title.to_lowercase().contains(&filter) {
                    continue;
                }

                let row_id = title.clone();
                let row = div()
                    .id(format!("quick-open-{row_id}"))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT))
                    .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.active_modal = ActiveModal::None;
                            this.open_file(path.clone(), cx);
                            window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .w(px(32.0))
                            .text_color(theme::color(theme::ACCENT_BLUE))
                            .child(
                                crate::project::tree::FileKind::from_path(Path::new(&title))
                                    .label(),
                            ),
                    )
                    .child(title);
                list = list.child(row);
            }
            modal_content = modal_content.child(list);
        }

        Some(
            div()
                .id("modal-backdrop")
                .absolute()
                .size_full()
                .flex()
                .items_start()
                .justify_center()
                .pt(px(60.0))
                .bg(gpui::rgba(0x00000080))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.active_modal = ActiveModal::None;
                        cx.notify();
                    }),
                )
                .child(modal_content),
        )
    }
}
