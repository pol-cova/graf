use super::*;

pub(super) const SIDEBAR_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 160.0..=420.0;
pub(super) const PREVIEW_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 320.0..=800.0;
pub(super) const DIAGNOSTICS_HEIGHT_RANGE: std::ops::RangeInclusive<f32> = 100.0..=500.0;

impl Workspace {
    pub fn begin_panel_resize(&mut self, panel: ResizingPanel, cx: &mut Context<Self>) {
        self.layout.resizing_panel = Some(panel);
        cx.notify();
    }

    pub(crate) fn resize_panel(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }

        match self.layout.resizing_panel {
            Some(ResizingPanel::Sidebar) => {
                self.layout.sidebar_width = event
                    .position
                    .x
                    .as_f32()
                    .clamp(*SIDEBAR_WIDTH_RANGE.start(), *SIDEBAR_WIDTH_RANGE.end());
            }
            Some(ResizingPanel::Preview) => {
                self.layout.preview_width = (window.viewport_size().width - event.position.x)
                    .as_f32()
                    .clamp(*PREVIEW_WIDTH_RANGE.start(), *PREVIEW_WIDTH_RANGE.end());
            }
            Some(ResizingPanel::Diagnostics) => {
                self.layout.diagnostics_height = (window.viewport_size().height - event.position.y)
                    .as_f32()
                    .clamp(
                        *DIAGNOSTICS_HEIGHT_RANGE.start(),
                        *DIAGNOSTICS_HEIGHT_RANGE.end(),
                    );
            }
            None => return,
        }
        cx.notify();
    }

    pub(crate) fn finish_panel_resize(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout.resizing_panel.take().is_some() {
            self.settings.layout.sidebar_width = self.layout.sidebar_width;
            self.settings.layout.preview_width = self.layout.preview_width;
            self.settings.layout.diagnostics_height = self.layout.diagnostics_height;
            self.persist_settings();
            cx.notify();
        }
    }
}
