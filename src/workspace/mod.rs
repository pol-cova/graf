//! Workspace shell — the top-level layout for Graf.
//!
//! Features:
//! - Multi-Engine compilation pipeline (LaTeX via Tectonic & Typst)
//! - Project files tree & Document Outline navigation
//! - Multi-document tabs with dirty tracking and Save
//! - Native Vector Canvas View for `.graf` diagramming and auto SVG export
//! - AI Technical Writing Assistant (⌘I) over Agent Client Protocol (ACP)
//! - Interactive Settings Preferences Window (⌘,)
//! - In-Editor Find & Replace bar (⌘F)
//! - Quick Open modal (⌘P)
//! - Command Palette (⌘K)
//! - Syntax-highlighted editor with click-to-jump error problems drawer
//! - Native PDF preview with zoom toolbar & 2x Retina rendering

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

use gpui::{
    Context, Entity, FocusHandle, Focusable, KeyBinding, Render, Task, Window, actions, div,
    prelude::*,
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
use crate::editor::view::EditorView;
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
    ]
);

/// Registers workspace-level keybindings.
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
            ("cmd-s", Save),
            ("cmd-w", CloseTab),
            ("cmd-f", ToggleFind),
            ("cmd-p", QuickOpen),
            ("cmd-k", CommandPalette),
            ("cmd-i", AiAssist),
            ("cmd-,", OpenSettings),
            ("ctrl-space", Autocomplete),
            ("cmd-shift-e", ToggleSidebar),
            ("cmd-shift-p", TogglePreview),
            ("cmd-shift-m", ToggleDiagnostics),
            ("escape", CloseModal),
        ]
    );
}

/// Sidebar view mode: Files vs Document Outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Files,
    Outline,
}

/// Settings window tab selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Editor,
    Ai,
    Canvas,
    Licenses,
}

/// Diagnostics drawer filter selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticsFilter {
    All,
    Errors,
    Warnings,
}

/// Active center panel view mode: Text Editor vs Vector Canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveViewKind {
    Editor,
    Canvas,
}

/// Active modal dialog in the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveModal {
    None,
    QuickOpen(String),
    CommandPalette(String),
    AiAssist(String),
    DiffReview(DiffReview),
    Settings(SettingsTab),
}

/// The root workspace view composing the top bar, three-panel body, modals, and status bar.
pub struct Workspace {
    pub(crate) project_tree: ProjectTree,
    pub(crate) documents: Vec<Document>,
    pub(crate) active_doc_idx: usize,
    pub(crate) editor: Entity<EditorView>,
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
    pub(crate) latest_diagnostics: Vec<Diagnostic>,
    pub(crate) bib_index: crate::project::bibtex::BibtexIndex,
    pub(crate) label_index: crate::project::bibtex::LabelIndex,
    pub(crate) completions: Vec<crate::editor::completion::CompletionItem>,
    pub(crate) completion_open: bool,
    pub(crate) find_state: FindState,
    pub(crate) find_bar_open: bool,
    pub(crate) active_modal: ActiveModal,
}

impl Workspace {
    /// Creates a new workspace instance initialized with current working directory.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let project_tree = ProjectTree::scan(&current_dir);

        let initial_text = "\\documentclass{article}\n\\usepackage{graphicx}\n\\title{Hello Graf}\n\\author{Graf User}\n\\begin{document}\n\\maketitle\n\nHello from Graf — fast native LaTeX & Typst workspace with built-in vector diagramming, ACP assistant, and custom settings.\n\n\\section{Architecture}\nNative canvas drawings export to SVG and compile instantly.\n\n\\begin{equation}\n    E = mc^2\n\\end{equation}\n\n\\end{document}\n";

        let initial_doc = if let Some(root_doc) = project_tree.root_document() {
            Document::open(root_doc)
                .unwrap_or_else(|_| Document::new_untitled("main.tex", initial_text))
        } else {
            Document::new_untitled("main.tex", initial_text)
        };

        let is_typst = initial_doc.title().ends_with(".typ");
        let initial_content = initial_doc.buffer().content().to_string();
        let editor = cx.new(|cx| {
            let mut ed = EditorView::with_text(cx, initial_content);
            ed.is_typst = is_typst;
            ed
        });
        let canvas = cx.new(CanvasView::new);
        let preview = cx.new(|_cx| PreviewView::new());
        let tectonic_compiler: Arc<dyn DocumentEngine> = Arc::new(TectonicEngine::new());
        let typst_compiler: Arc<dyn DocumentEngine> = Arc::new(TypstEngine::new());
        let pdf_renderer: Arc<dyn PdfRenderer> = Arc::new(NativePdfRenderer::new());
        let ai_provider: Arc<dyn AiProvider> = crate::ai::provider::create_default_provider();
        let settings = GrafSettings::default();
        let controller = CompilerController::new();

        cx.observe(&editor, |this, editor, cx| {
            this.on_editor_changed(editor, cx);
        })
        .detach();

        let mut workspace = Self {
            project_tree,
            documents: vec![initial_doc],
            active_doc_idx: 0,
            editor,
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
            latest_diagnostics: Vec::new(),
            bib_index: crate::project::bibtex::BibtexIndex::new(),
            label_index: crate::project::bibtex::LabelIndex::default(),
            completions: Vec::new(),
            completion_open: false,
            find_state: FindState::new(),
            find_bar_open: false,
            active_modal: ActiveModal::None,
        };

        workspace.reload_bibtex_and_labels(cx);
        workspace.trigger_compile(cx);
        workspace
    }

    /// Determines the active typesetting engine based on the active document format.
    pub fn active_engine(&self) -> EngineKind {
        if let Some(doc) = self.documents.get(self.active_doc_idx)
            && doc.title().ends_with(".typ")
        {
            return EngineKind::Typst;
        }
        EngineKind::Latex
    }

    /// Triggers context-aware autocompletion for citations, labels, and environments.
    pub fn trigger_autocomplete(&mut self, cx: &mut Context<Self>) {
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
        self.completion_open = !self.completions.is_empty();
        cx.notify();
    }

    /// Applies a selected autocompletion item.
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
        cx.notify();
    }

    /// Returns the focus handle for the workspace's editor.
    pub fn editor_focus_handle(&self, cx: &Context<Self>) -> FocusHandle {
        self.editor.read(cx).focus_handle(cx)
    }

    /// Toggles the left sidebar visibility.
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    /// Toggles the right preview pane visibility.
    pub fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_visible = !self.preview_visible;
        cx.notify();
    }

    /// Toggles the diagnostics problems drawer.
    pub fn toggle_diagnostics(&mut self, cx: &mut Context<Self>) {
        self.diagnostics_drawer_open = !self.diagnostics_drawer_open;
        cx.notify();
    }

    /// Toggles the Find & Replace bar.
    pub fn toggle_find(&mut self, cx: &mut Context<Self>) {
        self.find_bar_open = !self.find_bar_open;
        if self.find_bar_open {
            let text = self.editor.read(cx).text().to_string();
            let query = self.find_state.query.clone();
            self.find_state.set_query(query, &text);
        }
        cx.notify();
    }

    /// Opens AI Assist action modal (⌘I).
    pub fn open_ai_assist(&mut self, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::AiAssist(String::new());
        cx.notify();
    }

    /// Opens Settings modal (⌘,).
    pub fn open_settings(&mut self, tab: SettingsTab, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::Settings(tab);
        cx.notify();
    }

    /// Creates a new Typst document tab.
    pub fn new_typst_document(&mut self, cx: &mut Context<Self>) {
        let initial_typst = "= Hello Typst\n#set page(paper: \"a4\")\n\nWelcome to Graf with first-class native Typst support.\n\n== Mathematical Formulations\nTypst math is fast and clean:\n$ E = m c^2 $\n$ integral_0^infinity e^(-x^2) dif x = sqrt(pi) / 2 $\n";
        let doc_name = format!("document-{}.typ", self.documents.len() + 1);
        let doc = Document::new_untitled(&doc_name, initial_typst);
        self.documents.push(doc);
        self.active_doc_idx = self.documents.len() - 1;
        self.active_view_kind = ActiveViewKind::Editor;
        self.editor.update(cx, |ed, cx| {
            ed.set_text(initial_typst, cx);
            ed.set_is_typst(true, cx);
        });
        cx.notify();
        self.trigger_compile(cx);
    }

    /// Executes an AI technical writing operation.
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

    /// Accepts staged AI diff review and writes changes to buffer.
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

    /// Creates a new vector diagram canvas tab.
    pub fn new_canvas_diagram(&mut self, cx: &mut Context<Self>) {
        let default_canvas_json = self.canvas.read(cx).save_to_json().unwrap_or_default();
        let doc_name = format!("diagram-{}.graf", self.documents.len() + 1);
        let doc = Document::new_untitled(&doc_name, default_canvas_json);
        self.documents.push(doc);
        self.active_doc_idx = self.documents.len() - 1;
        self.active_view_kind = ActiveViewKind::Canvas;
        cx.notify();
    }

    /// Inserts a formatted academic table into the editor at cursor position.
    pub fn insert_table_template(&mut self, cx: &mut Context<Self>) {
        let is_typst = self.documents[self.active_doc_idx]
            .title()
            .ends_with(".typ");
        let mut table = crate::editor::table::TableData::new(3, 3);
        table.rows[0] = vec![
            "Method".to_string(),
            "Latency (ms)".to_string(),
            "Accuracy (%)".to_string(),
        ];
        table.rows[1] = vec![
            "Baseline".to_string(),
            "12.4".to_string(),
            "89.5".to_string(),
        ];
        table.rows[2] = vec![
            "Ours (Graf)".to_string(),
            "1.2".to_string(),
            "97.8".to_string(),
        ];
        table.caption = Some("Experimental Results on Benchmark Dataset".to_string());
        table.label = Some("tab:results".to_string());

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

    /// Exports active vector canvas diagram to TikZ LaTeX code in clipboard.
    pub fn export_canvas_to_tikz(&mut self, cx: &mut Context<Self>) {
        let doc = self.canvas.read(cx).document();
        let tikz_code = crate::canvas::tikz::export_to_tikz(doc);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(tikz_code));
    }

    /// Exports active vector canvas diagram to SVG markup in clipboard.
    pub fn export_canvas_to_svg(&mut self, cx: &mut Context<Self>) {
        let doc = self.canvas.read(cx).document();
        let svg_code = crate::canvas::svg::export_to_svg(doc);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(svg_code));
    }

    /// Runs academic style linter on the active document and feeds warnings to diagnostics.
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

    /// Syncs local Zotero library entries into the citation autocompletion index.
    pub fn sync_zotero_library(&mut self, cx: &mut Context<Self>) {
        let zotero_lib = crate::project::zotero::ZoteroLibrary::scan_local_storage();
        for item in zotero_lib.items {
            self.bib_index.add_entry(item.to_bib_entry());
        }
        cx.notify();
    }

    /// Inserts a standardized arXiv reference snippet at the editor cursor.
    pub fn import_arxiv_sample(&mut self, cx: &mut Context<Self>) {
        let sample_paper = crate::project::arxiv::ArxivPaper {
            id: "1706.03762".to_string(),
            title: "Attention Is All You Need".to_string(),
            authors: vec!["Ashish Vaswani".to_string(), "Noam Shazeer".to_string()],
            summary: "The dominant sequence transduction models are based on complex recurrent or convolutional neural networks.".to_string(),
            year: 2017,
            primary_category: "cs.CL".to_string(),
            pdf_url: "https://arxiv.org/pdf/1706.03762.pdf".to_string(),
        };

        let is_typst = self.documents[self.active_doc_idx]
            .title()
            .ends_with(".typ");
        let cite_snippet = if is_typst {
            format!("@{}", sample_paper.citekey())
        } else {
            format!("\\cite{{{}}}", sample_paper.citekey())
        };

        self.editor.update(cx, |editor, cx| {
            editor.insert_snippet(&cite_snippet, cx);
        });
        self.sync_active_doc_from_editor(cx);
        cx.notify();
    }

    /// Converts a Mermaid flowchart specification into a new native `.graf` diagram tab.
    pub fn import_mermaid_diagram(&mut self, cx: &mut Context<Self>) {
        let sample_mermaid = r#"graph TD
    A[Data Pipeline] --> B[Feature Engineering]
    B --> C[Neural Transformer]
    C --> D[Evaluation & Benchmarks]"#;

        if let Ok(canvas_doc) = crate::canvas::mermaid::parse_mermaid_to_canvas(sample_mermaid) {
            let json = canvas_doc.to_json().unwrap_or_default();
            let doc_name = format!("mermaid-{}.graf", self.documents.len() + 1);
            let doc = Document::new_untitled(&doc_name, json);
            self.documents.push(doc);
            self.active_doc_idx = self.documents.len() - 1;
            self.active_view_kind = ActiveViewKind::Canvas;
            self.canvas.update(cx, |canvas, cx| {
                let _ = canvas
                    .load_from_json(self.documents[self.active_doc_idx].buffer().content(), cx);
            });
            cx.notify();
        }
    }

    /// Scans ~/.graf/plugins for Wasm extensions and updates the plugin registry.
    pub fn scan_plugins(&mut self, cx: &mut Context<Self>) {
        let mut host = crate::plugins::host::PluginHost::new();
        let _ = host.scan_plugin_directory();
        cx.notify();
    }

    /// Opens Quick Open modal (⌘P).
    pub fn open_quick_open(&mut self, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::QuickOpen(String::new());
        cx.notify();
    }

    /// Opens Command Palette modal (⌘K).
    pub fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.active_modal = ActiveModal::CommandPalette(String::new());
        cx.notify();
    }

    /// Closes any open modal dialog, completion list, or Find bar.
    pub fn close_modal(&mut self, cx: &mut Context<Self>) {
        if self.active_modal != ActiveModal::None {
            self.active_modal = ActiveModal::None;
        } else if self.completion_open {
            self.completion_open = false;
        } else if self.find_bar_open {
            self.find_bar_open = false;
        }
        cx.notify();
    }

    /// Jumps to a specific line in the editor and focuses it.
    pub fn jump_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.active_view_kind = ActiveViewKind::Editor;
        self.editor.update(cx, |editor, cx| {
            editor.jump_to_line(line, cx);
        });
        cx.notify();
    }

    /// Opens a file from path into a workspace tab.
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

        if let Ok(doc) = Document::open(&path) {
            let is_canvas = doc.title().ends_with(".graf");
            let is_typst = doc.title().ends_with(".typ");
            let content = doc.buffer().content().to_string();
            self.documents.push(doc);
            self.active_doc_idx = self.documents.len() - 1;

            if is_canvas {
                self.active_view_kind = ActiveViewKind::Canvas;
                self.canvas.update(cx, |c, cx| {
                    let _ = c.load_from_json(&content, cx);
                });
            } else {
                self.active_view_kind = ActiveViewKind::Editor;
                self.editor.update(cx, |editor, cx| {
                    editor.set_text(content, cx);
                    editor.set_is_typst(is_typst, cx);
                });
            }
            cx.notify();
            self.trigger_compile(cx);
        }
    }

    /// Switches the active tab index.
    pub fn switch_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.documents.len() || idx == self.active_doc_idx {
            return;
        }
        self.sync_active_doc_from_editor(cx);
        self.active_doc_idx = idx;

        let is_canvas = self.documents[idx].title().ends_with(".graf");
        let is_typst = self.documents[idx].title().ends_with(".typ");
        if is_canvas {
            self.active_view_kind = ActiveViewKind::Canvas;
            let content = self.documents[idx].buffer().content();
            self.canvas.update(cx, |c, cx| {
                let _ = c.load_from_json(content, cx);
            });
        } else {
            self.active_view_kind = ActiveViewKind::Editor;
            let content = self.documents[idx].buffer().content().to_string();
            self.editor.update(cx, |editor, cx| {
                editor.set_text(content, cx);
                editor.set_is_typst(is_typst, cx);
            });
        }
        cx.notify();
        self.trigger_compile(cx);
    }

    /// Closes the tab at index `idx`.
    pub fn close_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if self.documents.len() <= 1 {
            return;
        }
        if idx >= self.documents.len() {
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

        if is_canvas {
            self.active_view_kind = ActiveViewKind::Canvas;
            let content = self.documents[self.active_doc_idx].buffer().content();
            self.canvas.update(cx, |c, cx| {
                let _ = c.load_from_json(content, cx);
            });
        } else {
            self.active_view_kind = ActiveViewKind::Editor;
            let content = self.documents[self.active_doc_idx]
                .buffer()
                .content()
                .to_string();
            self.editor.update(cx, |editor, cx| {
                editor.set_text(content, cx);
                editor.set_is_typst(is_typst, cx);
            });
        }
        cx.notify();
        self.trigger_compile(cx);
    }

    /// Saves the currently active document to disk. Auto-exports companion .svg for .graf files.
    pub fn save_active_document(&mut self, cx: &mut Context<Self>) {
        if self.active_view_kind == ActiveViewKind::Canvas {
            let json = self.canvas.read(cx).save_to_json().unwrap_or_default();
            let svg = self.canvas.read(cx).export_svg();

            if let Some(doc) = self.documents.get_mut(self.active_doc_idx) {
                doc.buffer_mut().replace_all(json);
                let saved = doc.save().is_ok();
                if saved {
                    if let Some(path) = doc.path() {
                        let svg_path = path.with_extension("svg");
                        let _ = std::fs::write(svg_path, svg);
                    }
                    cx.notify();
                    self.trigger_compile(cx);
                }
            }
            return;
        }

        self.sync_active_doc_from_editor(cx);
        if let Some(doc) = self.documents.get_mut(self.active_doc_idx) {
            let saved = doc.save().is_ok();
            if saved {
                self.save_recovery_snapshot();
                cx.notify();
                self.trigger_compile(cx);
            }
        }
    }

    /// Saves periodic recovery snapshot of all unsaved buffers to disk.
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
        if entries.is_empty() {
            let _ = crate::project::recovery::RecoveryJournal::clear_dir(&recovery_dir);
        } else {
            let journal = crate::project::recovery::RecoveryJournal::new(entries);
            let _ = journal.save_to_dir(&recovery_dir);
        }
    }

    fn sync_active_doc_from_editor(&mut self, cx: &Context<Self>) {
        if self.active_view_kind == ActiveViewKind::Canvas {
            let json = self.canvas.read(cx).save_to_json().unwrap_or_default();
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

    /// Scans project .bib files from disk.
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

    /// Parses in-memory editor labels for cross-reference autocompletion.
    pub fn reload_editor_labels(&mut self, cx: &Context<Self>) {
        let editor_text = self.editor.read(cx).text();
        self.label_index.parse_and_load(editor_text);
    }

    /// Scans project .bib files and editor labels to update autocompletion indices.
    pub fn reload_bibtex_and_labels(&mut self, cx: &Context<Self>) {
        self.reload_bib_files();
        self.reload_editor_labels(cx);
    }

    /// Called whenever the editor buffer changes.
    fn on_editor_changed(&mut self, editor: Entity<EditorView>, cx: &mut Context<Self>) {
        let rev = editor.read(cx).revision();
        self.sync_active_doc_from_editor(cx);
        self.reload_editor_labels(cx);

        if rev > self.controller.current_revision() {
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

    /// Triggers an immediate compilation of current editor source using active engine.
    pub fn trigger_compile(&mut self, cx: &mut Context<Self>) {
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

            match result {
                Ok(output) => {
                    let pdf_bytes = output.artifact.clone();
                    let output_rev = output.revision;
                    let diags = output.diagnostics.clone();

                    let render_result = cx
                        .background_executor()
                        .spawn(async move { pdf_renderer.render_document(output_rev, &pdf_bytes) })
                        .await;

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

    /// Dispatches a command palette action by unique identifier.
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
            10 => self.open_settings(SettingsTab::General, cx),
            11 => self.new_typst_document(cx),
            12 => self.open_settings(SettingsTab::Licenses, cx),
            13 => {
                self.save_recovery_snapshot();
                self.open_settings(SettingsTab::General, cx);
            }
            14 => self.insert_table_template(cx),
            15 => self.export_canvas_to_tikz(cx),
            16 => self.export_canvas_to_svg(cx),
            17 => self.lint_academic_style(cx),
            18 => self.sync_zotero_library(cx),
            19 => self.import_arxiv_sample(cx),
            20 => self.import_mermaid_diagram(cx),
            21 => self.scan_plugins(cx),
            _ => {}
        }
    }

    fn on_compile(&mut self, _: &Compile, _window: &mut Window, cx: &mut Context<Self>) {
        self.trigger_compile(cx);
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

    fn on_toggle_find(&mut self, _: &ToggleFind, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_find(cx);
    }

    fn on_quick_open(&mut self, _: &QuickOpen, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_quick_open(cx);
    }

    fn on_command_palette(
        &mut self,
        _: &CommandPalette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(cx);
    }

    fn on_ai_assist(&mut self, _: &AiAssist, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_ai_assist(cx);
    }

    fn on_open_settings(&mut self, _: &OpenSettings, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(SettingsTab::General, cx);
    }

    fn on_close_modal(&mut self, _: &CloseModal, _window: &mut Window, cx: &mut Context<Self>) {
        self.close_modal(cx);
    }

    fn on_autocomplete(&mut self, _: &Autocomplete, _window: &mut Window, cx: &mut Context<Self>) {
        self.trigger_autocomplete(cx);
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
            .on_action(cx.listener(Self::on_compile))
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
            .on_action(cx.listener(Self::on_close_modal))
            .on_action(cx.listener(Self::on_autocomplete))
            .child(self.render_top_bar(cx))
            .child(self.render_body(cx))
            .child(self.render_status_bar(cx));

        // Render modal overlay if active
        if let Some(modal) = self.render_modal(cx) {
            root = root.child(modal);
        }

        root
    }
}
