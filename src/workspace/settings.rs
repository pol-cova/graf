use super::*;

impl Workspace {
    pub(crate) fn persist_settings(&self) {
        let Some(path) = GrafSettings::default_path() else {
            return;
        };
        if let Err(error) = self.settings.save_to_path(&path) {
            warn!("failed to save settings to {}: {error}", path.display());
        }
    }

    fn apply_editor_settings(&mut self, cx: &mut Context<Self>) {
        let editor = &self.settings.editor;
        self.editor.update(cx, |view, cx| {
            view.set_preferences(editor.font_size, editor.tab_size, editor.line_numbers, cx);
        });
        self.controller
            .set_debounce_duration(std::time::Duration::from_millis(editor.compile_debounce_ms));
        self.persist_settings();
        cx.notify();
    }

    pub fn adjust_editor_font_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.settings.editor.font_size =
            (self.settings.editor.font_size + delta).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.apply_editor_settings(cx);
    }

    pub fn cycle_tab_size(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.tab_size = match self.settings.editor.tab_size {
            1 | 2 => 4,
            3..=7 => MAX_TAB_SIZE,
            _ => 2,
        };
        self.apply_editor_settings(cx);
    }

    pub fn toggle_line_numbers_setting(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.line_numbers = !self.settings.editor.line_numbers;
        self.apply_editor_settings(cx);
    }

    pub fn toggle_auto_compile_setting(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.auto_compile = !self.settings.editor.auto_compile;
        self.apply_editor_settings(cx);
    }

    pub fn cycle_compile_debounce(&mut self, cx: &mut Context<Self>) {
        self.settings.editor.compile_debounce_ms = match self.settings.editor.compile_debounce_ms {
            0..=150 => 300,
            151..=300 => 500,
            301..=500 => 750,
            _ => 150,
        };
        self.apply_editor_settings(cx);
    }
}
