//! Template-driven creation: the picker modal state, new documents from
//! templates, and the "new project" flow that scaffolds a root document in
//! a user-chosen folder and re-roots the workspace there.

use super::*;

impl Workspace {
    pub(crate) fn open_template_picker(
        &mut self,
        kind: Option<crate::project::document::DocumentKind>,
        for_new_project: bool,
        cx: &mut Context<Self>,
    ) {
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text("", cx));
        self.active_modal = ActiveModal::TemplatePicker(super::TemplatePickerRequest {
            kind,
            for_new_project,
        });
        self.prompt_target = state::PromptTarget::TemplatePicker;
        cx.notify();
    }

    pub(crate) fn create_document_from_template(
        &mut self,
        template: &'static crate::project::templates::DocumentTemplate,
        cx: &mut Context<Self>,
    ) {
        self.sync_active_doc_from_editor(cx);
        let titles: Vec<String> = self
            .documents
            .iter()
            .map(|document| document.title().to_string())
            .collect();
        let title = unique_title(template.file_name, &titles);
        self.documents
            .push(Document::new_untitled(&title, template.content));
        self.active_modal = ActiveModal::None;
        self.prompt_target = state::PromptTarget::Idle;
        self.activate_document(self.documents.len() - 1, cx);
    }

    /// Resolves a template id chosen from the picker (click or Enter) and
    /// routes it to document creation or project scaffolding. The request is
    /// passed in because callers clear the modal before dispatching.
    pub(crate) fn accept_template(
        &mut self,
        id: &'static str,
        request: super::TemplatePickerRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(template) = crate::project::templates::template_by_id(id) else {
            return;
        };
        if request.for_new_project
            && let Some(dir) = self.pending_project_dir.take()
        {
            self.create_project_from_template(dir, template, cx);
            return;
        }
        self.create_document_from_template(template, cx);
    }

    /// Asks for a destination folder, then opens the template picker in
    /// project mode; the scaffold happens once a template is accepted.
    pub(crate) fn new_project(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a folder for the new project".into()),
        });
        cx.spawn(async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                let Some(dir) = paths.into_iter().next() else {
                    return;
                };
                this.update(cx, |this, cx| {
                    this.pending_project_dir = Some(dir);
                    this.open_template_picker(None, true, cx);
                })
                .ok();
            }
            Ok(Err(error)) => {
                this.update(cx, |this, cx| {
                    this.workspace_error = Some(format!("Could not open folder picker: {error}"));
                    cx.notify();
                })
                .ok();
            }
            _ => {}
        })
        .detach();
    }

    pub(crate) fn create_project_from_template(
        &mut self,
        dir: PathBuf,
        template: &'static crate::project::templates::DocumentTemplate,
        cx: &mut Context<Self>,
    ) {
        self.active_modal = ActiveModal::None;
        self.prompt_target = state::PromptTarget::Idle;

        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.workspace_error = Some(format!("Could not create {}: {error}", dir.display()));
            cx.notify();
            return;
        }

        let target = dir.join(template.file_name);
        // Never overwrite existing files: if the name is taken, open the
        // folder as-is so the user keeps whatever is already there.
        if !target.exists()
            && let Err(error) = crate::project::atomic_write(&target, template.content.as_bytes())
        {
            self.workspace_error = Some(format!("Could not create {}: {error}", target.display()));
            cx.notify();
            return;
        }

        self.apply_project_root(dir, cx);
        self.open_file(target, cx);
    }

    /// Re-roots the workspace at `root`: scans the tree, reloads references,
    /// and surfaces any recovery journal that lives in the new project.
    fn apply_project_root(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        // Dirty documents belong to the old project's recovery journal.
        self.save_recovery_snapshot();
        self.project_tree = ProjectTree::scan(&root);
        self.workspace_error = None;
        self.reload_bibtex_and_labels(cx);

        let recovery_dir = root.join(".graf").join("recovery");
        self.pending_recovery =
            crate::project::recovery::RecoveryJournal::load_from_dir(&recovery_dir)
                .filter(|journal| !journal.entries.is_empty());
        if self.pending_recovery.is_some() {
            self.active_modal = ActiveModal::RestoreRecovery;
        }
        cx.notify();
    }
}
