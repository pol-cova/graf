//! The workspace's GPUI shell: one render root wiring every action
//! subscription into the component tree.

use super::*;

use crate::ui::theme;
use gpui::{Context, Render, Window, div, prelude::*};

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::color(theme::BG))
            .text_color(theme::color(theme::TEXT))
            .text_sm()
            .on_mouse_move(cx.listener(Self::resize_panel))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(Self::finish_panel_resize),
            )
            .on_action(cx.listener(Self::on_focus_editor))
            .on_action(cx.listener(Self::on_compile))
            .on_action(cx.listener(Self::on_open_file))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_close_tab))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_preview))
            .on_action(cx.listener(Self::on_toggle_diagnostics))
            .on_action(cx.listener(Self::on_toggle_find))
            .on_action(cx.listener(Self::on_quick_open))
            .on_action(cx.listener(Self::on_command_palette))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_open_about))
            .on_action(cx.listener(Self::on_close_modal))
            .on_action(cx.listener(Self::on_autocomplete))
            .on_action(cx.listener(Self::on_toggle_performance_overlay))
            .child(self.render_top_bar(cx))
            .child(self.render_body(cx))
            .child(self.render_status_bar(cx));

        if self.workspace_menu_open {
            root = root.child(self.render_workspace_menu(cx));
        }

        if let Some(modal) = self.render_modal(cx) {
            root = root.child(modal);
        }

        root
    }
}
