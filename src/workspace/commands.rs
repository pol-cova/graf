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
            title: "▶ Compile Document",
            shortcut: "⌘⇧B",
            category: "Build",
        },
        CommandPaletteItem {
            id: 2,
            title: "💾 Save Active Document",
            shortcut: "⌘S",
            category: "File",
        },
        CommandPaletteItem {
            id: 14,
            title: "📊 Insert Academic Table / Matrix",
            shortcut: "⌘⌥T",
            category: "Editor",
        },
        CommandPaletteItem {
            id: 20,
            title: "📊 Import Mermaid Diagram (.graf)",
            shortcut: "",
            category: "Canvas",
        },
        CommandPaletteItem {
            id: 17,
            title: "🔍 Lint Academic Writing Style",
            shortcut: "⌘⌥L",
            category: "Quality",
        },
        CommandPaletteItem {
            id: 18,
            title: "🔄 Sync Local Zotero Library",
            shortcut: "",
            category: "References",
        },
        CommandPaletteItem {
            id: 19,
            title: "📚 Insert Citation from arXiv",
            shortcut: "",
            category: "References",
        },
        CommandPaletteItem {
            id: 15,
            title: "🎨 Export Canvas to TikZ LaTeX",
            shortcut: "",
            category: "Export",
        },
        CommandPaletteItem {
            id: 16,
            title: "🎨 Export Canvas to SVG Markup",
            shortcut: "",
            category: "Export",
        },
        CommandPaletteItem {
            id: 21,
            title: "🧩 Scan & Reload Plugins",
            shortcut: "",
            category: "Extensions",
        },
        CommandPaletteItem {
            id: 11,
            title: "⚡ New Typst Document (.typ)",
            shortcut: "⌘T",
            category: "File",
        },
        CommandPaletteItem {
            id: 8,
            title: "🎨 New Vector Diagram (.graf)",
            shortcut: "⌘N",
            category: "File",
        },
        CommandPaletteItem {
            id: 10,
            title: "⚙️ Preferences: Open Settings",
            shortcut: "⌘,",
            category: "Preferences",
        },
        CommandPaletteItem {
            id: 9,
            title: "✨ AI Technical Writing Assistant",
            shortcut: "⌘I",
            category: "AI",
        },
        CommandPaletteItem {
            id: 12,
            title: "📜 View Open Source Licenses",
            shortcut: "",
            category: "About",
        },
        CommandPaletteItem {
            id: 13,
            title: "🛡️ Check Crash Recovery Journal",
            shortcut: "",
            category: "Diagnostics",
        },
        CommandPaletteItem {
            id: 3,
            title: "🔍 Find & Replace in File",
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
            title: "⚠️ Toggle Problems Drawer",
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
