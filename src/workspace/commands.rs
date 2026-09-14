#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPaletteItem {
    pub id: CommandId,
    pub title: &'static str,
    pub shortcut: &'static str,
    pub category: &'static str,
}

/// Exhaustive identifier for every palette command; dispatch matches on
/// this enum so the compiler flags forgotten arms instead of silently
/// ignoring a hand-typed number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    Compile,
    Save,
    FindInFile,
    ToggleProject,
    TogglePreview,
    ToggleProblems,
    CloseTab,
    NewVectorDiagram,
    NewTypstDocument,
    AboutGraf,
    OpenSettings,
    CheckWritingStyle,
    SyncZotero,
    ExportTikz,
    ExportSvg,
    InsertTable,
}

pub fn all_commands() -> &'static [CommandPaletteItem] {
    &[
        CommandPaletteItem {
            id: CommandId::Compile,
            title: "Compile",
            shortcut: "⌘⇧B",
            category: "Document",
        },
        CommandPaletteItem {
            id: CommandId::Save,
            title: "Save",
            shortcut: "⌘S",
            category: "File",
        },
        CommandPaletteItem {
            id: CommandId::InsertTable,
            title: "Insert Table or Matrix",
            shortcut: "⌘⌥T",
            category: "Editor",
        },
        CommandPaletteItem {
            id: CommandId::CheckWritingStyle,
            title: "Check Writing Style",
            shortcut: "⌘⌥L",
            category: "Quality",
        },
        CommandPaletteItem {
            id: CommandId::SyncZotero,
            title: "Sync Zotero Library",
            shortcut: "",
            category: "References",
        },
        CommandPaletteItem {
            id: CommandId::ExportTikz,
            title: "Export Canvas as TikZ",
            shortcut: "",
            category: "Export",
        },
        CommandPaletteItem {
            id: CommandId::ExportSvg,
            title: "Export Canvas as SVG",
            shortcut: "",
            category: "Export",
        },
        CommandPaletteItem {
            id: CommandId::NewTypstDocument,
            title: "New Typst Document",
            shortcut: "⌘T",
            category: "File",
        },
        CommandPaletteItem {
            id: CommandId::NewVectorDiagram,
            title: "New Vector Diagram",
            shortcut: "⌘N",
            category: "File",
        },
        CommandPaletteItem {
            id: CommandId::OpenSettings,
            title: "Settings",
            shortcut: "⌘,",
            category: "Preferences",
        },
        CommandPaletteItem {
            id: CommandId::AboutGraf,
            title: "About graf",
            shortcut: "",
            category: "About",
        },
        CommandPaletteItem {
            id: CommandId::FindInFile,
            title: "Find in File",
            shortcut: "⌘F",
            category: "Editor",
        },
        CommandPaletteItem {
            id: CommandId::ToggleProject,
            title: "Toggle Project",
            shortcut: "⌘⇧E",
            category: "View",
        },
        CommandPaletteItem {
            id: CommandId::TogglePreview,
            title: "Toggle Preview",
            shortcut: "⌘⇧P",
            category: "View",
        },
        CommandPaletteItem {
            id: CommandId::ToggleProblems,
            title: "Toggle Problems",
            shortcut: "⌘⇧M",
            category: "View",
        },
        CommandPaletteItem {
            id: CommandId::CloseTab,
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
