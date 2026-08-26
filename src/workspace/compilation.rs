use super::*;

impl Workspace {
    pub(super) fn on_editor_changed(&mut self, editor: Entity<EditorView>, cx: &mut Context<Self>) {
        let rev = editor.read(cx).revision();
        self.sync_active_doc_from_editor(cx);
        self.save_recovery_snapshot();
        self.reload_editor_labels(cx);
        self.trigger_autocomplete(cx);

        if self.active_document_is_compilable()
            && rev > self.controller.current_revision()
            && self.settings.editor.auto_compile
        {
            self.controller.on_source_edited(rev);
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
            .filter(|path| match engine {
                EngineKind::Latex => path.extension().is_some_and(|extension| extension == "tex"),
                EngineKind::Typst => path.extension().is_some_and(|extension| extension == "typ"),
            })
            .map(Path::to_path_buf)
            .or_else(|| {
                self.documents
                    .get(self.active_doc_idx)
                    .and_then(|document| document.path().map(Path::to_path_buf))
            });

        let request = CompileRequest::with_project(text, rev, project_root, root_document);
        self.controller.begin_compile(request.compile_id, rev);
        cx.notify();

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
                    let render_id = output.compile_id.0;
                    let diags = output.diagnostics.clone();

                    let render_result = cx
                        .background_executor()
                        .spawn(async move { pdf_renderer.render_document(render_id, &pdf_bytes) })
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
                        if let Err(stale) = this.controller.handle_output(output) {
                            info!(
                                "discarded compile output for revision {}; current revision is {}",
                                stale.completed_revision, stale.current_revision
                            );
                            return;
                        }

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
                        if let Err(stale) = this.controller.handle_error(err) {
                            info!(
                                "discarded compile error for revision {}; current revision is {}",
                                stale.completed_revision, stale.current_revision
                            );
                            return;
                        }

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
}
