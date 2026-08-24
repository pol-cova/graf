mod commands;
mod diagnostics;
mod editor_panel;
mod find_bar;
mod modals;
mod sidebar;
mod status_bar;
mod top_bar;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use log::{info, warn};

use gpui::{
    Context, DebugFrameOverlayMode, Entity, FocusHandle, Focusable, KeyBinding, MouseMoveEvent,
    MouseUpEvent, PathPromptOptions, Render, Task, Window, actions, div, prelude::*,
};

use crate::ai::diff::DiffReview;
use crate::ai::operations::{AiOperationKind, execute_operation, parse_canvas_response};
use crate::ai::provider::AiProvider;
use crate::canvas::view::CanvasView;
use crate::compiler::EngineKind;
use crate::compiler::controller::CompilerController;
use crate::compiler::diagnostics::Diagnostic;
use crate::compiler::engine::{CompileId, CompileRequest, DocumentEngine};
use crate::compiler::tectonic::TectonicEngine;
use crate::compiler::typst::TypstEngine;
use crate::editor::find::FindState;
use crate::editor::view::{EditorEvent, EditorView};
use crate::preview::renderer::{NativePdfRenderer, PdfRenderer};
use crate::preview::view::PreviewView;
use crate::project::document::Document;
use crate::project::settings::GrafSettings;
use crate::project::tree::ProjectTree;
use crate::ui::theme;

static NEXT_COMPILE_ID: AtomicU64 = AtomicU64::new(1);

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
        AiAssist,
        OpenSettings,
        OpenAbout,
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
    QuickOpen(String),
    CommandPalette(String),
    AiAssist(String),
    DiffReview(DiffReview),
    ConfirmClose(usize),
    Settings(SettingsTab),
    About,
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
    pub(crate) ai_provider: Arc<dyn AiProvider>,
    pub(crate) settings: GrafSettings,
    pub(crate) controller: CompilerController,
    pub(crate) compile_task: Option<Task<()>>,
    pub(crate) sidebar_visible: bool,
    pub(crate) sidebar_tab: SidebarTab,
    pub(crate) preview_visible: bool,
    pub(crate) diagnostics_drawer_open: bool,
    pub(crate) diagnostics_filter: DiagnosticsFilter,
    pub(crate) sidebar_width: f32,
    pub(crate) preview_width: f32,
    pub(crate) diagnostics_height: f32,
    pub(crate) resizing_panel: Option<ResizingPanel>,
    pub(crate) workspace_menu_open: bool,
    pub(crate) latest_diagnostics: Vec<Diagnostic>,
    pub(crate) workspace_error: Option<String>,
    pub(crate) bib_index: crate::project::bibtex::BibtexIndex,
    pub(crate) label_index: crate::project::bibtex::LabelIndex,
    pub(crate) completions: Vec<crate::editor::completion::CompletionItem>,
    pub(crate) completion_open: bool,
    pub(crate) completion_selected: usize,
    pub(crate) find_state: FindState,
    pub(crate) find_bar_open: bool,
    pub(crate) active_modal: ActiveModal,
}

impl Workspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_tree = ProjectTree::scan(&current_dir);

        let initial_text = "\\documentclass{article}\n\\title{Untitled}\n\\author{}\n\n\\begin{document}\n\\maketitle\n\n\\section{Introduction}\nStart writing here.\n\n\\end{document}\n";

        let initial_doc = if let Some(root_doc) = project_tree.root_document() {
            Document::open(root_doc)
                .unwrap_or_else(|_| Document::new_untitled("main.tex", initial_text))
        } else {
            Document::new_untitled("main.tex", initial_text)
        };

        let settings = GrafSettings::load_default();
        let editor_settings = settings.editor.clone();
        let is_typst = initial_doc.title().ends_with(".typ");
        let is_plain_text = !is_typst && !initial_doc.title().ends_with(".tex");
        let initial_content = initial_doc.buffer().content().to_string();
        let editor = cx.new(|cx| {
            let mut editor = EditorView::with_text(cx, initial_content);
            editor.is_typst = is_typst;
            editor.set_plain_text(is_plain_text, cx);
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
            editor.set_plain_text(true, cx);
            editor.set_single_line(true);
            editor.set_preferences(13.0, 2, false, cx);
            editor
        });
        let canvas = cx.new(CanvasView::new);
        let preview = cx.new(|_cx| PreviewView::new());
        let tectonic_compiler: Arc<dyn DocumentEngine> = Arc::new(TectonicEngine::new());
        let typst_compiler: Arc<dyn DocumentEngine> = Arc::new(TypstEngine::new());
        let pdf_renderer: Arc<dyn PdfRenderer> = Arc::new(NativePdfRenderer::new());
        let ai_provider: Arc<dyn AiProvider> = crate::ai::provider::create_default_provider();
        let controller = CompilerController::with_debounce(std::time::Duration::from_millis(
            settings.editor.compile_debounce_ms,
        ));
        let sidebar_width = settings.layout.sidebar_width.clamp(160.0, 420.0);
        let preview_width = settings.layout.preview_width.clamp(320.0, 800.0);
        let diagnostics_height = settings.layout.diagnostics_height.clamp(100.0, 500.0);

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
            ai_provider,
            settings,
            controller,
            compile_task: None,
            sidebar_visible: true,
            sidebar_tab: SidebarTab::Files,
            preview_visible: true,
            diagnostics_drawer_open: false,
            diagnostics_filter: DiagnosticsFilter::All,
            sidebar_width,
            preview_width,
            diagnostics_height,
            resizing_panel: None,
            workspace_menu_open: false,
            latest_diagnostics: Vec::new(),
            workspace_error: None,
            bib_index: crate::project::bibtex::BibtexIndex::new(),
            label_index: crate::project::bibtex::LabelIndex::default(),
            completions: Vec::new(),
            completion_open: false,
            completion_selected: 0,
            find_state: FindState::new(),
            find_bar_open: false,
            active_modal: ActiveModal::None,
        };

        workspace.reload_bibtex_and_labels(cx);
        workspace.trigger_compile(cx);
        workspace
    }

    pub fn active_engine(&self) -> EngineKind {
        if let Some(doc) = self.documents.get(self.active_doc_idx)
            && doc.title().ends_with(".typ")
        {
            return EngineKind::Typst;
        }
        EngineKind::Latex
    }

    pub fn active_document_is_compilable(&self) -> bool {
        self.documents
            .get(self.active_doc_idx)
            .is_some_and(|document| {
                document.title().ends_with(".tex") || document.title().ends_with(".typ")
            })
    }

    pub fn trigger_autocomplete(&mut self, cx: &mut Context<Self>) {
        if !self.documents[self.active_doc_idx]
            .title()
            .ends_with(".tex")
        {
            self.completions.clear();
            self.completion_open = false;
            self.editor
                .update(cx, |editor, _| editor.set_completion_active(false));
            cx.notify();
            return;
        }

        let (text, cursor) = {
            let ed = self.editor.read(cx);
            (ed.text().to_string(), ed.cursor_offset())
        };
        self.completions = crate::editor::completion::compute_completions(
            &text,
            cursor,
            &self.bib_index,
            &self.label_index,
        );
        self.completions.truncate(8);
        self.completion_open = !self.completions.is_empty();
        self.completion_selected = 0;
        self.editor.update(cx, |editor, _| {
            editor.set_completion_active(self.completion_open);
        });
        cx.notify();
    }

    pub fn apply_completion(
        &mut self,
        item: &crate::editor::completion::CompletionItem,
        cx: &mut Context<Self>,
    ) {
        let insert = item.insert_text.clone();
        self.editor.update(cx, |ed, cx| {
            ed.insert_snippet(&insert, cx);
        });
        self.completion_open = false;
        self.editor
            .update(cx, |editor, _| editor.set_completion_active(false));
        cx.notify();
    }

    fn on_editor_event(&mut self, event: EditorEvent, cx: &mut Context<Self>) {
        match event {
            EditorEvent::NextCompletion => {
                if !self.completions.is_empty() {
                    self.completion_selected =
                        (self.completion_selected + 1) % self.completions.len();
                }
            }
            EditorEvent::PreviousCompletion => {
                if !self.completions.is_empty() {
                    self.completion_selected = self
                        .completion_selected
                        .checked_sub(1)
                        .unwrap_or(self.completions.len() - 1);
                }
            }
            EditorEvent::AcceptCompletion => {
                if let Some(item) = self.completions.get(self.completion_selected).cloned() {
                    self.apply_completion(&item, cx);
                    return;
                }
            }
            EditorEvent::FindReferences => {
                self.find_all_references(cx);
                return;
            }
        }
        cx.notify();
    }

    fn find_all_references(&mut self, cx: &mut Context<Self>) {
        let Some(reference) = self.editor.read(cx).reference_at_cursor() else {
            return;
        };
        let content = self.editor.read(cx).text().to_string();
        self.find_state.set_query(reference.clone(), &content);
        self.find_bar_open = true;
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text(reference, cx));
        if let Some(matched) = self.find_state.next_match().cloned() {
            self.editor
                .update(cx, |editor, cx| editor.select_range(matched, cx));
        }
        cx.notify();
    }

    pub fn editor_focus_handle(&self, cx: &Context<Self>) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }

    fn persist_settings(&self) {
        let Some(path) = GrafSettings::default_path() else {
            return;
        };
        if let Err(error) = self.settings.save_to_path(&path) {
            warn!("failed to save settings to {}: {error}", path.display());
        }
    }

    fn apply_editor_settings(&mut self, cx: &mut Context<Self>) {
        let editor = &self.settings.editor;
        self.editor.update(cx, |view, cx| {
            view.set_preferences(editor.font_size, editor.tab_size, editor.line_numbers, cx);
        });
        self.controller
            .set_debounce_duration(std::time::Duration::from_millis(editor.compile_debounce_ms));
        self.persist_settings();
        cx.notify();
    }

    pub fn adjust_editor_font_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.settings.editor.font_size = (self.settings.editor.font_size + delta).clamp(10.0, 24.0);
        self.apply_editor_settings(cx);
    }

    pub fn cycle_tab_size(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.tab_size = match self.settings.editor.tab_size {
            1 | 2 => 4,
            3..=7 => 8,
            _ => 2,
        };
        self.apply_editor_settings(cx);
    }

    pub fn toggle_line_numbers_setting(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.line_numbers = !self.settings.editor.line_numbers;
        self.apply_editor_settings(cx);
    }

    pub fn toggle_auto_compile_setting(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.auto_compile = !self.settings.editor.auto_compile;
        self.apply_editor_settings(cx);
    }

    pub fn cycle_compile_debounce(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.compile_debounce_ms = match self.settings.editor.compile_debounce_ms {
            0..=150 => 300,
            151..=300 => 500,
            301..=500 => 750,
            _ => 150,
        };
        self.apply_editor_settings(cx);
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
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    pub fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_visible = !self.preview_visible;
        cx.notify();
    }

    pub fn begin_panel_resize(&mut self, panel: ResizingPanel, cx: &mut Context<Self>) {
        self.resizing_panel = Some(panel);
        cx.notify();
    }

    fn resize_panel(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }

        match self.resizing_panel {
            Some(ResizingPanel::Sidebar) => {
                self.sidebar_width = event.position.x.as_f32().clamp(160.0, 420.0);
            }
            Some(ResizingPanel::Preview) => {
                self.preview_width = (window.viewport_size().width - event.position.x)
                    .as_f32()
                    .clamp(320.0, 800.0);
            }
            Some(ResizingPanel::Diagnostics) => {
                self.diagnostics_height = (window.viewport_size().height - event.position.y)
                    .as_f32()
                    .clamp(100.0, 500.0);
            }
            None => return,
        }
        cx.notify();
    }

    fn finish_panel_resize(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.resizing_panel.take().is_some() {
            self.settings.layout.sidebar_width = self.sidebar_width;
            self.settings.layout.preview_width = self.preview_width;
            self.settings.layout.diagnostics_height = self.diagnostics_height;
            self.persist_settings();
            cx.notify();
        }
    }

    pub fn toggle_diagnostics(&mut self, cx: &mut Context<Self>) {
        self.diagnostics_drawer_open = !self.diagnostics_drawer_open;
        cx.notify();
    }

    pub fn toggle_find(&mut self, cx: &mut Context<Self>) {
        self.find_bar_open = !self.find_bar_open;
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

    pub fn open_ai_assist(&mut self, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::AiAssist(String::new());
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

    pub fn new_typst_document(&mut self, cx: &mut Context<Self>) {
        let initial_typst = "= Untitled\n\nStart writing here.\n";
        let doc_name = format!("document-{}.typ", self.documents.len() + 1);
        let doc = Document::new_untitled(&doc_name, initial_typst);
        self.documents.push(doc);
        self.active_doc_idx = self.documents.len() - 1;
        self.active_view_kind = ActiveViewKind::Editor;
        self.editor.update(cx, |ed, cx| {
            ed.set_text(initial_typst, cx);
            ed.set_is_typst(true, cx);
            ed.set_plain_text(false, cx);
        });
        cx.notify();
        self.trigger_compile(cx);
    }

    pub fn run_ai_operation(&mut self, op: AiOperationKind, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().to_string();

        match &op {
            AiOperationKind::GenerateDiagram { prompt } => {
                let provider = self.ai_provider.clone();
                let prompt_clone = prompt.clone();
                let resp_res = execute_operation(
                    provider.as_ref(),
                    &AiOperationKind::GenerateDiagram {
                        prompt: prompt_clone,
                    },
                    "",
                );
                if let Ok(resp) = resp_res
                    && let Ok(doc) = parse_canvas_response(&resp)
                {
                    let json = doc.to_json().unwrap_or_default();
                    let title = format!("ai-diagram-{}.graf", self.documents.len() + 1);
                    let new_doc = Document::new_untitled(&title, json.clone());
                    self.documents.push(new_doc);
                    self.active_doc_idx = self.documents.len() - 1;
                    self.active_view_kind = ActiveViewKind::Canvas;
                    self.canvas.update(cx, |c, cx| {
                        let _ = c.load_from_json(&json, cx);
                    });
                }
                self.active_modal = ActiveModal::None;
                cx.notify();
            }
            _ => {
                let provider = self.ai_provider.clone();
                if let Ok(replacement) = execute_operation(provider.as_ref(), &op, &text) {
                    let review = DiffReview::new(op.label(), text, replacement);
                    self.active_modal = ActiveModal::DiffReview(review);
                } else {
                    self.active_modal = ActiveModal::None;
                }
                cx.notify();
            }
        }
    }

    pub fn accept_diff_review(&mut self, cx: &mut Context<Self>) {
        if let ActiveModal::DiffReview(review) = &self.active_modal {
            let repl = review.replacement.clone();
            self.editor.update(cx, |ed, cx| {
                ed.set_text(repl, cx);
            });
            self.active_modal = ActiveModal::None;
            self.trigger_compile(cx);
            cx.notify();
        }
    }

    pub fn new_canvas_diagram(&mut self, cx: &mut Context<Self>) {
        let default_canvas_json = match self.canvas.read(cx).save_to_json() {
            Ok(json) => json,
            Err(error) => {
                self.workspace_error = Some(format!("Could not create diagram: {error}"));
                cx.notify();
                return;
            }
        };
        let doc_name = format!("diagram-{}.graf", self.documents.len() + 1);
        let doc = Document::new_untitled(&doc_name, default_canvas_json);
        self.documents.push(doc);
        self.active_doc_idx = self.documents.len() - 1;
        self.active_view_kind = ActiveViewKind::Canvas;
        cx.notify();
    }

    pub fn insert_table_template(&mut self, cx: &mut Context<Self>) {
        let is_typst = self.documents[self.active_doc_idx]
            .title()
            .ends_with(".typ");
        let mut table = crate::editor::table::TableData::new(3, 3);
        table.rows[0] = vec![
            "Column 1".to_string(),
            "Column 2".to_string(),
            "Column 3".to_string(),
        ];

        let table_code = if is_typst {
            table.to_typst()
        } else {
            table.to_latex()
        };

        self.editor.update(cx, |editor, cx| {
            editor.insert_snippet(&table_code, cx);
        });
        self.sync_active_doc_from_editor(cx);
        self.trigger_compile(cx);
    }

    pub fn export_canvas_to_tikz(&mut self, cx: &mut Context<Self>) {
        let doc = self.canvas.read(cx).document();
        let tikz_code = crate::canvas::tikz::export_to_tikz(doc);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(tikz_code));
    }

    pub fn export_canvas_to_svg(&mut self, cx: &mut Context<Self>) {
        let doc = self.canvas.read(cx).document();
        let svg_code = crate::canvas::svg::export_to_svg(doc);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(svg_code));
    }

    pub fn lint_academic_style(&mut self, cx: &mut Context<Self>) {
        let is_typst = self.documents[self.active_doc_idx]
            .title()
            .ends_with(".typ");
        let text = self.editor.read(cx).text();
        let warnings = crate::project::linter::lint_academic_text(text, is_typst);

        let mut diags = Vec::new();
        for (i, w) in warnings.into_iter().enumerate() {
            diags.push(crate::compiler::diagnostics::Diagnostic {
                id: crate::compiler::diagnostics::DiagnosticId(1000 + i as u64),
                severity: crate::compiler::diagnostics::Severity::Warning,
                source: crate::compiler::diagnostics::DiagnosticSource::Parser,
                file: None,
                line: Some(w.line),
                message: w.message,
            });
        }

        if !diags.is_empty() {
            self.latest_diagnostics = diags.clone();
            self.diagnostics_drawer_open = true;
            self.editor.update(cx, |editor, cx| {
                editor.set_diagnostics(diags, cx);
            });
        }
        cx.notify();
    }

    pub fn sync_zotero_library(&mut self, cx: &mut Context<Self>) {
        let zotero_lib = crate::project::zotero::ZoteroLibrary::scan_local_storage();
        for item in zotero_lib.items {
            self.bib_index.add_entry(item.to_bib_entry());
        }
        cx.notify();
    }

    pub fn scan_plugins(&mut self, cx: &mut Context<Self>) {
        let mut host = crate::plugins::host::PluginHost::new();
        let _ = host.scan_plugin_directory();
        cx.notify();
    }

    pub fn open_quick_open(&mut self, cx: &mut Context<Self>) {
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text("", cx));
        self.active_modal = ActiveModal::QuickOpen(String::new());
        cx.notify();
    }

    pub fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text("", cx));
        self.active_modal = ActiveModal::CommandPalette(String::new());
        cx.notify();
    }

    pub fn close_modal(&mut self, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.dismiss_context_menu(cx));
        if self.active_modal != ActiveModal::None {
            self.active_modal = ActiveModal::None;
        } else if self.completion_open {
            self.completion_open = false;
            self.editor
                .update(cx, |editor, _| editor.set_completion_active(false));
        } else if self.find_bar_open {
            self.find_bar_open = false;
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

    pub fn open_file_picker(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open".into()),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                for path in paths {
                    this.update(cx, |this, cx| this.open_file(path, cx)).ok();
                }
            }
            Ok(Err(error)) => {
                this.update(cx, |this, cx| {
                    this.workspace_error = Some(format!("Could not open file picker: {error}"));
                    cx.notify();
                })
                .ok();
            }
            _ => {}
        })
        .detach();
    }

    pub fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.sync_active_doc_from_editor(cx);

        if let Some(idx) = self
            .documents
            .iter()
            .position(|doc| doc.path() == Some(&path))
        {
            self.switch_tab(idx, cx);
            return;
        }

        let doc = match Document::open(&path) {
            Ok(doc) => doc,
            Err(error) => {
                self.workspace_error = Some(format!(
                    "Could not open {}: {error}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("file")
                ));
                cx.notify();
                return;
            }
        };

        let is_canvas = doc.title().ends_with(".graf");
        let is_typst = doc.title().ends_with(".typ");
        let is_plain_text = !is_typst && !doc.title().ends_with(".tex");
        let content = doc.buffer().content().to_string();
        self.documents.push(doc);
        self.active_doc_idx = self.documents.len() - 1;
        self.workspace_error = None;

        if is_canvas {
            self.active_view_kind = ActiveViewKind::Canvas;
            if let Err(error) = self
                .canvas
                .update(cx, |canvas, cx| canvas.load_from_json(&content, cx))
            {
                self.workspace_error = Some(error);
            }
        } else {
            self.active_view_kind = ActiveViewKind::Editor;
            self.editor.update(cx, |editor, cx| {
                editor.set_text(content, cx);
                editor.set_is_typst(is_typst, cx);
                editor.set_plain_text(is_plain_text, cx);
            });
        }
        cx.notify();
        self.trigger_compile(cx);
    }

    pub fn switch_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.documents.len() || idx == self.active_doc_idx {
            return;
        }
        self.sync_active_doc_from_editor(cx);
        self.active_doc_idx = idx;

        let is_canvas = self.documents[idx].title().ends_with(".graf");
        let is_typst = self.documents[idx].title().ends_with(".typ");
        let is_plain_text = !is_typst && !self.documents[idx].title().ends_with(".tex");
        if is_canvas {
            self.active_view_kind = ActiveViewKind::Canvas;
            let content = self.documents[idx].buffer().content().to_string();
            if let Err(error) = self
                .canvas
                .update(cx, |canvas, cx| canvas.load_from_json(&content, cx))
            {
                self.workspace_error = Some(error);
            }
        } else {
            self.active_view_kind = ActiveViewKind::Editor;
            let content = self.documents[idx].buffer().content().to_string();
            self.editor.update(cx, |editor, cx| {
                editor.set_text(content, cx);
                editor.set_is_typst(is_typst, cx);
                editor.set_plain_text(is_plain_text, cx);
            });
        }
        cx.notify();
        self.trigger_compile(cx);
    }

    pub fn close_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.documents.len() || self.documents.len() <= 1 {
            return;
        }
        self.sync_active_doc_from_editor(cx);
        if self.documents[idx].is_dirty() {
            self.active_modal = ActiveModal::ConfirmClose(idx);
            cx.notify();
            return;
        }
        self.force_close_tab(idx, cx);
    }

    pub fn force_close_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.documents.len() || self.documents.len() <= 1 {
            return;
        }
        self.documents.remove(idx);
        if self.active_doc_idx >= self.documents.len() {
            self.active_doc_idx = self.documents.len() - 1;
        }

        let is_canvas = self.documents[self.active_doc_idx]
            .title()
            .ends_with(".graf");
        let is_typst = self.documents[self.active_doc_idx]
            .title()
            .ends_with(".typ");
        let is_plain_text = !is_typst
            && !self.documents[self.active_doc_idx]
                .title()
                .ends_with(".tex");

        if is_canvas {
            self.active_view_kind = ActiveViewKind::Canvas;
            let content = self.documents[self.active_doc_idx]
                .buffer()
                .content()
                .to_string();
            if let Err(error) = self
                .canvas
                .update(cx, |canvas, cx| canvas.load_from_json(&content, cx))
            {
                self.workspace_error = Some(error);
            }
        } else {
            self.active_view_kind = ActiveViewKind::Editor;
            let content = self.documents[self.active_doc_idx]
                .buffer()
                .content()
                .to_string();
            self.editor.update(cx, |editor, cx| {
                editor.set_text(content, cx);
                editor.set_is_typst(is_typst, cx);
                editor.set_plain_text(is_plain_text, cx);
            });
        }
        cx.notify();
        self.trigger_compile(cx);
    }

    pub fn save_active_document(&mut self, cx: &mut Context<Self>) {
        self.sync_active_doc_from_editor(cx);
        let Some(doc) = self.documents.get(self.active_doc_idx) else {
            return;
        };
        if doc.path().is_none() {
            self.prompt_save_as(cx);
            return;
        }

        let svg = (self.active_view_kind == ActiveViewKind::Canvas)
            .then(|| self.canvas.read(cx).export_svg());
        let result = self.documents[self.active_doc_idx]
            .save()
            .map_err(|error| error.to_string())
            .and_then(|_| {
                if let Some(svg) = svg {
                    let path = self.documents[self.active_doc_idx]
                        .path()
                        .ok_or_else(|| "saved document has no path".to_string())?
                        .with_extension("svg");
                    std::fs::write(path, svg).map_err(|error| error.to_string())?;
                }
                Ok(())
            });

        match result {
            Ok(()) => {
                self.workspace_error = None;
                self.save_recovery_snapshot();
                if self.settings.editor.auto_compile {
                    self.trigger_compile(cx);
                }
            }
            Err(error) => self.workspace_error = Some(format!("Could not save file: {error}")),
        }
        cx.notify();
    }

    fn prompt_save_as(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.documents.get(self.active_doc_idx) else {
            return;
        };
        let document_id = doc.id();
        let suggested_name = doc.title().to_string();
        let receiver =
            cx.prompt_for_new_path(self.project_tree.root_path(), Some(suggested_name.as_str()));

        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(path))) => {
                this.update(cx, |this, cx| {
                    let Some(document) = this
                        .documents
                        .iter_mut()
                        .find(|document| document.id() == document_id)
                    else {
                        return;
                    };
                    match document.save_as(path) {
                        Ok(()) => {
                            this.workspace_error = None;
                            this.save_recovery_snapshot();
                        }
                        Err(error) => {
                            this.workspace_error = Some(format!("Could not save file: {error}"));
                        }
                    }
                    cx.notify();
                })
                .ok();
            }
            Ok(Err(error)) => {
                this.update(cx, |this, cx| {
                    this.workspace_error = Some(format!("Could not open save dialog: {error}"));
                    cx.notify();
                })
                .ok();
            }
            _ => {}
        })
        .detach();
    }

    pub fn save_recovery_snapshot(&self) {
        let entries: Vec<crate::project::recovery::RecoveryEntry> = self
            .documents
            .iter()
            .filter(|d| d.is_dirty())
            .map(|d| {
                crate::project::recovery::RecoveryEntry::new(
                    d.title(),
                    d.path().map(Path::to_path_buf),
                    d.buffer().content(),
                )
            })
            .collect();

        let recovery_dir = self.project_tree.root_path().join(".graf").join("recovery");
        let result = if entries.is_empty() {
            crate::project::recovery::RecoveryJournal::clear_dir(&recovery_dir)
        } else {
            crate::project::recovery::RecoveryJournal::new(entries)
                .save_to_dir(&recovery_dir)
                .map(|_| ())
        };
        if let Err(error) = result {
            warn!("failed to update recovery journal: {error}");
        }
    }

    fn sync_active_doc_from_editor(&mut self, cx: &Context<Self>) {
        if self.active_view_kind == ActiveViewKind::Canvas {
            let json = match self.canvas.read(cx).save_to_json() {
                Ok(json) => json,
                Err(error) => {
                    self.workspace_error = Some(format!("Could not serialize diagram: {error}"));
                    return;
                }
            };
            if let Some(doc) = self
                .documents
                .get_mut(self.active_doc_idx)
                .filter(|doc| doc.buffer().content() != json)
            {
                doc.buffer_mut().replace_all(json);
            }
            return;
        }

        let text = self.editor.read(cx).text().to_string();
        if let Some(doc) = self
            .documents
            .get_mut(self.active_doc_idx)
            .filter(|doc| doc.buffer().content() != text)
        {
            doc.buffer_mut().replace_all(text);
        }
    }

    pub fn reload_bib_files(&mut self) {
        let root = self.project_tree.root_path();
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|ext| ext == "bib") {
                    let content_res = std::fs::read_to_string(&p);
                    if let Ok(content) = content_res {
                        self.bib_index.parse_and_load(&content);
                    }
                }
            }
        }
    }

    pub fn reload_editor_labels(&mut self, cx: &Context<Self>) {
        let editor_text = self.editor.read(cx).text();
        self.label_index.parse_and_load(editor_text);
    }

    pub fn reload_bibtex_and_labels(&mut self, cx: &Context<Self>) {
        self.reload_bib_files();
        self.reload_editor_labels(cx);
    }

    fn on_prompt_changed(&mut self, prompt: Entity<EditorView>, cx: &mut Context<Self>) {
        let raw_query = prompt.read(cx).text().to_string();
        let submitted = raw_query.contains('\n');
        let query = raw_query.replace('\n', "");

        if self.find_bar_open {
            let content = self.editor.read(cx).text().to_string();
            self.find_state.set_query(query.clone(), &content);
        }

        if submitted {
            if self.find_bar_open
                && let Some(matched) = self.find_state.next_match().cloned()
            {
                self.editor
                    .update(cx, |editor, cx| editor.select_range(matched, cx));
            }

            match self.active_modal.clone() {
                ActiveModal::QuickOpen(_) => {
                    let query = query.to_lowercase();
                    if let Some(path) = self.project_tree.file_paths().into_iter().find(|path| {
                        path.strip_prefix(self.project_tree.root_path())
                            .unwrap_or(path)
                            .display()
                            .to_string()
                            .to_lowercase()
                            .contains(&query)
                    }) {
                        self.active_modal = ActiveModal::None;
                        self.open_file(path, cx);
                    }
                }
                ActiveModal::CommandPalette(_) => {
                    let query = query.to_lowercase();
                    if let Some(command) = commands::all_commands().iter().find(|command| {
                        command.title.to_lowercase().contains(&query)
                            || command.category.to_lowercase().contains(&query)
                    }) {
                        self.active_modal = ActiveModal::None;
                        self.dispatch_command_action(command.id, cx);
                    }
                }
                _ => {}
            }
            self.prompt_editor
                .update(cx, |input, cx| input.set_input_text(query, cx));
        }

        cx.notify();
    }

    fn on_editor_changed(&mut self, editor: Entity<EditorView>, cx: &mut Context<Self>) {
        let rev = editor.read(cx).revision();
        self.sync_active_doc_from_editor(cx);
        self.reload_editor_labels(cx);
        self.trigger_autocomplete(cx);

        if self.active_document_is_compilable()
            && rev > self.controller.current_revision()
            && self.settings.editor.auto_compile
        {
            self.controller.on_source_edited(rev, Instant::now());
            cx.notify();

            let debounce = self.controller.debounce_duration();
            self.compile_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(debounce).await;
                this.update(cx, |this, cx| {
                    this.trigger_compile(cx);
                })
                .ok();
            }));
        }
    }

    pub fn trigger_compile(&mut self, cx: &mut Context<Self>) {
        if !self.active_document_is_compilable() {
            self.controller.reset();
            self.latest_diagnostics.clear();
            self.preview.update(cx, |preview, cx| preview.clear(cx));
            cx.notify();
            return;
        }

        self.preview
            .update(cx, |preview, cx| preview.set_rendering(cx));
        let (rev, text) = {
            let ed = self.editor.read(cx);
            (ed.revision(), ed.text().to_string())
        };

        let compile_id = CompileId(NEXT_COMPILE_ID.fetch_add(1, Ordering::Relaxed));
        self.controller.begin_compile(compile_id, rev);
        cx.notify();

        let engine = self.active_engine();
        let compiler = if engine == EngineKind::Typst {
            self.typst_compiler.clone()
        } else {
            self.tectonic_compiler.clone()
        };

        let pdf_renderer = self.pdf_renderer.clone();

        let project_root = Some(self.project_tree.root_path().to_path_buf());
        let root_document = self
            .project_tree
            .root_document()
            .map(Path::to_path_buf)
            .or_else(|| {
                self.documents
                    .get(self.active_doc_idx)
                    .and_then(|d| d.path().map(Path::to_path_buf))
            });

        let request = CompileRequest::with_project(text, rev, project_root, root_document);

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { compiler.compile(request) })
                .await;

            match &result {
                Ok(output) => info!(
                    "compile finished for revision {} in {:.0}ms",
                    output.revision,
                    output.duration.as_secs_f64() * 1000.0
                ),
                Err(error) => warn!("compile failed for revision {}: {}", error.revision, error),
            }

            match result {
                Ok(output) => {
                    let pdf_bytes = output.artifact.clone();
                    let output_rev = output.revision;
                    let diags = output.diagnostics.clone();

                    let render_result = cx
                        .background_executor()
                        .spawn(async move { pdf_renderer.render_document(output_rev, &pdf_bytes) })
                        .await;
                    match &render_result {
                        Ok(pages) => info!(
                            "preview rendered for revision {output_rev} with {} page(s)",
                            pages.len()
                        ),
                        Err(error) => {
                            warn!("preview render failed for revision {output_rev}: {error}")
                        }
                    }

                    this.update(cx, |this, cx| {
                        let _ = this.controller.handle_output(output);
                        this.latest_diagnostics = diags.clone();
                        this.editor.update(cx, |editor, cx| {
                            editor.set_diagnostics(diags, cx);
                        });

                        if let Ok(pages) = render_result {
                            this.preview.update(cx, |preview, cx| {
                                preview.set_rendered_pages(pages, cx);
                            });
                        }
                        cx.notify();
                    })
                    .ok();
                }
                Err(err) => {
                    let err_summary = Some(err.message.clone());
                    let diags = err.diagnostics.clone();

                    this.update(cx, |this, cx| {
                        let _ = this.controller.handle_error(err);
                        this.latest_diagnostics = diags.clone();
                        this.editor.update(cx, |editor, cx| {
                            editor.set_diagnostics(diags, cx);
                        });
                        this.preview.update(cx, |preview, cx| {
                            preview.set_compile_failed(err_summary, cx);
                        });
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub fn dispatch_command_action(&mut self, id: u32, cx: &mut Context<Self>) {
        match id {
            1 => self.trigger_compile(cx),
            2 => self.save_active_document(cx),
            3 => self.toggle_find(cx),
            4 => self.toggle_sidebar(cx),
            5 => self.toggle_preview(cx),
            6 => self.toggle_diagnostics(cx),
            7 => {
                let active = self.active_doc_idx;
                self.close_tab(active, cx);
            }
            8 => self.new_canvas_diagram(cx),
            9 => self.open_ai_assist(cx),
            10 => self.open_settings(SettingsTab::Editor, cx),
            11 => self.new_typst_document(cx),
            12 => self.open_about(cx),
            13 => {
                self.save_recovery_snapshot();
                self.open_settings(SettingsTab::Build, cx);
            }
            14 => self.insert_table_template(cx),
            15 => self.export_canvas_to_tikz(cx),
            16 => self.export_canvas_to_svg(cx),
            17 => self.lint_academic_style(cx),
            18 => self.sync_zotero_library(cx),
            21 => self.scan_plugins(cx),
            _ => {}
        }
    }

    fn on_focus_editor(&mut self, _: &FocusEditor, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
    }

    fn on_compile(&mut self, _: &Compile, _window: &mut Window, cx: &mut Context<Self>) {
        self.trigger_compile(cx);
    }

    fn on_open_file(&mut self, _: &OpenFile, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_file_picker(cx);
    }

    fn on_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_active_document(cx);
    }

    fn on_close_tab(&mut self, _: &CloseTab, _window: &mut Window, cx: &mut Context<Self>) {
        let active = self.active_doc_idx;
        self.close_tab(active, cx);
    }

    fn on_toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    fn on_toggle_preview(
        &mut self,
        _: &TogglePreview,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_preview(cx);
    }

    fn on_toggle_diagnostics(
        &mut self,
        _: &ToggleDiagnostics,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_diagnostics(cx);
    }

    fn on_toggle_find(&mut self, _: &ToggleFind, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_find(cx);
        if self.find_bar_open {
            window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
    }

    fn on_quick_open(&mut self, _: &QuickOpen, window: &mut Window, cx: &mut Context<Self>) {
        self.open_quick_open(cx);
        window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
    }

    fn on_command_palette(
        &mut self,
        _: &CommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(cx);
        window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
    }

    fn on_ai_assist(&mut self, _: &AiAssist, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_ai_assist(cx);
    }

    fn on_open_settings(&mut self, _: &OpenSettings, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(SettingsTab::Editor, cx);
    }

    fn on_open_about(&mut self, _: &OpenAbout, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_about(cx);
    }

    fn on_close_modal(&mut self, _: &CloseModal, window: &mut Window, cx: &mut Context<Self>) {
        self.close_modal(cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
    }

    fn on_autocomplete(&mut self, _: &Autocomplete, _window: &mut Window, cx: &mut Context<Self>) {
        self.trigger_autocomplete(cx);
    }

    fn on_toggle_performance_overlay(
        &mut self,
        _: &TogglePerformanceOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_performance_overlay(window, cx);
    }
}

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
            .on_action(cx.listener(Self::on_ai_assist))
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
