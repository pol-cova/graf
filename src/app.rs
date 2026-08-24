use gpui::{
    App, Bounds, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, TitlebarOptions,
    WindowBounds, WindowOptions, actions, prelude::*, px, size,
};
use gpui_platform::application;
use log::{error, info};

use crate::workspace::Workspace;

const WINDOW_WIDTH: f32 = 1200.0;
const WINDOW_HEIGHT: f32 = 800.0;

actions!(graf, [Quit]);

pub fn run() {
    application().run(|cx: &mut App| {
        crate::editor::view::register_bindings(cx);
        crate::workspace::register_bindings(cx);
        set_app_menus(cx);

        let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);

        let window = match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: None,
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(Workspace::new),
        ) {
            Ok(window) => window,
            Err(error) => {
                error!("failed to open graf window: {error}");
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

fn set_app_menus(cx: &mut App) {
    use crate::editor::view::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    use crate::workspace::{
        CloseTab, CommandPalette, OpenAbout, OpenFile, OpenSettings, Save, ToggleDiagnostics,
        ToggleFind, TogglePerformanceOverlay, TogglePreview, ToggleSidebar,
    };

    cx.set_menus([
        Menu::new("graf").items([
            MenuItem::action("About graf", OpenAbout),
            MenuItem::separator(),
            MenuItem::action("Settings...", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Quit graf", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Open...", OpenFile),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Close Tab", CloseTab),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            MenuItem::separator(),
            MenuItem::action("Find", ToggleFind),
        ]),
        Menu::new("View").items([
            MenuItem::action("Command Palette", CommandPalette),
            MenuItem::separator(),
            MenuItem::action("Project", ToggleSidebar),
            MenuItem::action("Preview", TogglePreview),
            MenuItem::action("Problems", ToggleDiagnostics),
            MenuItem::separator(),
            MenuItem::action("Frame Timings", TogglePerformanceOverlay),
        ]),
        Menu::new("Window").items([MenuItem::action("Close Tab", CloseTab)]),
        Menu::new("Help").items([MenuItem::action("Command Palette", CommandPalette)]),
    ]);
}
