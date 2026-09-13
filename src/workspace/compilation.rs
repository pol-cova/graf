use super::*;

impl Workspace {
    pub(super) fn on_editor_changed(&mut self, editor: Entity<EditorView>, cx: &mut Context<Self>) {
        let rev = editor.read(cx).revision();
        // Fast path for cursor/selection-only notifications: the revision is
        // unchanged, so skip document sync, recovery, and compile entirely.
        // Assist still refreshes (debounced) because the cursor moved.
        if !crate::workspace::coalesce::should_sync_editor_text(rev, self.last_synced_editor_rev) {
            self.schedule_assist_debounced(cx);
            self.refresh_outline_cache(cx);
            self.schedule_stats_refresh(cx);
            return;
        }
        // Single snapshot sync: reads editor text once, compares borrowed
        // content before cloning, clones at most once.
        self.sync_active_doc_from_editor(cx);
        // Debounced, coalesced background work. No `create_dir`, pretty
        // JSON, `fsync`, label parse, or completion compute on the UI thread
        // per keystroke.
        self.schedule_recovery_snapshot(cx, rev);
        self.schedule_assist_debounced(cx);
        // Cached outline (inline or background for large docs) and debounced
        // background stats. Render paths only read the caches.
        self.refresh_outline_cache(cx);
        self.schedule_stats_refresh(cx);

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

    /// Debounced label-index reload plus autocomplete. Captures the revision
    /// at schedule time; after a 150ms quiet period a single editor snapshot
    /// (one `to_string`) is parsed and completed on the background executor,
    /// and stale results are rejected by revision. Overwriting `assist_task`
    /// cancels the previous timer, coalescing rapid keystrokes.
    pub(crate) fn schedule_assist_debounced(&mut self, cx: &mut Context<Self>) {
        use crate::workspace::coalesce::{ASSIST_DEBOUNCE, assist_result_is_current};
        let scheduled_rev = self.editor.read(cx).revision();
        self.assist_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ASSIST_DEBOUNCE).await;
            // Single snapshot after the quiet period (one UI-thread clone).
            let snapshot: Option<(
                u64,
                String,
                usize,
                bool,
                crate::project::bibtex::BibtexIndex,
            )> = this
                .update(cx, |this, cx| {
                    let current_rev = this.editor.read(cx).revision();
                    if !assist_result_is_current(scheduled_rev, current_rev) {
                        return None;
                    }
                    let (text, cursor) = {
                        let editor = this.editor.read(cx);
                        (editor.text().to_string(), editor.cursor_offset())
                    };
                    let is_tex = this
                        .documents
                        .get(this.active_doc_idx)
                        .is_some_and(|doc| doc.title().ends_with(".tex"));
                    let bib = this.bib_index.clone();
                    Some((current_rev, text, cursor, is_tex, bib))
                })
                .ok()
                .flatten();
            let Some((snap_rev, text, cursor, is_tex, bib)) = snapshot else {
                return;
            };
            // Parse labels and compute completions off the UI thread.
            let (labels, completions) = cx
                .background_executor()
                .spawn(async move {
                    let labels = crate::project::bibtex::parse_latex_labels(&text);
                    let label_index = crate::project::bibtex::LabelIndex { labels };
                    let mut completions = if is_tex {
                        crate::editor::completion::compute_completions(
                            &text,
                            cursor,
                            &bib,
                            &label_index,
                        )
                    } else {
                        Vec::new()
                    };
                    completions.truncate(8);
                    (label_index.labels, completions)
                })
                .await;
            this.update(cx, |this, cx| {
                let current_rev = this.editor.read(cx).revision();
                if !assist_result_is_current(snap_rev, current_rev) {
                    return;
                }
                this.label_index.labels = labels;
                this.completions = completions;
                this.completion_open = is_tex && !this.completions.is_empty();
                if !is_tex {
                    this.completions.clear();
                    this.completion_open = false;
                }
                this.completion_selected = 0;
                let open = this.completion_open;
                this.editor.update(cx, |editor, _| {
                    editor.set_completion_active(open);
                });
                this.last_assist_rev = Some(snap_rev);
                cx.notify();
            })
            .ok();
        }));
    }

    pub fn trigger_compile(&mut self, cx: &mut Context<Self>) {
        if !self.active_document_is_compilable() {
            self.compile_pending = false;
            self.controller.reset();
            self.latest_diagnostics.clear();
            self.preview.update(cx, |preview, cx| preview.clear(cx));
            cx.notify();
            return;
        }

        if self.compile_running {
            self.compile_pending = true;
            return;
        }

        self.preview
            .update(cx, |preview, cx| preview.set_rendering(cx));
        // Snapshot the revision plus cheap metadata on the UI thread. The
        // editor text itself is NOT cloned here: a single `Arc<str>` snapshot
        // happens inside the spawned task (after spawn, after the revision
        // snapshot), and the shared snapshot moves to the background compile
        // with no extra UI-thread `String` clone before spawn. Debounce
        // timing (controller 150ms default / settings value) is unchanged:
        // callers still debounce before calling `trigger_compile`.
        let trigger_rev = self.editor.read(cx).revision();
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

        // Reserve the compile slot synchronously so concurrent triggers
        // coalesce via `compile_pending`. `begin_compile` (with the real
        // id/rev) happens inside the spawned task once the text snapshot
        // exists, keeping id/rev consistent for stale-revision rejection.
        self.compile_running = true;
        self.compile_pending = false;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let request_opt: Option<CompileRequest> = this
                .update(cx, |this, cx| {
                    let editor = this.editor.read(cx);
                    let snap_rev = editor.revision();
                    if snap_rev != trigger_rev {
                        info!(
                            "compile snapshot advanced from rev {trigger_rev} to rev {snap_rev}; compiling latest"
                        );
                    }
                    let text: Arc<str> = Arc::from(editor.text());
                    let request = CompileRequest::with_project(
                        text,
                        snap_rev,
                        project_root.clone(),
                        root_document.clone(),
                    );
                    this.controller.begin_compile(request.compile_id, snap_rev);
                    Some(request)
                })
                .ok()
                .flatten();
            let Some(request) = request_opt else {
                this.update(cx, |this, cx| {
                    this.finish_compile(cx);
                })
                .ok();
                return;
            };

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

                    let (output, render_result) = cx
                        .background_executor()
                        .spawn(async move {
                            let result =
                                pdf_renderer.render_document(render_id, &output.artifact);
                            (output, result)
                        })
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

                        if let Ok(pages) = render_result {
                            this.preview.update(cx, |preview, cx| {
                                preview.set_rendered_pages(pages, cx);
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
        self.compile_running = false;
        if self.compile_pending {
            self.compile_pending = false;
            self.trigger_compile(cx);
        } else {
            cx.notify();
        }
    }
}
