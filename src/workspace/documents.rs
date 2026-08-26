use super::*;

impl Workspace {
    pub(super) fn activate_document(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.documents.len() {
            return;
        }

        self.active_doc_idx = idx;
        self.show_welcome = false;
        self.compile_task = None;
        self.controller.reset();
        self.latest_diagnostics.clear();
        self.editor
            .update(cx, |editor, cx| editor.set_diagnostics(Vec::new(), cx));

        let document = &self.documents[idx];
        let title = document.title();
        let content = document.buffer().content().to_string();
        let is_canvas = title.ends_with(".graf");
        let is_typst = title.ends_with(".typ");
        let is_plain_text = !is_typst && !title.ends_with(".tex");

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

        self.documents.push(doc);
        self.workspace_error = None;
        self.activate_document(self.documents.len() - 1, cx);
    }

    pub fn switch_tab(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx >= self.documents.len() || idx == self.active_doc_idx {
            return;
        }
        self.sync_active_doc_from_editor(cx);
        self.activate_document(idx, cx);
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
        let closed_active_document = idx == self.active_doc_idx;
        self.documents.remove(idx);

        if idx < self.active_doc_idx {
            self.active_doc_idx -= 1;
        } else if self.active_doc_idx >= self.documents.len() {
            self.active_doc_idx = self.documents.len() - 1;
        }

        if closed_active_document {
            self.activate_document(self.active_doc_idx, cx);
        } else {
            cx.notify();
        }
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
                    crate::project::atomic_write(&path, svg.as_bytes())
                        .map_err(|error| error.to_string())?;
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

    pub(super) fn sync_active_doc_from_editor(&mut self, cx: &Context<Self>) {
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
}
