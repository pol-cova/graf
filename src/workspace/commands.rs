#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteItem {
    pub id: u32,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
}

pub fn all_commands() -> &'static [CommandPaletteItem] {
    &[
        CommandPaletteItem {
            id: 1,
            title: "Compile",
            shortcut: "⌘⇧B",
            category: "Document",
        },
        CommandPaletteItem {
            id: 2,
            title: "Save",
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
            title: "Settings",
            shortcut: "⌘,",
            category: "Preferences",
        },
        CommandPaletteItem {
            id: 12,
            title: "About graf",
            shortcut: "",
            category: "About",
        },
        CommandPaletteItem {
            id: 3,
            title: "Find in File",
            shortcut: "⌘F",
            category: "Editor",
        },
        CommandPaletteItem {
            id: 4,
            title: "Toggle Project",
            shortcut: "⌘⇧E",
            category: "View",
        },
        CommandPaletteItem {
            id: 5,
            title: "Toggle Preview",
            shortcut: "⌘⇧P",
            category: "View",
        },
        CommandPaletteItem {
            id: 6,
            title: "Toggle Problems",
            shortcut: "⌘⇧M",
            category: "View",
        },
        CommandPaletteItem {
            id: 7,
            title: "Close Tab",
            shortcut: "⌘W",
            category: "File",
        },
    ]
}

/// Substring filter shared by the palette view and Enter-picking so the
/// visible list and the accepted result can never disagree. `query` is
/// expected lowercase.
pub fn filter_commands(query_lower: &str) -> impl Iterator<Item = &'static CommandPaletteItem> {
    all_commands().iter().filter(move |item| {
        query_lower.is_empty()
            || item.title.to_lowercase().contains(query_lower)
            || item.category.to_lowercase().contains(query_lower)
    })
}
