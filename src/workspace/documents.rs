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
        // Refresh cached outline + stats for the newly active tab. Canvas
        // tabs clear to empty; editor tabs compute inline when small.
        self.refresh_caches_for_doc_switch(cx);

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

    pub fn restore_recovery(&mut self, cx: &mut Context<Self>) {
        let Some(journal) = self.pending_recovery.take() else {
            self.active_modal = ActiveModal::None;
            cx.notify();
            return;
        };

        for entry in journal.entries {
            match crate::project::recovery::RecoveryJournal::restore_target(&entry) {
                crate::project::recovery::RestoreTarget::Existing(path) => {
                    if let Some(index) = self
                        .documents
                        .iter()
                        .position(|document| document.path() == Some(path.as_path()))
                    {
                        let document = &mut self.documents[index];
                        if document.buffer().content() != entry.content {
                            document.buffer_mut().replace_all(entry.content);
                        }
                    } else if let Ok(mut document) = Document::open(&path) {
                        document.buffer_mut().replace_all(entry.content);
                        self.documents.push(document);
                    }
                }
                crate::project::recovery::RestoreTarget::Untitled(title) => {
                    self.documents
                        .push(Document::new_untitled(title, entry.content));
                }
            }
        }

        self.clear_recovery_journal();
        self.active_modal = ActiveModal::None;
        if self.documents.is_empty() {
            cx.notify();
        } else {
            self.activate_document(self.documents.len() - 1, cx);
        }
    }

    pub fn discard_recovery(&mut self, cx: &mut Context<Self>) {
        self.pending_recovery = None;
        self.clear_recovery_journal();
        self.active_modal = ActiveModal::None;
        cx.notify();
    }

    fn clear_recovery_journal(&self) {
        let recovery_dir = self.project_tree.root_path().join(".graf").join("recovery");
        if let Err(error) = crate::project::recovery::RecoveryJournal::clear_dir(&recovery_dir) {
            warn!("failed to clear recovery journal: {error}");
        }
    }

    pub fn save_recovery_snapshot(&mut self) {
        // Explicit flush (save, tab switch persistence points): cancel any
        // pending debounced flush so it cannot overwrite the just-saved
        // state with stale dirty data, then write synchronously with compact
        // JSON via the existing atomic path.
        self.recovery_task = None;
        let current_rev = self.last_synced_editor_rev;
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
        } else {
            self.last_recovery_rev = Some(current_rev);
        }
    }

    /// Debounced crash-recovery flush for the per-keystroke path.
    /// Keystrokes only reschedule a 1-2s coalescing timer; the journal is
    /// snapshotted after the quiet period and serialized plus written on the
    /// background executor (compact JSON, atomic write). Never `create_dir`,
    /// pretty-print, or `fsync` on the UI thread per keystroke. Crash losses
    /// are bounded to the debounce window.
    pub(crate) fn schedule_recovery_snapshot(&mut self, cx: &mut Context<Self>, editor_rev: u64) {
        use crate::workspace::coalesce::{RECOVERY_DEBOUNCE, recovery_result_is_current};
        if self.last_recovery_rev == Some(editor_rev) {
            return;
        }
        let recovery_dir = self.project_tree.root_path().join(".graf").join("recovery");
        // Overwriting drops (cancels) the previous pending timer, coalescing
        // rapid keystrokes into a single background flush.
        self.recovery_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECOVERY_DEBOUNCE).await;
            let snapshot: Option<crate::project::recovery::RecoveryJournal> = this
                .update(cx, |this, cx| {
                    let current_rev = this.editor.read(cx).revision();
                    if !recovery_result_is_current(editor_rev, current_rev) {
                        return None;
                    }
                    let entries: Vec<crate::project::recovery::RecoveryEntry> = this
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
                    if entries.is_empty() {
                        return None;
                    }
                    Some(crate::project::recovery::RecoveryJournal::new(entries))
                })
                .ok()
                .flatten();
            let Some(journal) = snapshot else {
                // No dirty docs, or a newer keystroke superseded this flush.
                // Clearing an empty journal is handled by the explicit
                // save path; the debounced path never deletes on stale revs.
                this.update(cx, |this, cx| {
                    let current_rev = this.editor.read(cx).revision();
                    if recovery_result_is_current(editor_rev, current_rev)
                        && !this.documents.iter().any(|d| d.is_dirty())
                    {
                        let dir = recovery_dir.clone();
                        let background = cx.background_executor().clone();
                        background
                            .spawn(async move {
                                if let Err(error) =
                                    crate::project::recovery::RecoveryJournal::clear_dir(&dir)
                                {
                                    warn!("failed to clear recovery journal: {error}");
                                }
                            })
                            .detach();
                        this.last_recovery_rev = Some(editor_rev);
                    }
                })
                .ok();
                return;
            };
            let write_result = cx
                .background_executor()
                .spawn(async move {
                    journal
                        .save_to_dir(&recovery_dir)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
                .await;
            match write_result {
                Ok(()) => {
                    this.update(cx, |this, cx| {
                        let current_rev = this.editor.read(cx).revision();
                        if recovery_result_is_current(editor_rev, current_rev) {
                            this.last_recovery_rev = Some(editor_rev);
                        }
                    })
                    .ok();
                }
                Err(error) => {
                    warn!("failed to update recovery journal: {error}");
                }
            }
        }));
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

        // Single snapshot per change: read the editor once, compare the
        // borrowed text before cloning, and clone at most once. Revision
        // equality skips cursor-only notifications without touching text.
        let (editor_rev, cloned) = {
            let editor = self.editor.read(cx);
            let rev = editor.revision();
            if !crate::workspace::coalesce::should_sync_editor_text(
                rev,
                self.last_synced_editor_rev,
            ) {
                (rev, None)
            } else {
                let needs = self.documents.get(self.active_doc_idx).is_some_and(|doc| {
                    crate::workspace::coalesce::doc_needs_update(
                        doc.buffer().content(),
                        editor.text(),
                    )
                });
                let text = needs.then(|| editor.text().to_string());
                (rev, text)
            }
        };
        if let Some(text) = cloned
            && let Some(doc) = self.documents.get_mut(self.active_doc_idx)
        {
            doc.buffer_mut().replace_all(text);
        }
        self.last_synced_editor_rev = editor_rev;
    }

    // Explicit manual refresh (palettes, commands). The per-keystroke hot
    // path uses debounced `schedule_assist_debounced` instead, so this stays
    // off the typing path.
    #[allow(dead_code)]
    pub fn reload_editor_labels(&mut self, cx: &Context<Self>) {
        let editor_text = self.editor.read(cx).text();
        self.label_index.parse_and_load(editor_text);
    }

    // Note: startup `.bib` scanning moved to the background
    // `load_startup_payload` in `super` (see P0-1); the synchronous
    // `reload_bib_files`/`reload_bibtex_and_labels` helpers were removed so
    // project I/O never runs on the UI thread.
}
