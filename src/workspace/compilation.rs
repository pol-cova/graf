use super::*;

impl Workspace {
    /// Abort any in-flight compile whose result would be stale: flip its
    /// cancel flag so the engine kills the subprocess instead of running to
    /// completion. Called whenever the source changes faster than the build.
    pub(super) fn cancel_in_flight_compile(&mut self) {
        if let Some(flag) = self.compile.cancel.as_ref() {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

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
            self.cancel_in_flight_compile();
            self.controller.on_source_edited(rev);
            cx.notify();

            // Replacing the stored task drops the previous handle, which
            // cancels that pending timer (gpui tasks cancel on drop); the
            // generation check below is the explicit backstop so a stale
            // timer can never call trigger_compile even if this invariant
            // is broken by a later refactor.
            self.compile.generation += 1;
            let debounce = self.controller.debounce_duration();
            let generation = self.compile.generation;
            self.compile.task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(debounce).await;
                this.update(cx, |this, cx| {
                    if this.compile.generation == generation {
                        this.trigger_compile(cx);
                    }
                })
                .ok();
            }));
        }
    }

    pub fn trigger_compile(&mut self, cx: &mut Context<Self>) {
        if !self.active_document_is_compilable() {
            self.compile.pending = false;
            self.controller.reset();
            self.latest_diagnostics.clear();
            self.preview.update(cx, |preview, cx| preview.clear(cx));
            cx.notify();
            return;
        }

        if self.compile.running {
            self.compile.pending = true;
            return;
        }

        // Drop the previous token so a fresh one guards this compile.
        self.cancel_in_flight_compile();
        self.compile.cancel = None;

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
            .filter(|path| {
                crate::project::kinds::FileKind::from_path(path).as_engine() == Some(engine)
            })
            .map(Path::to_path_buf)
            .or_else(|| {
                self.documents
                    .get(self.active_doc_idx)
                    .and_then(|document| document.path().map(Path::to_path_buf))
            });

        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.compile.cancel = Some(cancel_flag.clone());
        let render_cancel = cancel_flag.clone();
        let request = CompileRequest::with_project(text, rev, project_root, root_document)
            .with_cancel(cancel_flag);
        self.controller.begin_compile(request.compile_id, rev);
        self.compile.running = true;
        self.compile.pending = false;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { compiler.compile(request) })
                .await;

            match result {
                Ok(output) => {
                    info!(
                        "compile finished for revision {} in {:.0}ms",
                        output.revision,
                        output.duration.as_secs_f64() * 1000.0
                    );

                    let output_rev = output.revision;
                    let render_id = output.compile_id.0;
                    let should_render = this
                        .update(cx, |this, _| {
                            this.controller
                                .accepts_result(output.compile_id, output.revision)
                        })
                        .unwrap_or(false);

                    if !should_render {
                        this.update(cx, |this, cx| {
                            if let Err(stale) = this.controller.handle_output(&output) {
                                info!(
                                    "discarded compile output for revision {}; current revision is {}",
                                    stale.completed_revision, stale.current_revision
                                );
                            }
                            this.finish_compile(cx);
                        })
                        .ok();
                        return;
                    }

                    // A cancel triggered between the accept check and now
                    // means a newer edit arrived; abort the raster too.
                    let render_cancelled = this
                        .update(cx, |this, _| {
                            this.compile.cancel
                                .as_ref()
                                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                        })
                        .unwrap_or(true);
                    if render_cancelled {
                        this.update(cx, |this, cx| this.finish_compile(cx))
                            .ok();
                        return;
                    }

                    let (output, render_result) = cx
                        .background_executor()
                        .spawn(async move {
                            let result = pdf_renderer.render_document(
                                render_id,
                                &output.artifact,
                                Some(&render_cancel),
                            );
                            (output, result)
                        })
                        .await;

                    match &render_result {
                        Ok(outcome) => info!(
                            "preview rendered for revision {output_rev} with {} page(s)",
                            outcome.pages.len()
                        ),
                        Err(error) => {
                            warn!("preview render failed for revision {output_rev}: {error}")
                        }
                    }

                    this.update(cx, move |this, cx| {
                        if let Err(stale) = this.controller.handle_output(&output) {
                            info!(
                                "discarded compile output for revision {}; current revision is {}",
                                stale.completed_revision, stale.current_revision
                            );
                            this.finish_compile(cx);
                            return;
                        }

                        let diags = output.diagnostics;
                        this.latest_diagnostics = diags.clone();
                        this.editor.update(cx, |editor, cx| {
                            editor.set_diagnostics(diags, cx);
                        });

                        if let Ok(outcome) = render_result {
                            this.preview.update(cx, |preview, cx| {
                                preview.set_rendered_pages(outcome.pages, outcome.notice, cx);
                            });
                        }
                        this.finish_compile(cx);
                    })
                    .ok();
                }
                Err(err) => {
                    warn!("compile failed for revision {}: {}", err.revision, err);
                    let err_summary = Some(err.message.clone());
                    let diags = err.diagnostics.clone();

                    this.update(cx, move |this, cx| {
                        if let Err(stale) = this.controller.handle_error(err) {
                            info!(
                                "discarded compile error for revision {}; current revision is {}",
                                stale.completed_revision, stale.current_revision
                            );
                            this.finish_compile(cx);
                            return;
                        }

                        this.latest_diagnostics = diags.clone();
                        this.editor.update(cx, |editor, cx| {
                            editor.set_diagnostics(diags, cx);
                        });
                        this.preview.update(cx, |preview, cx| {
                            preview.set_compile_failed(err_summary, cx);
                        });
                        this.finish_compile(cx);
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn finish_compile(&mut self, cx: &mut Context<Self>) {
        self.compile.running = false;
        if self.compile.pending {
            self.compile.pending = false;
            self.trigger_compile(cx);
        } else {
            cx.notify();
        }
    }
}
