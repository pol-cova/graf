use super::*;

impl Workspace {
    pub fn new_typst_document(&mut self, cx: &mut Context<Self>) {
        let initial_typst = crate::project::templates::DEFAULT_TYPST_STARTER;
        let doc_name = self.next_document_title(DraftKind::Typst);
        self.documents
            .push(Document::new_untitled(&doc_name, initial_typst));
        self.activate_document(self.documents.len() - 1, cx);
    }

    pub fn new_canvas_diagram(&mut self, cx: &mut Context<Self>) {
        let default_canvas_json = match self.canvas.read(cx).save_to_json() {
            Ok(json) => json,
            Err(error) => {
                self.workspace_error = Some(format!("Could not create diagram: {error}"));
                cx.notify();
                return;
            }
        };
        let doc_name = self.next_document_title(DraftKind::Diagram);
        let doc = Document::new_untitled(&doc_name, default_canvas_json);
        let new_diagram_id = doc.id();
        self.documents.push(doc);
        self.active_doc_idx = self.documents.len() - 1;
        self.active_view_kind = ActiveViewKind::Canvas;
        // The scene the canvas currently displays is the new document's
        // starting content, and the undo trail held in the view now belongs
        // to that new document.
        self.history_store.retitle(new_diagram_id);
        cx.notify();
    }

    pub fn insert_table_template(&mut self, cx: &mut Context<Self>) {
        let is_typst =
            self.active_document_kind() == Some(crate::project::document::DocumentKind::Typst);
        let table = crate::editor::table::TableData::sample();

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

    /// Free title among open documents for a new draft of `kind`.
    pub(crate) fn next_document_title(&self, kind: DraftKind) -> String {
        let titles: Vec<String> = self
            .documents
            .iter()
            .map(|document| document.title().to_string())
            .collect();
        next_draft_title(kind, &titles)
    }
}
