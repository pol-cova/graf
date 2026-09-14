use super::*;

impl Workspace {
    /// Export commands must not run while a text document is on screen: the
    /// shared canvas holds the last-loaded .graf scene, and exporting that
    /// would silently copy another file's diagram to the clipboard.
    fn assert_canvas_export_target(&mut self) -> bool {
        if self.active_view_kind != ActiveViewKind::Canvas {
            self.workspace_error =
                Some("Canvas export requires an active .graf document".to_string());
            return false;
        }
        true
    }

    pub fn export_canvas_to_tikz(&mut self, cx: &mut Context<Self>) {
        if !self.assert_canvas_export_target() {
            return;
        }
        let doc = self.canvas.read(cx).document();
        let tikz_code = crate::canvas::tikz::export_to_tikz(doc);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(tikz_code));
    }

    pub fn export_canvas_to_svg(&mut self, cx: &mut Context<Self>) {
        if !self.assert_canvas_export_target() {
            return;
        }
        let doc = self.canvas.read(cx).document();
        let svg_code = crate::canvas::svg::export_to_svg(doc);
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(svg_code));
    }
}
