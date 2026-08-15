use gpui::{
    App, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds, WindowOptions, actions,
    prelude::*, px, size,
};
use gpui_platform::application;
use log::{error, info};

use crate::workspace::Workspace;

const WINDOW_WIDTH: f32 = 1200.0;
const WINDOW_HEIGHT: f32 = 800.0;

actions!(graf, [Quit]);

/// Launch the Graf application.
///
/// Creates a native GPUI window at approximately 1200×800, sets up global
/// keybindings, and renders the [`Workspace`] shell.
pub fn run() {
    application().run(|cx: &mut App| {
        crate::editor::view::register_bindings(cx);
        crate::workspace::register_bindings(cx);

        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

        let window = match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Graf")),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(Workspace::new),
        ) {
            Ok(window) => window,
            Err(error) => {
                error!("failed to open Graf window: {error}");
                cx.quit();
                return;
            }
        };

        window
            .update(cx, |workspace, window, cx| {
                window.focus(&workspace.editor_focus_handle(cx), cx);
            })
            .ok();

        cx.activate(true);

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

        info!("graf window opened");
    });
}
