use super::*;

/// A delivered AI result is superseded when a newer op started before it
/// finished; the workspace keeps the newest generation it issued.
fn ai_result_superseded(delivered_generation: u64, newest_generation: u64) -> bool {
    delivered_generation != newest_generation
}

impl Workspace {
    pub fn run_ai_operation(&mut self, op: AiOperationKind, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        // Selection-first context: ops rewrite or explain what the user
        // highlighted; without a selection the full document goes in — but
        // always capped through the provider-boundary context builder.
        let (context, is_selection) = match editor.selected_text() {
            Some(selection) => (selection, true),
            None => (editor.text().to_string(), false),
        };
        let context = crate::ai::operations::build_ai_context(is_selection, &context);
        let revision = editor.revision();
        let document_id = self.documents[self.active_doc_idx].id();
        let provider = self.ai_provider.clone();
        // Per-workspace counter: no app-global generation state.
        self.ai_operation_generation += 1;
        let generation = self.ai_operation_generation;

        cx.spawn(async move |this, cx| {
            let operation = op.clone();
            let (context, result) = cx
                .background_executor()
                .spawn(async move {
                    let result = execute_operation(provider.as_ref(), &operation, &context);
                    (context, result)
                })
                .await;

            this.update(cx, |this, cx| {
                // A newer AI op started while this one ran: its result is
                // superseded, deliver nothing (the underlying request keeps
                // draining until the provider times out).
                if ai_result_superseded(generation, this.ai_operation_generation) {
                    return;
                }

                let document_changed = this.documents[this.active_doc_idx].id() != document_id
                    || this.editor.read(cx).revision() != revision;
                if !matches!(&op, AiOperationKind::GenerateDiagram { .. }) && document_changed {
                    this.workspace_error =
                        Some("The document changed before the AI operation finished".to_string());
                    this.active_modal = ActiveModal::None;
                    cx.notify();
                    return;
                }

                match result {
                    Ok(response) => match op {
                        AiOperationKind::GenerateDiagram { .. } => {
                            let json = parse_canvas_response(&response).and_then(|document| {
                                document.to_json().map_err(|error| error.to_string())
                            });
                            match json {
                                Ok(json) => {
                                    let title =
                                        format!("ai-diagram-{}.graf", this.documents.len() + 1);
                                    this.documents.push(Document::new_untitled(&title, json));
                                    this.workspace_error = None;
                                    this.active_modal = ActiveModal::None;
                                    this.activate_document(this.documents.len() - 1, cx);
                                    return;
                                }
                                Err(error) => {
                                    this.workspace_error = Some(error);
                                    this.active_modal = ActiveModal::None;
                                }
                            }
                        }
                        _ => {
                            this.workspace_error = None;
                            this.active_modal = ActiveModal::DiffReview(DiffReview::new(
                                op.label(),
                                context,
                                response,
                            ));
                        }
                    },
                    Err(error) => {
                        this.workspace_error = Some(error);
                        this.active_modal = ActiveModal::None;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_generation_delivers() {
        assert!(!ai_result_superseded(3, 3));
        assert!(ai_result_superseded(2, 3));
        assert!(ai_result_superseded(3, 4));
    }
}
