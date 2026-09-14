use super::commands::filter_commands;
use super::{ActiveModal, SettingsTab, Workspace, state};
use gpui::{
    ClipboardItem, Context, Focusable, IntoElement, ParentElement, Role, Stateful, Styled, div,
    prelude::*, px,
};

use crate::ui::icons::{Icon, icon};
use crate::ui::theme;
use crate::ui::widgets::{list_row, list_row_lead};

impl Workspace {
    pub fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        // One dispatch point decides which modal renders; each arm owns its
        // body and hands it to a shared chrome (title bar, prompt row).
        let modal_body = match &self.active_modal {
            ActiveModal::QuickOpen => Some(self.render_quick_open(cx)),
            ActiveModal::CommandPalette => Some(self.render_command_palette(cx)),
            ActiveModal::Settings(_) => Some(self.render_settings_modal(cx)),
            ActiveModal::About => Some(self.render_about_modal(cx)),
            ActiveModal::ConfirmClose(_) => Some(self.render_confirm_close_modal(cx)),
            ActiveModal::RestoreRecovery => Some(self.render_restore_recovery_modal(cx)),
            ActiveModal::TemplatePicker(_) => self.render_template_picker(cx),
            ActiveModal::None => None,
        }?;

        Some(
            div()
                .id("modal-backdrop")
                .absolute()
                .size_full()
                .flex()
                .items_start()
                .justify_center()
                .pt(px(60.0))
                .bg(theme::OVERLAY)
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.active_modal = ActiveModal::None;
                        cx.notify();
                    }),
                )
                .child(modal_body),
        )
    }

    fn modal_frame(&self, title: &'static str, cx: &mut Context<Self>) -> gpui::Div {
        div()
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
            .child(modal_title_bar(title, cx))
    }

    /// Frame for the three query-driven modals: chrome plus a prompt input.
    fn query_modal_frame(
        &self,
        title: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        self.modal_frame(title, cx)
            .child(
                div()
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
                    ),
            )
            .id("modal-frame")
    }

    fn render_quick_open(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
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
            let path = entry.path.clone();
            let row = list_row(
                format!("quick-open-{}", entry.relative),
                cx.listener(move |this, _, window, cx| {
                    this.active_modal = ActiveModal::None;
                    this.open_file(path.clone(), cx);
                    window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    cx.notify();
                }),
                [
                    list_row_lead(entry.kind.label()).into_any_element(),
                    div().child(entry.relative.clone()).into_any_element(),
                ],
            );
            list = list.child(row);
        }

        self.query_modal_frame("Open file", cx).child(list)
    }

    fn render_command_palette(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let filter = self.prompt_editor.read(cx).text().to_lowercase();

        let mut list = div()
            .id("cmd-palette-list")
            .flex()
            .flex_col()
            .py_1()
            .overflow_scroll();

        for item in filter_commands(&filter) {
            let row = list_row(
                format!("cmd-row-{:?}", item.id),
                cx.listener(move |this, _, window, cx| {
                    this.active_modal = ActiveModal::None;
                    this.dispatch_command_action(item.id, cx);
                    window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    cx.notify();
                }),
                [
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
                                .child(item.category),
                        )
                        .child(item.title)
                        .into_any_element(),
                    div()
                        .text_color(theme::TEXT_MUTED)
                        .child(item.shortcut)
                        .into_any_element(),
                ],
            )
            .justify_between();
            list = list.child(row);
        }

        self.query_modal_frame("Commands", cx).child(list)
    }

    fn render_template_picker(&self, cx: &mut Context<Self>) -> Option<Stateful<gpui::Div>> {
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
            let row = list_row(
                format!("template-row-{}", template.id),
                cx.listener(move |this, _, window, cx| {
                    this.accept_template(template.id, request, cx);
                    window.focus(&this.editor.read(cx).focus_handle(cx), cx);
                    cx.notify();
                }),
                [
                    list_row_lead(kind_label).into_any_element(),
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
                        )
                        .into_any_element(),
                    div()
                        .text_color(theme::TEXT_MUTED)
                        .child(template.file_name)
                        .into_any_element(),
                ],
            );
            list = list.child(row);
        }

        Some(
            self.query_modal_frame(
                if request.for_new_project {
                    "New project"
                } else {
                    "New from template"
                },
                cx,
            )
            .child(list),
        )
    }

    fn render_settings_modal(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let mut modal = self.modal_frame("Settings", cx).id("modal-frame");
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
                            cx.listener(move |this, _, _, cx| this.open_settings(t, cx)),
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
                    settings_body = settings_body
                        .child(
                            setting_row(
                                "Compile while editing",
                                Some("Rebuild the active document after changes."),
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
                            setting_row(
                                "Compile delay",
                                Some("Wait before rebuilding after a keystroke."),
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
                    settings_body =
                        settings_body
                            .child(
                                setting_row("Font size", None).child(
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
                                setting_row("Tab width", None).child(
                                    setting_button()
                                        .id("tab-size-setting")
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(|this, _, _, cx| this.cycle_tab_size(cx)),
                                        )
                                        .child(format!("{} spaces", self.settings.editor.tab_size)),
                                ),
                            )
                            .child(
                                setting_row("Line numbers", None).child(
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

            modal = modal.child(tab_header).child(settings_body);
        }
        modal
    }

    fn render_about_modal(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let version = env!("CARGO_PKG_VERSION");
        self.modal_frame("About graf", cx).id("modal-frame").child(
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
                        .text_color(theme::WHITE)
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
                                        cx.write_to_clipboard(ClipboardItem::new_string(format!(
                                            "graf {version}\n{} {}",
                                            std::env::consts::OS,
                                            std::env::consts::ARCH
                                        )));
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
                                .text_color(theme::WHITE)
                                .cursor_pointer()
                                .hover(|style| style.opacity(0.9))
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.close_modal(cx)),
                                )
                                .child("OK"),
                        ),
                ),
        )
    }

    fn render_restore_recovery_modal(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
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

        self.modal_frame("Restore unsaved work", cx)
            .id("modal-frame")
            .child(
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
                                footer_button(
                                    "discard-recovery",
                                    "Discard",
                                    Some(theme::ACCENT_RED),
                                    None,
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.discard_recovery(cx)),
                                ),
                            )
                            .child(
                                footer_button("restore-recovery", "Restore", None, Some(theme::BG))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| this.restore_recovery(cx)),
                                    ),
                            ),
                    ),
            )
    }

    fn render_confirm_close_modal(&self, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        let mut modal = self.modal_frame("Unsaved changes", cx).id("modal-frame");
        if let ActiveModal::ConfirmClose(index) = self.active_modal {
            let title = self
                .documents
                .get(index)
                .map(|document| document.title().to_string())
                .unwrap_or_else(|| "document".to_string());
            modal = modal.child(
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
                                footer_button("cancel-close", "Cancel", None, None)
                                    .hover(|style| style.bg(theme::HOVER_BG))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            this.active_modal = ActiveModal::None;
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(
                                footer_button(
                                    "discard-close",
                                    "Discard",
                                    Some(theme::ACCENT_RED),
                                    None,
                                )
                                .hover(|style| style.bg(theme::HOVER_BG))
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.active_modal = ActiveModal::None;
                                        this.force_close_tab(index, cx);
                                    }),
                                ),
                            )
                            .child(
                                footer_button("save-before-close", "Save", None, Some(theme::BG))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            this.active_modal = ActiveModal::None;
                                            if index != this.active_doc_idx {
                                                this.switch_tab(index, cx);
                                            }
                                            this.save_active_document(cx);
                                        }),
                                    ),
                            ),
                    ),
            );
        }
        modal
    }
}

/// Title bar shared by every modal: label, Esc hint, and close button.
fn modal_title_bar(title: &'static str, cx: &mut Context<Workspace>) -> gpui::Div {
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
        )
}

/// Small padded chip used by all modal settings toggles.
fn setting_button() -> gpui::Div {
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
}

fn setting_row(label: &'static str, hint: Option<&'static str>) -> gpui::Div {
    div().flex().items_center().justify_between().child(
        div()
            .flex()
            .flex_col()
            .child(div().text_xs().child(label))
            .child(hint.map_or_else(
                || div(),
                |hint| div().text_xs().text_color(theme::TEXT_MUTED).child(hint),
            )),
    )
}

/// Small footer button shared by the recovery and confirm-close modals; the
/// text color and (optional) solid background vary per action.
fn footer_button(
    id: &'static str,
    label: &'static str,
    text_color: Option<gpui::Rgba>,
    bg: Option<gpui::Rgba>,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_xs()
        .text_xs()
        .cursor_pointer()
        .text_color(text_color.unwrap_or(theme::TEXT_MUTED))
        .child(label)
        .when_some(bg, |button, bg| button.bg(bg))
}
