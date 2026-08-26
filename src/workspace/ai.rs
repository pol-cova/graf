use super::*;

impl Workspace {
    pub fn run_ai_operation(&mut self, op: AiOperationKind, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        let context = editor.text().to_string();
        let revision = editor.revision();
        let document_id = self.documents[self.active_doc_idx].id();
        let provider = self.ai_provider.clone();

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
