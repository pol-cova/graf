//! Modal dialogs and overlay views (Quick Open, Command Palette, AI Assist, Diff Review, Settings).

use gpui::{Context, IntoElement, ParentElement, Styled, div, prelude::*, px};
use std::path::Path;

use super::commands::all_commands;
use super::{ActiveModal, SettingsTab, Workspace};
use crate::ai::operations::AiOperationKind;
use crate::ui::theme;

impl Workspace {
    /// Renders modal overlay for Quick Open (⌘P), Command Palette (⌘K), AI Assist (⌘I), Settings (⌘,), or Diff Review.
    pub fn render_modal(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let is_quick_open = matches!(self.active_modal, ActiveModal::QuickOpen(_));
        let is_cmd_palette = matches!(self.active_modal, ActiveModal::CommandPalette(_));
        let is_ai_assist = matches!(self.active_modal, ActiveModal::AiAssist(_));
        let is_diff_review = matches!(self.active_modal, ActiveModal::DiffReview(_));
        let is_settings = matches!(self.active_modal, ActiveModal::Settings(_));

        if !is_quick_open && !is_cmd_palette && !is_ai_assist && !is_diff_review && !is_settings {
            return None;
        }

        let title = if is_quick_open {
            "Quick Open File (⌘P)"
        } else if is_cmd_palette {
            "Command Palette (⌘K)"
        } else if is_ai_assist {
            "✨ AI Technical Writing Assistant (⌘I)"
        } else if is_settings {
            "⚙️ Graf Settings & Preferences (⌘,)"
        } else {
            "🔍 AI Diff Review — Accept (⌘Enter) or Reject (Esc)"
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
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child("Esc to close"),
                    ),
            );

        if is_settings {
            if let ActiveModal::Settings(tab) = self.active_modal {
                let tabs = [
                    (SettingsTab::General, "General"),
                    (SettingsTab::Editor, "Editor"),
                    (SettingsTab::Ai, "AI & ACP"),
                    (SettingsTab::Canvas, "Canvas"),
                    (SettingsTab::Licenses, "About & Licenses"),
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
                    SettingsTab::General => {
                        settings_body = settings_body
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Theme Palette:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child(self.settings.theme.theme_name.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Supported Engines:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child("LaTeX (Tectonic) + Typst"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Autosave Crash Recovery:"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme::color(theme::ACCENT_GREEN))
                                            .child("✓ Active (.graf/recovery)"),
                                    ),
                            );
                    }
                    SettingsTab::Editor => {
                        settings_body = settings_body
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Editor Font Size:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child(format!(
                                                "{:.1} px",
                                                self.settings.editor.font_size
                                            )),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Tab Size:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
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
                                    .child(div().text_xs().child("Auto-compile on Save (⌘S):"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme::color(theme::ACCENT_GREEN))
                                            .child(if self.settings.editor.auto_compile_on_save {
                                                "✓ Enabled"
                                            } else {
                                                "✗ Disabled"
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Debounce Compile Delay:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child(format!(
                                                "{} ms",
                                                self.settings.editor.compile_debounce_ms
                                            )),
                                    ),
                            );
                    }
                    SettingsTab::Ai => {
                        settings_body = settings_body
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Protocol Protocol:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::ACCENT_BLUE))
                                            .text_xs()
                                            .text_color(theme::color(theme::ACCENT_BLUE))
                                            .child("Agent Client Protocol (ACP v1)"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("ACP Agent Command:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child(if self.settings.ai.acp_command.is_empty() {
                                                "Built-in Local ACP Runtime".to_string()
                                            } else {
                                                self.settings.ai.acp_command.clone()
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Sampling Temperature:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child(format!("{:.2}", self.settings.ai.temperature)),
                                    ),
                            );
                    }
                    SettingsTab::Canvas => {
                        settings_body = settings_body
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Dot Grid Background:"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme::color(theme::ACCENT_GREEN))
                                            .child("✓ 20px Grid Enabled"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Snap to Grid on Drag:"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme::color(theme::ACCENT_GREEN))
                                            .child("✓ Snap Enabled"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(div().text_xs().child("Default Stroke Accent:"))
                                    .child(
                                        div()
                                            .px_2()
                                            .py_1()
                                            .rounded_xs()
                                            .bg(theme::color(theme::BG_BAR))
                                            .border_1()
                                            .border_color(theme::color(theme::BORDER))
                                            .text_xs()
                                            .child(
                                                self.settings.canvas.default_stroke_color.clone(),
                                            ),
                                    ),
                            );
                    }
                    SettingsTab::Licenses => {
                        let licenses = crate::project::licenses::audited_licenses();
                        let mut lic_div = div().flex().flex_col().gap_2();

                        lic_div = lic_div.child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child("Graf v0.1.0 (macOS Apple Silicon)"),
                                )
                                .child(
                                    div()
                                        .text_color(theme::color(theme::ACCENT_BLUE))
                                        .child("MIT License"),
                                ),
                        );

                        for lic in licenses {
                            let item = div()
                                .p_2()
                                .rounded_xs()
                                .bg(theme::color(theme::BG_BAR))
                                .border_1()
                                .border_color(theme::color(theme::BORDER))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(theme::color(theme::TEXT))
                                                .child(format!("{} (v{})", lic.name, lic.version)),
                                        )
                                        .child(
                                            div()
                                                .text_color(theme::color(theme::ACCENT_GREEN))
                                                .child(lic.license),
                                        ),
                                )
                                .child(
                                    div()
                                        .mt_0p5()
                                        .text_xs()
                                        .text_color(theme::color(theme::TEXT_MUTED))
                                        .child(lic.description),
                                );
                            lic_div = lic_div.child(item);
                        }

                        settings_body = settings_body.child(lic_div);
                    }
                }

                modal_content = modal_content.child(tab_header).child(settings_body);
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
                    // Original block
                    .child(
                        div()
                            .p_2()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::ACCENT_RED))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT_MUTED))
                            .child("− Original:")
                            .child(div().mt_1().child(original)),
                    )
                    // Replacement block
                    .child(
                        div()
                            .p_2()
                            .rounded_xs()
                            .bg(theme::color(theme::BG_BAR))
                            .border_1()
                            .border_color(theme::color(theme::ACCENT_GREEN))
                            .text_xs()
                            .text_color(theme::color(theme::TEXT))
                            .child("+ AI Proposal:")
                            .child(div().mt_1().child(replacement)),
                    )
                    // Action footer
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
                                    .child("Accept Changes (⌘Enter)"),
                            ),
                    );

                modal_content = modal_content.child(diff_body);
            }
        } else if is_ai_assist {
            let ai_ops = [
                (
                    AiOperationKind::RewriteAcademic,
                    "✨ Polish Academic Tone",
                    "Rewrite buffer with formal tone & mathematical rigor",
                ),
                (
                    AiOperationKind::Shorten,
                    "✂️ Shorten & Condense",
                    "Tighten prose while retaining formulas and key claims",
                ),
                (
                    AiOperationKind::Explain,
                    "💡 Explain Section / Formula",
                    "Generate clear walkthrough of selected technical concepts",
                ),
                (
                    AiOperationKind::FixDiagnostic {
                        message: "Auto-detected diagnostic".to_string(),
                        line: None,
                    },
                    "🔧 Fix LaTeX Compiler Errors",
                    "Analyze errors and apply automated syntax patch",
                ),
                (
                    AiOperationKind::GenerateDiagram {
                        prompt: "System Architecture Pipeline".to_string(),
                    },
                    "🎨 Generate Vector Diagram (.graf)",
                    "Create structured vector scene from description",
                ),
            ];

            let mut list = div()
                .id("ai-assist-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();
            for (op, name, desc) in ai_ops {
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
                            .child("Run ➔"),
                    );
                list = list.child(row);
            }
            modal_content = modal_content.child(list);
        } else if is_cmd_palette {
            let filter = match &self.active_modal {
                ActiveModal::CommandPalette(f) => f.to_lowercase(),
                _ => String::new(),
            };

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
                    .py_2()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT))
                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.active_modal = ActiveModal::None;
                            this.dispatch_command_action(id, cx);
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
            // Quick Open list of project files
            let filter = match &self.active_modal {
                ActiveModal::QuickOpen(f) => f.to_lowercase(),
                _ => String::new(),
            };

            let mut list = div()
                .id("quick-open-list")
                .flex()
                .flex_col()
                .py_1()
                .overflow_scroll();

            for doc in &self.documents {
                let title = doc.title().to_string();
                if !filter.is_empty() && !title.to_lowercase().contains(&filter) {
                    continue;
                }

                let doc_path = doc.path().map(Path::to_path_buf);
                let row = div()
                    .id(format!("quick-open-doc-{}", doc.id().0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT))
                    .hover(|s| s.bg(theme::color(theme::HOVER_BG)))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.active_modal = ActiveModal::None;
                            if let Some(path) = doc_path.clone() {
                                this.open_file(path, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .child("📄")
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
