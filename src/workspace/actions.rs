//! GPUI action handlers and the command dispatch: everything the shell
//! subscribes to in render.rs lives here so mod.rs keeps structural state.

use super::*;

impl Workspace {
    pub(super) fn on_prompt_changed(&mut self, prompt: Entity<EditorView>, cx: &mut Context<Self>) {
        let raw_query = prompt.read(cx).text().to_string();
        let submitted = raw_query.contains('\n');
        let query = raw_query.replace('\n', "");

        if self.prompt_target == state::PromptTarget::Find {
            let content = self.editor.read(cx).text().to_string();
            self.find_state.set_query(query.clone(), &content);
        }

        if submitted {
            if self.prompt_target == state::PromptTarget::Find
                && let Some(matched) = self.find_state.next_match().cloned()
            {
                self.editor
                    .update(cx, |editor, cx| editor.select_range(matched, cx));
            }

            match self.prompt_target {
                state::PromptTarget::QuickOpen => {
                    let query = query.to_lowercase();
                    if let Some(path) = self
                        .project_tree
                        .quick_open_matches(&query, QUICK_OPEN_SEARCH_LIMIT)
                        .first()
                        .map(|entry| entry.path.clone())
                    {
                        self.active_modal = ActiveModal::None;
                        self.prompt_target = state::PromptTarget::Idle;
                        self.open_file(path, cx);
                    }
                }
                state::PromptTarget::Palette => {
                    let query = query.to_lowercase();
                    if let Some(command) = commands::filter_commands(&query).next() {
                        self.active_modal = ActiveModal::None;
                        self.prompt_target = state::PromptTarget::Idle;
                        self.dispatch_command_action(command.id, cx);
                    }
                }
                state::PromptTarget::TemplatePicker => {
                    let request = match self.active_modal {
                        ActiveModal::TemplatePicker(request) => request,
                        _ => super::TemplatePickerRequest {
                            kind: None,
                            for_new_project: false,
                        },
                    };
                    let query = query.to_lowercase();
                    if let Some(template) =
                        crate::project::templates::filter_templates(&query, request.kind).next()
                    {
                        self.active_modal = ActiveModal::None;
                        self.prompt_target = state::PromptTarget::Idle;
                        self.accept_template(template.id, request, cx);
                    }
                }
                state::PromptTarget::Find | state::PromptTarget::Idle => {}
            }
            self.prompt_editor
                .update(cx, |input, cx| input.set_input_text(query, cx));
        }

        cx.notify();
    }

    pub fn dispatch_command_action(&mut self, id: commands::CommandId, cx: &mut Context<Self>) {
        match id {
            CommandId::Compile => self.trigger_compile(cx),
            CommandId::Save => self.save_active_document(cx),
            CommandId::FindInFile => self.toggle_find(cx),
            CommandId::ToggleProject => self.toggle_sidebar(cx),
            CommandId::TogglePreview => self.toggle_preview(cx),
            CommandId::ToggleProblems => self.toggle_diagnostics(cx),
            CommandId::CloseTab => {
                let active = self.active_doc_idx;
                self.close_tab(active, cx);
            }
            CommandId::NewVectorDiagram => self.new_canvas_diagram(cx),
            CommandId::OpenSettings => self.open_settings(SettingsTab::Editor, cx),
            CommandId::NewTypstDocument => self.new_typst_document(cx),
            CommandId::NewFromTemplate => self.open_template_picker(None, false, cx),
            CommandId::NewProject => self.new_project(cx),
            CommandId::AboutGraf => self.open_about(cx),
            CommandId::InsertTable => self.insert_table_template(cx),
            CommandId::ExportTikz => self.export_canvas_to_tikz(cx),
            CommandId::ExportSvg => self.export_canvas_to_svg(cx),
            CommandId::CheckWritingStyle => self.lint_academic_style(cx),
            CommandId::SyncZotero => self.sync_zotero_library(cx),
        }
    }

    pub(super) fn on_focus_editor(
        &mut self,
        _: &FocusEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
    }

    pub(super) fn on_compile(&mut self, _: &Compile, _window: &mut Window, cx: &mut Context<Self>) {
        self.trigger_compile(cx);
    }

    pub(super) fn on_open_file(
        &mut self,
        _: &OpenFile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_picker(cx);
    }

    pub(super) fn on_save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        self.save_active_document(cx);
    }

    pub(super) fn on_close_tab(
        &mut self,
        _: &CloseTab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active = self.active_doc_idx;
        self.close_tab(active, cx);
    }

    pub(super) fn on_toggle_sidebar(
        &mut self,
        _: &ToggleSidebar,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_sidebar(cx);
    }

    pub(super) fn on_toggle_preview(
        &mut self,
        _: &TogglePreview,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_preview(cx);
    }

    pub(super) fn on_toggle_diagnostics(
        &mut self,
        _: &ToggleDiagnostics,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_diagnostics(cx);
    }

    pub(super) fn on_toggle_find(
        &mut self,
        _: &ToggleFind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_find(cx);
        if self.find_bar_open {
            window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
        } else {
            window.focus(&self.editor.read(cx).focus_handle(cx), cx);
        }
    }

    pub(super) fn on_quick_open(
        &mut self,
        _: &QuickOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_quick_open(cx);
        window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
    }

    pub(super) fn on_command_palette(
        &mut self,
        _: &CommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_command_palette(cx);
        window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
    }

    pub(super) fn on_open_settings(
        &mut self,
        _: &OpenSettings,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_settings(SettingsTab::Editor, cx);
    }

    pub(super) fn on_open_about(
        &mut self,
        _: &OpenAbout,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_about(cx);
    }

    pub(super) fn on_new_from_template(
        &mut self,
        _: &NewFromTemplate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_template_picker(None, false, cx);
        window.focus(&self.prompt_editor.read(cx).focus_handle(cx), cx);
    }

    pub(super) fn on_new_project(
        &mut self,
        _: &NewProject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_project(cx);
    }

    pub(super) fn on_close_modal(
        &mut self,
        _: &CloseModal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_modal(cx);
        window.focus(&self.editor.read(cx).focus_handle(cx), cx);
    }

    pub(super) fn on_autocomplete(
        &mut self,
        _: &Autocomplete,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.trigger_autocomplete(cx);
    }

    pub(super) fn on_toggle_performance_overlay(
        &mut self,
        _: &TogglePerformanceOverlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_performance_overlay(window, cx);
    }
}
