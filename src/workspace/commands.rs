//! Command Palette action definitions and registry for the Graf workspace.

/// A registered action in the Command Palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteItem {
    pub id: u32,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
}

/// Returns the static registry of all available workspace commands.
pub fn all_commands() -> &'static [CommandPaletteItem] {
    &[
        CommandPaletteItem {
            id: 1,
            title: "Compile Document",
            shortcut: "⌘⇧B",
            category: "Build",
        },
        CommandPaletteItem {
            id: 2,
            title: "Save Active Document",
            shortcut: "⌘S",
            category: "File",
        },
        CommandPaletteItem {
            id: 14,
            title: "Insert Table or Matrix",
            shortcut: "⌘⌥T",
            category: "Editor",
        },
        CommandPaletteItem {
            id: 17,
            title: "Check Writing Style",
            shortcut: "⌘⌥L",
            category: "Quality",
        },
        CommandPaletteItem {
            id: 18,
            title: "Sync Zotero Library",
            shortcut: "",
            category: "References",
        },
        CommandPaletteItem {
            id: 15,
            title: "Export Canvas as TikZ",
            shortcut: "",
            category: "Export",
        },
        CommandPaletteItem {
            id: 16,
            title: "Export Canvas as SVG",
            shortcut: "",
            category: "Export",
        },
        CommandPaletteItem {
            id: 21,
            title: "Reload Plugins",
            shortcut: "",
            category: "Extensions",
        },
        CommandPaletteItem {
            id: 11,
            title: "New Typst Document",
            shortcut: "⌘T",
            category: "File",
        },
        CommandPaletteItem {
            id: 8,
            title: "New Vector Diagram",
            shortcut: "⌘N",
            category: "File",
        },
        CommandPaletteItem {
            id: 10,
            title: "Open Settings",
            shortcut: "⌘,",
            category: "Preferences",
        },
        CommandPaletteItem {
            id: 12,
            title: "View Open Source Licenses",
            shortcut: "",
            category: "About",
        },
        CommandPaletteItem {
            id: 13,
            title: "Check Recovery Journal",
            shortcut: "",
            category: "Diagnostics",
        },
        CommandPaletteItem {
            id: 3,
            title: "Find in File",
            shortcut: "⌘F",
            category: "Editor",
        },
        CommandPaletteItem {
            id: 4,
            title: "◫ Toggle Left Sidebar",
            shortcut: "⌘⇧E",
            category: "View",
        },
        CommandPaletteItem {
            id: 5,
            title: "◨ Toggle Right Preview",
            shortcut: "⌘⇧P",
            category: "View",
        },
        CommandPaletteItem {
            id: 6,
            title: "Toggle Problems Drawer",
            shortcut: "⌘⇧M",
            category: "View",
        },
        CommandPaletteItem {
            id: 7,
            title: "× Close Active Tab",
            shortcut: "⌘W",
            category: "File",
        },
    ]
}
