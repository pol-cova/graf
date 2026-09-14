use super::*;

impl Workspace {
    pub fn lint_academic_style(&mut self, cx: &mut Context<Self>) {
        let is_typst =
            self.active_document_kind() == Some(crate::project::document::DocumentKind::Typst);
        let text = self.editor.read(cx).text().to_string();
        let revision_at_start = self.editor.read(cx).revision();

        cx.spawn(async move |this, cx| {
            let diagnostics = cx
                .background_executor()
                .spawn(async move {
                    crate::project::linter::lint_academic_warnings_as_diagnostics(&text, is_typst)
                })
                .await;

            this.update(cx, |this, cx| {
                // A newer revision means the lint describes text the user has
                // already changed; drop it rather than mislabel lines.
                let revision_now = this.editor.read(cx).revision();
                if revision_now != revision_at_start {
                    cx.notify();
                    return;
                }

                // A clean lint replaces what compile diagnostics left behind,
                // instead of silently keeping stale entries on screen.
                this.latest_diagnostics = diagnostics.clone();
                this.editor.update(cx, |editor, cx| {
                    editor.set_diagnostics(diagnostics, cx);
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn sync_zotero_library(&mut self, cx: &mut Context<Self>) {
        let zotero_lib = crate::project::zotero::ZoteroLibrary::scan_local_storage();
        for item in zotero_lib.items {
            self.project_state.bib_index.add_entry(item.to_bib_entry());
        }
        cx.notify();
    }
}
