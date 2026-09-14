mod actions;
mod commands;
mod compilation;
mod completions;
mod diagnostics;
mod documents;
mod editor_panel;
mod exports;
mod find_bar;
mod layout;
mod lint;
mod modals;
mod new_documents;
mod render;
mod settings;
mod sidebar;
mod state;
mod status_bar;
mod templates;
mod top_bar;
mod welcome;
pub(crate) use state::next_draft_title;
pub(crate) use state::unique_title;
pub(crate) use state::{DraftKind, active_index_after_close};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::{info, warn};

use gpui::{
    Context, DebugFrameOverlayMode, Entity, FocusHandle, Focusable, KeyBinding, MouseMoveEvent,
    MouseUpEvent, PathPromptOptions, Task, Window, actions, prelude::*,
};

use self::commands::CommandId;
use self::layout::{DIAGNOSTICS_HEIGHT_RANGE, PREVIEW_WIDTH_RANGE, SIDEBAR_WIDTH_RANGE};
use crate::canvas::view::CanvasView;
use crate::compiler::EngineKind;
use crate::compiler::controller::CompilerController;
use crate::compiler::diagnostics::Diagnostic;
use crate::compiler::engine::{CompileRequest, DocumentEngine};
use crate::compiler::tectonic::TectonicEngine;
use crate::compiler::typst::TypstEngine;
use crate::editor::find::FindState;
use crate::editor::view::{EditorEvent, EditorView, MAX_FONT_SIZE, MAX_TAB_SIZE, MIN_FONT_SIZE};
use crate::preview::renderer::{NativePdfRenderer, PdfRenderer};
use crate::preview::view::PreviewView;
use crate::project::document::Document;
use crate::project::settings::GrafSettings;
use crate::project::tree::ProjectTree;

/// Panel geometry and per-panel view toggles; layout.rs owns the resize
/// math, render submodules read the values.
pub(crate) struct LayoutState {
    pub(crate) sidebar_visible: bool,
    pub(crate) sidebar_tab: SidebarTab,
    pub(crate) preview_visible: bool,
    pub(crate) diagnostics_drawer_open: bool,
    pub(crate) diagnostics_filter: DiagnosticsFilter,
    pub(crate) sidebar_width: f32,
    pub(crate) preview_width: f32,
    pub(crate) diagnostics_height: f32,
    pub(crate) resizing_panel: Option<ResizingPanel>,
}

/// Compile lifecycle: debounced task, generations and cancellation so stale
/// timers and subprocess results never leak into the UI; compilation.rs
/// owns every mutation.
pub(crate) struct CompileState {
    pub(crate) task: Option<Task<()>>,
    pub(crate) running: bool,
    pub(crate) pending: bool,
    /// Bumped on every edit that schedules a compile; the debounce timer's
    /// captured generation must match to fire, so stale timers cannot
    /// trigger compiles.
    pub(crate) generation: u64,
    /// Cancel flag for the in-flight compile; flipping it kills the running
    /// compiler subprocess (and rasterization) instead of waiting it out.
    pub(crate) cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}

actions!(
    workspace,
    [
        Compile,
        OpenFile,
        Save,
        CloseTab,
        ToggleSidebar,
        TogglePreview,
        ToggleDiagnostics,
        ToggleFind,
        QuickOpen,
        CommandPalette,
        CloseModal,
        Autocomplete,
        OpenSettings,
        OpenAbout,
        NewFromTemplate,
        NewProject,
        TogglePerformanceOverlay,
        FocusEditor,
    ]
);

pub fn register_bindings(cx: &mut gpui::App) {
    macro_rules! bind {
        ($cx:expr, [ $( ($key:expr, $action:expr), )* ]) => {
            $cx.bind_keys([
                $(
                    KeyBinding::new($key, $action, None),
                    KeyBinding::new($key, $action, Some("Editor")),
                )*
            ]);
        };
    }

    bind!(
        cx,
        [
            ("cmd-shift-b", Compile),
            ("cmd-r", Compile),
            ("cmd-o", OpenFile),
            ("cmd-s", Save),
            ("cmd-w", CloseTab),
            ("cmd-f", ToggleFind),
            ("cmd-p", QuickOpen),
            ("cmd-k", CommandPalette),
            ("cmd-,", OpenSettings),
            ("cmd-shift-n", NewFromTemplate),
            ("cmd-shift-d", TogglePerformanceOverlay),
            ("ctrl-space", Autocomplete),
            ("cmd-shift-e", ToggleSidebar),
            ("cmd-shift-p", TogglePreview),
            ("cmd-shift-m", ToggleDiagnostics),
            ("escape", CloseModal),
        ]
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Files,
    Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Editor,
    Build,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsFilter {
    All,
    Errors,
    Warnings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveViewKind {
    Editor,
    Canvas,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizingPanel {
    Sidebar,
    Preview,
    Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    QuickOpen,
    CommandPalette,
    ConfirmClose(usize),
    RestoreRecovery,
    Settings(SettingsTab),
    About,
    TemplatePicker(TemplatePickerRequest),
}

/// What the template picker should do when a template is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplatePickerRequest {
    /// Scaffold a project in `pending_project_dir` instead of opening a tab.
    pub for_new_project: bool,
    /// Restrict the list to one document kind (used by the welcome screen).
    pub kind: Option<crate::project::document::DocumentKind>,
}

pub struct Workspace {
    pub(crate) project_tree: ProjectTree,
    pub(crate) documents: Vec<Document>,
    pub(crate) active_doc_idx: usize,
    pub(crate) editor: Entity<EditorView>,
    pub(crate) prompt_editor: Entity<EditorView>,
    pub(crate) canvas: Entity<CanvasView>,
    pub(crate) active_view_kind: ActiveViewKind,
    pub(crate) preview: Entity<PreviewView>,
    pub(crate) tectonic_compiler: Arc<dyn DocumentEngine>,
    pub(crate) typst_compiler: Arc<dyn DocumentEngine>,
    pub(crate) pdf_renderer: Arc<dyn PdfRenderer>,
    pub(crate) settings: GrafSettings,
    pub(crate) controller: CompilerController,
    pub(crate) compile: CompileState,
    pub(crate) show_welcome: bool,
    /// Panel visibility, sizes and resize progress; see `LayoutState`.
    pub(crate) layout: LayoutState,
    pub(crate) workspace_menu_open: bool,
    pub(crate) latest_diagnostics: Vec<Diagnostic>,
    pub(crate) workspace_error: Option<String>,
    /// Project-scoped reference indexes; the workspace only routes reloads.
    pub(crate) project_state: crate::project::state::ProjectState,
    /// Outline items keyed by editor revision; sidebar render (which is a
    /// `&self` paint) must not parse the document every paint.
    pub(crate) outline_cache:
        std::cell::RefCell<Option<(u64, Vec<crate::project::outline::OutlineItem>)>>,
    /// Cached word count keyed by (editor revision, Typst-ness): the status
    /// bar paints every frame but this scan must run per edit, not per frame.
    pub(crate) word_count_cache: std::cell::RefCell<Option<(u64, bool, usize)>>,
    pub(crate) completions: Vec<crate::editor::completion::CompletionItem>,
    pub(crate) completion_open: bool,
    pub(crate) completion_selected: usize,
    pub(crate) find_state: FindState,
    pub(crate) find_bar_open: bool,
    /// Personas routed through the shared prompt editor.
    pub(crate) prompt_target: state::PromptTarget,
    pub(crate) active_modal: ActiveModal,
    pub(crate) pending_recovery: Option<crate::project::recovery::RecoveryJournal>,
    /// Undo history for `.graf` documents, owned per document so the shared
    /// `CanvasView` keeps each document's trail across tab switches.
    pub(crate) history_store: state::CanvasHistoryStore,
    /// Directory chosen for a project that is being created; set by the
    /// directory picker and consumed when a template is accepted.
    pub(crate) pending_project_dir: Option<PathBuf>,
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_tree = ProjectTree::scan(&current_dir);

        let initial_text = crate::project::templates::DEFAULT_LATEX_STARTER;

        let show_welcome = project_tree.root_document().is_none();
        let (initial_doc, open_error) = if let Some(root_doc) = project_tree.root_document() {
            match Document::open(root_doc) {
                Ok(doc) => (doc, None),
                Err(error) => (
                    Document::new_untitled("main.tex", initial_text),
                    Some(format!("Could not open {}: {error}", root_doc.display())),
                ),
            }
        } else {
            (Document::new_untitled("main.tex", initial_text), None)
        };

        let settings = GrafSettings::load_default();
        let editor_settings = settings.editor.clone();
        let initial_kind = initial_doc.kind();
        let initial_content = initial_doc.buffer().content().to_string();
        let editor = cx.new(|cx| {
            let mut editor = EditorView::with_text(cx, initial_content);
            editor.set_kind(initial_kind, cx);
            editor.set_preferences(
                editor_settings.font_size,
                editor_settings.tab_size,
                editor_settings.line_numbers,
                cx,
            );
            editor
        });
        let prompt_editor = cx.new(|cx| {
            let mut editor = EditorView::with_text(cx, "");
            editor.set_kind(crate::project::document::DocumentKind::PlainText, cx);
            editor.set_single_line(true);
            editor.set_preferences(13.0, 2, false, cx);
            editor
        });
        let canvas = cx.new(CanvasView::new);
        let preview = cx.new(|_cx| PreviewView::new());
        let tectonic_compiler: Arc<dyn DocumentEngine> = Arc::new(TectonicEngine::new());
        let typst_compiler: Arc<dyn DocumentEngine> = Arc::new(TypstEngine::new());

        // Prime engines off the UI thread: resolution (a `which` spawn and
        // path probes) and the Tectonic support-file download both happen in
        // this background task, so the first frame and the first compile do
        // not wait on them.
        let warm_up_engine = tectonic_compiler.clone();
        let warm_up_typst = typst_compiler.clone();
        cx.background_executor()
            .spawn(async move {
                warm_up_typst.warm_up();
                warm_up_engine.warm_up();
            })
            .detach();
        let pdf_renderer: Arc<dyn PdfRenderer> = Arc::new(NativePdfRenderer::new());
        let controller = CompilerController::with_debounce(std::time::Duration::from_millis(
            settings.editor.compile_debounce_ms,
        ));
        let sidebar_width = settings
            .layout
            .sidebar_width
            .clamp(*SIDEBAR_WIDTH_RANGE.start(), *SIDEBAR_WIDTH_RANGE.end());
        let preview_width = settings
            .layout
            .preview_width
            .clamp(*PREVIEW_WIDTH_RANGE.start(), *PREVIEW_WIDTH_RANGE.end());
        let diagnostics_height = settings.layout.diagnostics_height.clamp(
            *DIAGNOSTICS_HEIGHT_RANGE.start(),
            *DIAGNOSTICS_HEIGHT_RANGE.end(),
        );

        cx.observe(&editor, |this, editor, cx| {
            this.on_editor_changed(editor, cx);
        })
        .detach();
        cx.observe(&prompt_editor, |this, prompt, cx| {
            this.on_prompt_changed(prompt, cx);
        })
        .detach();
        cx.subscribe(&editor, |this, _, event: &EditorEvent, cx| {
            this.on_editor_event(*event, cx);
        })
        .detach();

        let mut workspace = Self {
            project_tree,
            documents: vec![initial_doc],
            active_doc_idx: 0,
            editor,
            prompt_editor,
            canvas,
            active_view_kind: ActiveViewKind::Editor,
            preview,
            tectonic_compiler,
            typst_compiler,
            pdf_renderer,
            settings,
            controller,
            compile: CompileState {
                task: None,
                running: false,
                pending: false,
                generation: 0,
                cancel: None,
            },
            show_welcome,
            layout: LayoutState {
                sidebar_visible: true,
                sidebar_tab: SidebarTab::Files,
                preview_visible: true,
                diagnostics_drawer_open: false,
                diagnostics_filter: DiagnosticsFilter::All,
                sidebar_width,
                preview_width,
                diagnostics_height,
                resizing_panel: None,
            },
            workspace_menu_open: false,
            latest_diagnostics: Vec::new(),
            workspace_error: open_error,
            project_state: crate::project::state::ProjectState::new(),
            outline_cache: std::cell::RefCell::new(None),
            word_count_cache: std::cell::RefCell::new(None),
            completions: Vec::new(),
            completion_open: false,
            completion_selected: 0,
            find_state: FindState::new(),
            find_bar_open: false,
            prompt_target: state::PromptTarget::Idle,
            active_modal: ActiveModal::None,
            pending_recovery: None,
            history_store: state::CanvasHistoryStore::default(),
            pending_project_dir: None,
        };

        let recovery_dir = workspace
            .project_tree
            .root_path()
            .join(".graf")
            .join("recovery");
        if let Some(journal) =
            crate::project::recovery::RecoveryJournal::load_from_dir(&recovery_dir)
            && !journal.entries.is_empty()
        {
            workspace.pending_recovery = Some(journal);
            workspace.active_modal = ActiveModal::RestoreRecovery;
        }

        workspace.reload_bibtex_and_labels(cx);
        if !workspace.show_welcome {
            workspace.trigger_compile(cx);
        }
        workspace
    }

    pub fn active_engine(&self) -> EngineKind {
        self.active_document()
            .and_then(|document| document.kind().as_engine())
            .unwrap_or(EngineKind::Latex)
    }

    /// One accessor over the active-tab lookup so the dozens of call sites
    /// never re-walk `documents.get(active_doc_idx)` themselves.
    pub(crate) fn active_document(&self) -> Option<&crate::project::document::Document> {
        self.documents.get(self.active_doc_idx)
    }

    pub(crate) fn active_document_kind(&self) -> Option<crate::project::document::DocumentKind> {
        self.active_document().map(|document| document.kind())
    }

    pub fn active_document_is_compilable(&self) -> bool {
        self.active_document()
            .is_some_and(|document| document.kind().is_compilable())
    }

    pub fn editor_focus_handle(&self, cx: &Context<Self>) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }

    pub fn toggle_workspace_menu(&mut self, cx: &mut Context<Self>) {
        self.workspace_menu_open = !self.workspace_menu_open;
        cx.notify();
    }

    pub fn toggle_performance_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = match window.debug_frame_overlay_mode() {
            DebugFrameOverlayMode::Hidden => DebugFrameOverlayMode::Full,
            DebugFrameOverlayMode::Minimal | DebugFrameOverlayMode::Full => {
                DebugFrameOverlayMode::Hidden
            }
        };
        window.set_debug_frame_overlay_mode(mode);
        cx.notify();
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.layout.sidebar_visible = !self.layout.sidebar_visible;
        cx.notify();
    }

    pub fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.layout.preview_visible = !self.layout.preview_visible;
        cx.notify();
    }

    pub fn toggle_diagnostics(&mut self, cx: &mut Context<Self>) {
        self.layout.diagnostics_drawer_open = !self.layout.diagnostics_drawer_open;
        cx.notify();
    }

    pub fn toggle_find(&mut self, cx: &mut Context<Self>) {
        self.find_bar_open = !self.find_bar_open;
        self.prompt_target = if self.find_bar_open {
            state::PromptTarget::Find
        } else {
            state::PromptTarget::Idle
        };
        if self.find_bar_open {
            let query = self
                .editor
                .read(cx)
                .selected_text()
                .unwrap_or_else(|| self.find_state.query.clone());
            let content = self.editor.read(cx).text().to_string();
            self.find_state.set_query(query.clone(), &content);
            self.prompt_editor
                .update(cx, |input, cx| input.set_input_text(query, cx));
        }
        cx.notify();
    }

    pub fn open_settings(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::Settings(tab);
        cx.notify();
    }

    pub fn open_about(&mut self, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::About;
        cx.notify();
    }

    pub fn open_quick_open(&mut self, cx: &mut Context<Self>) {
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text("", cx));
        self.active_modal = ActiveModal::QuickOpen;
        self.prompt_target = state::PromptTarget::QuickOpen;
        cx.notify();
    }

    pub fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text("", cx));
        self.active_modal = ActiveModal::CommandPalette;
        self.prompt_target = state::PromptTarget::Palette;
        cx.notify();
    }

    pub fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.dismiss_context_menu(cx));
        if self.active_modal != ActiveModal::None {
            self.active_modal = ActiveModal::None;
            // A cancelled project scaffold must not leave a stale folder
            // behind for the next template acceptance.
            self.pending_project_dir = None;
        } else if self.completion_open {
            self.completion_open = false;
            self.editor
                .update(cx, |editor, _| editor.set_completion_active(false));
        }
        if self.find_bar_open {
            self.find_bar_open = false;
            self.prompt_target = state::PromptTarget::Idle;
        }
        cx.notify();
    }

    pub fn jump_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.active_view_kind = ActiveViewKind::Editor;
        self.editor.update(cx, |editor, cx| {
            editor.jump_to_line(line, cx);
        });
        cx.notify();
    }
}
