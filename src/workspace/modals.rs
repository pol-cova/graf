use super::commands::filter_commands;
use super::{ActiveModal, SettingsTab, Workspace, state};
use gpui::{
    ClipboardItem, Context, Focusable, IntoElement, ParentElement, Role, Styled, div, prelude::*,
    px,
};

use crate::ui::icons::{Icon, icon};
use crate::ui::theme;

impl Workspace {
    pub fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        // One dispatch point decides which modal renders and its title; no
        // flag chorus to extend by hand when a modal kind is added, and no
        // dead else branch.
        let title = match &self.active_modal {
            ActiveModal::QuickOpen => "Open file",
            ActiveModal::CommandPalette => "Commands",
            ActiveModal::Settings(_) => "Settings",
            ActiveModal::About => "About graf",
            ActiveModal::ConfirmClose(_) => "Unsaved changes",
            ActiveModal::RestoreRecovery => "Restore unsaved work",
            ActiveModal::TemplatePicker(request) if request.for_new_project => "New project",
            ActiveModal::TemplatePicker(_) => "New from template",
            ActiveModal::None => return None,
        };
        let is_quick_open = matches!(self.active_modal, ActiveModal::QuickOpen);
        let is_cmd_palette = matches!(self.active_modal, ActiveModal::CommandPalette);
        let is_confirm_close = matches!(self.active_modal, ActiveModal::ConfirmClose(_));
        let is_restore_recovery = matches!(self.active_modal, ActiveModal::RestoreRecovery);
        let is_settings = matches!(self.active_modal, ActiveModal::Settings(_));
        let is_about = matches!(self.active_modal, ActiveModal::About);
        let is_template_picker = matches!(self.active_modal, ActiveModal::TemplatePicker(_));

        let mut modal_content = div()
            .flex()
            .flex_col()
            .w(px(580.0))
            .max_h(px(460.0))
            .bg(theme::BG_SURFACE)
            .rounded_md()
            .border_1()
            .border_color(theme::BORDER)
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
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_xs().text_color(theme::TEXT_MUTED).child("Esc"))
                            .child(
                                div()
                                    .id("close-modal")
                                    .px_1()
                                    .rounded_xs()
                                    .text_sm()
                                    .text_color(theme::TEXT_MUTED)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::HOVER_BG))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.close_modal(cx)),
                                    )
                                    .child(div().w(px(14.0)).h(px(14.0)).child(icon(Icon::Close))),
                            ),
                    ),
            );

        if is_quick_open || is_cmd_palette || is_template_picker {
            let input = div()
                .flex()
                .items_center()
                .h(px(34.0))
                .mx_3()
                .my_2()
                .px_2()
                .rounded_xs()
                .bg(theme::BG)
                .border_1()
                .border_color(theme::BORDER)
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
                    .border_color(theme::BORDER)
                    .bg(theme::BG_BAR)
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
                                theme::BG_SURFACE
                            } else {
                                theme::BG_BAR
                            })
                            .text_color(if is_active {
                                theme::TEXT
                            } else {
                                theme::TEXT_MUTED
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::HOVER_BG))
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
                                .bg(theme::BG_BAR)
                                .border_1()
                                .border_color(theme::BORDER)
                                .text_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::HOVER_BG))
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
                                                    .text_color(theme::TEXT_MUTED)
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
                                                    .text_color(theme::TEXT_MUTED)
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
                                .bg(theme::BG_BAR)
                                .border_1()
                                .border_color(theme::BORDER)
                                .text_xs()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::HOVER_BG))
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
                            .bg(theme::ACCENT_BLUE)
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
                                    .text_color(theme::TEXT)
                                    .child("graf"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(format!("Version {version}")),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
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
                                    .border_color(theme::BORDER)
                                    .text_center()
                                    .text_xs()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::HOVER_BG))
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
                                    .bg(theme::ACCENT_BLUE)
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
        } else if is_restore_recovery {
            let entries = self
                .pending_recovery
                .as_ref()
                .map(|journal| journal.entries.clone())
                .unwrap_or_default();

            let mut list = div().flex().flex_col().gap_1();
            for entry in &entries {
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .rounded_xs()
                        .bg(theme::BG_BAR)
                        .text_xs()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(theme::TEXT)
                                .child(entry.title.clone()),
                        )
                        .child(
                            div()
                                .text_color(theme::TEXT_MUTED)
                                .child(format!("{} chars", entry.content.len())),
                        ),
                );
            }

            modal_content = modal_content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme::TEXT)
                            .child("graf found unsaved changes from a previous session."),
                    )
                    .child(list)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .id("discard-recovery")
                                    .px_3()
                                    .py_1()
                                    .rounded_xs()
                                    .text_xs()
                                    .text_color(theme::ACCENT_RED)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(theme::HOVER_BG))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.discard_recovery(cx)),
                                    )
                                    .child("Discard"),
                            )
                            .child(
                                div()
                                    .id("restore-recovery")
                                    .px_3()
                                    .py_1()
                                    .rounded_xs()
                                    .bg(theme::ACCENT_BLUE)
                                    .text_xs()
                                    .text_color(theme::BG)
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.restore_recovery(cx)),
                                    )
                                    .child("Restore"),
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
                                .text_color(theme::TEXT)
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
                                        .hover(|style| style.bg(theme::HOVER_BG))
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
                                        .text_color(theme::ACCENT_RED)
                                        .cursor_pointer()
                                        .hover(|style| style.bg(theme::HOVER_BG))
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
                                        .bg(theme::ACCENT_BLUE)
                                        .text_xs()
                                        .text_color(theme::BG)
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
        } else if is_cmd_palette {
            let filter = self.prompt_editor.read(cx).text().to_lowercase();

            let mut list = div()
                .id("cmd-palette-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();

            for item in filter_commands(&filter) {
                let id = item.id;
                let name = item.title;
                let shortcut = item.shortcut;
                let category = item.category;

                let row = div()
                    .id(format!("cmd-row-{:?}", id))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .text_xs()
                    .text_color(theme::TEXT)
                    .hover(|s| s.bg(theme::HOVER_BG))
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
                                    .bg(theme::BG_BAR)
                                    .border_1()
                                    .border_color(theme::BORDER)
                                    .text_xs()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(category),
                            )
                            .child(name),
                    )
                    .child(div().text_color(theme::TEXT_MUTED).child(shortcut));
                list = list.child(row);
            }
            modal_content = modal_content.child(list);
        } else if is_template_picker {
            let ActiveModal::TemplatePicker(request) = self.active_modal else {
                return None;
            };
            let filter = self.prompt_editor.read(cx).text().to_lowercase();

            let mut list = div()
                .id("template-picker-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();

            for template in crate::project::templates::filter_templates(&filter, request.kind) {
                let kind_label = match template.kind {
                    crate::project::document::DocumentKind::Latex => "TEX",
                    _ => "TYP",
                };
                let template_id = template.id;
                let row = div()
                    .id(format!("template-row-{template_id}"))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .text_xs()
                    .text_color(theme::TEXT)
                    .hover(|s| s.bg(theme::HOVER_BG))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.accept_template(template_id, request, cx);
                            window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .w(px(32.0))
                            .text_color(theme::ACCENT_BLUE)
                            .child(kind_label),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(template.name),
                            )
                            .child(
                                div()
                                    .text_color(theme::TEXT_MUTED)
                                    .child(template.description),
                            ),
                    )
                    .child(
                        div()
                            .text_color(theme::TEXT_MUTED)
                            .child(template.file_name),
                    );
                list = list.child(row);
            }
            modal_content = modal_content.child(list);
        } else {
            let filter = self.prompt_editor.read(cx).text();

            let mut list = div()
                .id("quick-open-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();

            // MATCHES come from the prebuilt flattened list; only the
            // filtered handful is cloned per render.
            for entry in self
                .project_tree
                .quick_open_matches(filter, state::QUICK_OPEN_LIMIT)
            {
                let title = entry.relative.clone();
                let path = entry.path.clone();
                let kind = entry.kind;
                let row_id = title.clone();
                let row = div()
                    .id(format!("quick-open-{row_id}"))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .text_xs()
                    .text_color(theme::TEXT)
                    .hover(|style| style.bg(theme::HOVER_BG))
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
                            .text_color(theme::ACCENT_BLUE)
                            .child(kind.label()),
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
