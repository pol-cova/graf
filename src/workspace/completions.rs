use super::*;

impl Workspace {
    pub fn trigger_autocomplete(&mut self, cx: &mut Context<Self>) {
        if self.active_document_kind() != Some(crate::project::document::DocumentKind::Latex) {
            self.completions.clear();
            self.completion_open = false;
            self.editor
                .update(cx, |editor, _| editor.set_completion_active(false));
            cx.notify();
            return;
        }

        let (text, cursor) = {
            let ed = self.editor.read(cx);
            (ed.text().to_string(), ed.cursor_offset())
        };
        self.completions = crate::editor::completion::compute_completions(
            &text,
            cursor,
            &self.project_state.bib_index,
            &self.project_state.label_index,
        );
        self.completions.truncate(8);
        self.completion_open = !self.completions.is_empty();
        self.completion_selected = 0;
        self.editor.update(cx, |editor, _| {
            editor.set_completion_active(self.completion_open);
        });
        cx.notify();
    }

    pub fn apply_completion(
        &mut self,
        item: &crate::editor::completion::CompletionItem,
        cx: &mut Context<Self>,
    ) {
        let insert = item.insert_text.clone();
        self.editor.update(cx, |ed, cx| {
            ed.insert_snippet(&insert, cx);
        });
        self.completion_open = false;
        self.editor
            .update(cx, |editor, _| editor.set_completion_active(false));
        cx.notify();
    }

    pub(crate) fn on_editor_event(&mut self, event: EditorEvent, cx: &mut Context<Self>) {
        match event {
            EditorEvent::NextCompletion => {
                if !self.completions.is_empty() {
                    self.completion_selected =
                        (self.completion_selected + 1) % self.completions.len();
                }
            }
            EditorEvent::PreviousCompletion => {
                if !self.completions.is_empty() {
                    self.completion_selected = self
                        .completion_selected
                        .checked_sub(1)
                        .unwrap_or(self.completions.len() - 1);
                }
            }
            EditorEvent::AcceptCompletion => {
                if let Some(item) = self.completions.get(self.completion_selected).cloned() {
                    self.apply_completion(&item, cx);
                    return;
                }
            }
            EditorEvent::FindReferences => {
                self.find_all_references(cx);
                return;
            }
        }
        cx.notify();
    }

    pub(crate) fn find_all_references(&mut self, cx: &mut Context<Self>) {
        let Some(reference) = self.editor.read(cx).reference_at_cursor() else {
            return;
        };
        let content = self.editor.read(cx).text().to_string();
        self.find_state.set_query(reference.clone(), &content);
        self.find_bar_open = true;
        self.prompt_target = state::PromptTarget::Find;
        self.prompt_editor
            .update(cx, |input, cx| input.set_input_text(reference, cx));
        if let Some(matched) = self.find_state.next_match().cloned() {
            self.editor
                .update(cx, |editor, cx| editor.select_range(matched, cx));
        }
        cx.notify();
    }
}
