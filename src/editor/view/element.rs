use super::*;

pub(super) struct SingleLineInputElement {
    pub(super) editor: Entity<EditorView>,
}

impl IntoElement for SingleLineInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SingleLineInputElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("single-line-input".into()))
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        style.flex_grow = 1.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
    }
}

pub(super) struct EditorElement {
    pub(super) editor: Entity<EditorView>,
}

pub(super) struct EditorPrepaintState {
    line_layouts: Vec<ShapedLine>,
    first_line: usize,
    line_height: f32,
    gutter_width: f32,
    active_line_quad: Option<PaintQuad>,
    gutter_separator_quad: PaintQuad,
    cursor_quad: Option<PaintQuad>,
    selection_quads: Vec<PaintQuad>,
    cursor_line: usize,
    is_focused: bool,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = EditorPrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("editor-element".into()))
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        style.flex_grow = 1.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let line_height = window.line_height().as_f32().max(20.0);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());

        let scroll_offset = editor.scroll_offset;
        let view_height = bounds.size.height.as_f32();
        let first_line = (scroll_offset / line_height).floor() as usize;
        let visible_count = (view_height / line_height).ceil() as usize + 1;
        let total_lines = editor.buffer.line_count();
        let last_line = (first_line + visible_count).min(total_lines);

        let gutter_width = editor.gutter_width();
        let gutter_offset = px(gutter_width + TEXT_PADDING);

        let text_color = theme::color(theme::TEXT);
        let mut line_layouts = Vec::with_capacity(visible_count);
        for line_idx in first_line..last_line {
            let content = editor.buffer.line_content(line_idx).unwrap_or("");
            let shaped = if content.is_empty() {
                let run = TextRun {
                    len: 1,
                    font: style.font(),
                    color: text_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(" ".into(), font_size, &[run], None)
            } else {
                let runs = if editor.plain_text {
                    crate::editor::syntax::plain_text_line(content, style.font())
                } else {
                    crate::editor::syntax::highlight_line(content, style.font(), editor.is_typst)
                };
                window
                    .text_system()
                    .shape_line(content.into(), font_size, &runs, None)
            };
            line_layouts.push(shaped);
        }

        let is_focused = editor.focus_handle.is_focused(window);
        let (cursor_line, cursor_col) = editor.line_col_for_offset(editor.cursor);

        let active_line_quad = if is_focused && cursor_line >= first_line && cursor_line < last_line
        {
            let y = cursor_line as f32 * line_height - scroll_offset;
            Some(fill(
                Bounds::new(
                    point(bounds.left(), bounds.top() + px(y)),
                    size(bounds.size.width, px(line_height)),
                ),
                theme::color(theme::LINE_HIGHLIGHT),
            ))
        } else {
            None
        };

        let gutter_separator_quad = fill(
            Bounds::new(
                point(bounds.left() + px(gutter_width), bounds.top()),
                size(px(1.0), bounds.size.height),
            ),
            theme::color(theme::BG),
        );

        let cursor_quad = if editor.selected_range.is_empty() && is_focused {
            if cursor_line >= first_line && cursor_line < last_line {
                let local = cursor_line - first_line;
                let x = line_layouts
                    .get(local)
                    .map_or(px(0.0), |layout| layout.x_for_index(cursor_col));
                let y = cursor_line as f32 * line_height - scroll_offset;
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + gutter_offset + x, bounds.top() + px(y)),
                        size(px(2.0), px(line_height)),
                    ),
                    theme::color(theme::TEXT),
                ))
            } else {
                None
            }
        } else {
            None
        };

        let mut selection_quads = Vec::new();
        if !editor.selected_range.is_empty() {
            let sel_start = editor.selected_range.start;
            let sel_end = editor.selected_range.end;
            let (start_line, start_col) = editor.line_col_for_offset(sel_start);
            let (end_line, end_col) = editor.line_col_for_offset(sel_end);

            for line_idx in start_line..=end_line {
                if line_idx < first_line || line_idx >= last_line {
                    continue;
                }
                let local = line_idx - first_line;
                let line_len = editor.buffer.line_content(line_idx).map_or(0, str::len);

                let col_start = if line_idx == start_line { start_col } else { 0 };
                let col_end = if line_idx == end_line {
                    end_col
                } else {
                    line_len
                };

                if let Some(layout) = line_layouts.get(local) {
                    let x1 = layout.x_for_index(col_start);
                    let x2 = layout.x_for_index(col_end);
                    let y = line_idx as f32 * line_height - scroll_offset;
                    selection_quads.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + gutter_offset + x1, bounds.top() + px(y)),
                            point(
                                bounds.left() + gutter_offset + x2,
                                bounds.top() + px(y + line_height),
                            ),
                        ),
                        rgba(theme::SELECTION),
                    ));
                }
            }
        }

        EditorPrepaintState {
            line_layouts,
            first_line,
            line_height,
            gutter_width,
            active_line_quad,
            gutter_separator_quad,
            cursor_quad,
            selection_quads,
            cursor_line,
            is_focused,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        if let Some(active_line) = prepaint.active_line_quad.take() {
            window.paint_quad(active_line);
        }

        let show_line_numbers = self.editor.read(cx).line_numbers;
        if show_line_numbers {
            window.paint_quad(prepaint.gutter_separator_quad.clone());
        }

        let lh = prepaint.line_height;
        let scroll_offset = self.editor.read(cx).scroll_offset;
        let line_height_px = px(lh);
        let gutter_width = prepaint.gutter_width;
        let gutter_offset = px(gutter_width + TEXT_PADDING);

        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let font = text_style.font();

        if show_line_numbers {
            for (i, _) in prepaint.line_layouts.iter().enumerate() {
                let line_idx = prepaint.first_line + i;
                let y = line_idx as f32 * lh - scroll_offset;
                let num_str = (line_idx + 1).to_string();
                let is_active = prepaint.is_focused && line_idx == prepaint.cursor_line;

                let has_error = self.editor.read(cx).diagnostics.iter().any(|d| {
                    d.line == Some(line_idx + 1)
                        && d.severity == crate::compiler::diagnostics::Severity::Error
                });
                let has_warn = self.editor.read(cx).diagnostics.iter().any(|d| {
                    d.line == Some(line_idx + 1)
                        && d.severity == crate::compiler::diagnostics::Severity::Warning
                });

                let line_num_color = if has_error {
                    theme::color(theme::ACCENT_RED)
                } else if has_warn {
                    theme::color(theme::ACCENT_ORANGE)
                } else if is_active {
                    theme::color(theme::TEXT)
                } else {
                    theme::color(theme::TEXT_MUTED)
                };

                let run = TextRun {
                    len: num_str.len(),
                    font: font.clone(),
                    color: line_num_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped =
                    window
                        .text_system()
                        .shape_line(num_str.into(), font_size, &[run], None);
                let gutter_x = px(gutter_width - 10.0) - shaped.width;
                shaped
                    .paint(
                        point(bounds.left() + gutter_x, bounds.top() + px(y)),
                        line_height_px,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    )
                    .ok();
            }
        }

        for quad in prepaint.selection_quads.drain(..) {
            window.paint_quad(quad);
        }

        for (i, line) in prepaint.line_layouts.iter().enumerate() {
            let line_idx = prepaint.first_line + i;
            let y = line_idx as f32 * lh - scroll_offset;
            line.paint(
                point(bounds.left() + gutter_offset, bounds.top() + px(y)),
                line_height_px,
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        }

        if let Some(cursor) = prepaint.cursor_quad.take() {
            window.paint_quad(cursor);
        }

        let line_layouts = std::mem::take(&mut prepaint.line_layouts);
        let first_line = prepaint.first_line;
        let line_height = prepaint.line_height;
        self.editor.update(cx, |editor, _| {
            editor.last_line_layouts = line_layouts;
            editor.last_first_line = first_line;
            editor.last_bounds = Some(bounds);
            editor.last_line_height = line_height;
        });
    }
}
