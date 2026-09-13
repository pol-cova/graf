use std::sync::Arc;

use super::*;

/// Visible-window cap for reshaped gutter entries per frame is naturally
/// bounded by the visible line count; the cache itself is pruned in prepaint.
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

/// Everything `paint` needs is computed once in `prepaint` and passed
/// through here, so `paint` performs no entity reads, no shaping, and no
/// diagnostics scans.
pub(super) struct EditorPrepaintState {
    line_layouts: Vec<Arc<ShapedLine>>,
    gutter_layouts: Vec<Arc<ShapedLine>>,
    first_line: usize,
    line_height: f32,
    gutter_width: f32,
    scroll_offset: f32,
    show_line_numbers: bool,
    active_line_quad: Option<PaintQuad>,
    gutter_separator_quad: PaintQuad,
    cursor_quad: Option<PaintQuad>,
    selection_quads: Vec<PaintQuad>,
    focus_handle: FocusHandle,
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
        let line_height = window.line_height().as_f32().max(20.0);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let font_size_bits = font_size.as_f32().to_bits();
        let font = style.font();

        // ---- Single entity read: snapshot everything paint will need. ----
        // Hits clone the cached `Arc` (no alloc, no hash when the global
        // revision is unchanged). Misses clone only the dirty line's content.
        let snapshot = {
            let editor = self.editor.read(cx);
            let revision = editor.buffer.revision();
            let highlight_mode = highlight_mode_for(editor.plain_text, editor.is_typst);
            let scroll_offset = editor.scroll_offset;
            let view_height = bounds.size.height.as_f32();
            let first_line = (scroll_offset / line_height).floor().max(0.0) as usize;
            let visible_count = (view_height / line_height).ceil() as usize + 1;
            let total_lines = editor.buffer.line_count().max(1);
            let first_line = first_line.min(total_lines.saturating_sub(1));
            let last_line = (first_line + visible_count).min(total_lines);
            let gutter_width = editor.gutter_width();
            let show_line_numbers = editor.line_numbers;
            let is_focused = editor.focus_handle.is_focused(window);
            let focus_handle = editor.focus_handle.clone();
            let cursor = editor.cursor;
            let selected_range = editor.selected_range.clone();
            let (cursor_line, cursor_byte_col) = line_byte_col(&editor.buffer, cursor);
            let (sel_start_line, sel_start_byte) = if selected_range.is_empty() {
                (0, 0)
            } else {
                line_byte_col(&editor.buffer, selected_range.start)
            };
            let (sel_end_line, sel_end_byte) = if selected_range.is_empty() {
                (0, 0)
            } else {
                line_byte_col(&editor.buffer, selected_range.end)
            };

            let visible_len = last_line.saturating_sub(first_line);
            let mut line_layouts: Vec<Arc<ShapedLine>> = Vec::with_capacity(visible_len);
            let mut miss_indices: Vec<usize> = Vec::new();
            let mut miss_contents: Vec<String> = Vec::new();
            let mut refreshed: Vec<usize> = Vec::new();
            for line_idx in first_line..last_line {
                let content = editor.buffer.line_content(line_idx).unwrap_or("");
                if let Some(cached) = editor.shaped_line_cache.get(&line_idx) {
                    if cached.revision == revision
                        && cached.font_size_bits == font_size_bits
                        && cached.highlight_mode == highlight_mode
                    {
                        // No edits since shaped and same style: reuse directly.
                        line_layouts.push(cached.shaped.clone());
                        continue;
                    }
                    if cached.font_size_bits == font_size_bits
                        && cached.highlight_mode == highlight_mode
                    {
                        let content_hash = hash_str(content);
                        if content_hash == cached.content_hash {
                            // Edited elsewhere; this line is unchanged.
                            line_layouts.push(cached.shaped.clone());
                            refreshed.push(line_idx);
                            continue;
                        }
                    }
                }
                miss_indices.push(line_idx);
                miss_contents.push(content.to_string());
                // Placeholder; replaced after shaping below.
                line_layouts.push(Arc::new(ShapedLine::default()));
            }

            // Selection byte spans per visible line, computed once here so
            // paint needs no buffer access and byte/char units stay consistent.
            let mut selection_spans: Vec<Option<(usize, usize)>> = vec![None; visible_len];
            if !selected_range.is_empty() {
                for (i, line_idx) in (first_line..last_line).enumerate() {
                    if line_idx < sel_start_line || line_idx > sel_end_line {
                        continue;
                    }
                    let line_text = editor.buffer.line_content(line_idx).unwrap_or("");
                    let line_len = line_text.len();
                    let raw_start = if line_idx == sel_start_line {
                        sel_start_byte.min(line_len)
                    } else {
                        0
                    };
                    let raw_end = if line_idx == sel_end_line {
                        sel_end_byte.min(line_len)
                    } else {
                        line_len
                    };
                    let col_start = snap_byte_col(line_text, raw_start);
                    let col_end = snap_byte_col(line_text, raw_end);
                    if col_start <= col_end {
                        selection_spans[i] = Some((col_start, col_end));
                    }
                }
            }

            // Gutter colors + cache hits for the visible window.
            let mut gutter_layouts: Vec<Arc<ShapedLine>> = Vec::new();
            let mut gutter_misses: Vec<(usize, u32)> = Vec::new();
            let mut gutter_slots: Vec<Option<Arc<ShapedLine>>> = vec![None; visible_len];
            if show_line_numbers {
                gutter_layouts.reserve(visible_len);
                for (i, line_idx) in (first_line..last_line).enumerate() {
                    let severity = editor.diagnostic_severity_for_line(line_idx);
                    let color = gutter_color_for(line_idx, cursor_line, is_focused, severity);
                    let key = (line_idx, color, font_size_bits);
                    if let Some(cached) = editor.gutter_cache.get(&key) {
                        gutter_slots[i] = Some(cached.clone());
                    } else {
                        gutter_misses.push((line_idx, color));
                    }
                }
            }

            // Previous frame bookkeeping for the conditional write-back.
            let prev_first_line = editor.last_first_line;
            let prev_bounds = editor.last_bounds;
            let prev_line_height = editor.last_line_height;

            Snapshot {
                revision,
                highlight_mode,
                scroll_offset,
                first_line,
                last_line,
                gutter_width,
                show_line_numbers,
                is_focused,
                focus_handle,
                cursor_line,
                cursor_byte_col,
                selected_empty: selected_range.is_empty(),
                selection_spans,
                line_slots: line_layouts,
                miss_indices,
                miss_contents,
                refreshed,
                gutter_slots,
                gutter_misses,
                prev_first_line,
                prev_bounds,
                prev_line_height,
            }
        };

        // ---- Shape dirty text lines (misses only). ----
        let text_color = theme::color(theme::TEXT);
        let mut miss_shaped: Vec<(usize, u64, Arc<ShapedLine>)> =
            Vec::with_capacity(snapshot.miss_indices.len());
        for (line_idx, owned) in snapshot
            .miss_indices
            .iter()
            .zip(snapshot.miss_contents.iter())
        {
            let shaped = if owned.is_empty() {
                let run = TextRun {
                    len: 1,
                    font: font.clone(),
                    color: text_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .shape_line(" ".into(), font_size, &[run], None)
            } else {
                let runs = if snapshot.highlight_mode == 0 {
                    crate::editor::syntax::plain_text_line(owned, font.clone())
                } else {
                    crate::editor::syntax::highlight_line(
                        owned,
                        font.clone(),
                        snapshot.highlight_mode == 2,
                    )
                };
                // Hoisted `SharedString` alloc: exactly one per dirty line.
                let text: SharedString = owned.as_str().into();
                window
                    .text_system()
                    .shape_line(text, font_size, &runs, None)
            };
            let content_hash = hash_str(owned);
            miss_shaped.push((*line_idx, content_hash, Arc::new(shaped)));
        }

        // Fill placeholders with freshly shaped lines, preserving order.
        let mut line_layouts = snapshot.line_slots;
        {
            // Rebuild by index to avoid placeholder ambiguity.
            let miss_map: std::collections::HashMap<usize, Arc<ShapedLine>> = miss_shaped
                .iter()
                .map(|(idx, _, shaped)| (*idx, shaped.clone()))
                .collect();
            for (i, line_idx) in (snapshot.first_line..snapshot.last_line).enumerate() {
                if let Some(shaped) = miss_map.get(&line_idx) {
                    line_layouts[i] = shaped.clone();
                }
            }
        }

        // ---- Shape dirty gutter numbers (misses only, in prepaint). ----
        let mut gutter_miss_shaped: Vec<((usize, u32, u32), Arc<ShapedLine>)> =
            Vec::with_capacity(snapshot.gutter_misses.len());
        for (line_idx, color) in snapshot.gutter_misses.iter() {
            let num_str = (line_idx + 1).to_string();
            let run = TextRun {
                len: num_str.len(),
                font: font.clone(),
                color: theme::color(*color).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(num_str.into(), font_size, &[run], None);
            gutter_miss_shaped.push(((*line_idx, *color, font_size_bits), Arc::new(shaped)));
        }
        let mut gutter_layouts: Vec<Arc<ShapedLine>> =
            Vec::with_capacity(snapshot.gutter_slots.len());
        if snapshot.show_line_numbers {
            let gutter_miss_map: std::collections::HashMap<(usize, u32, u32), Arc<ShapedLine>> =
                gutter_miss_shaped
                    .iter()
                    .map(|(key, shaped)| (*key, shaped.clone()))
                    .collect();
            for (i, line_idx) in (snapshot.first_line..snapshot.last_line).enumerate() {
                if let Some(hit) = snapshot.gutter_slots[i].clone() {
                    gutter_layouts.push(hit);
                } else {
                    // Find the miss entry for this line (color is unique per line here).
                    let found = gutter_miss_map.iter().find_map(|((idx, _, _), shaped)| {
                        (*idx == line_idx).then(|| shaped.clone())
                    });
                    gutter_layouts.push(found.unwrap_or_else(|| {
                        // Fallback: should not happen; shape inline to keep paint total.
                        let num_str = (line_idx + 1).to_string();
                        let run = TextRun {
                            len: num_str.len(),
                            font: font.clone(),
                            color: theme::color(theme::TEXT_MUTED).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        Arc::new(window.text_system().shape_line(
                            num_str.into(),
                            font_size,
                            &[run],
                            None,
                        ))
                    }));
                }
            }
        }

        // ---- Quads from byte-based columns (no bytes/char mixing). ----
        let gutter_offset = px(snapshot.gutter_width + TEXT_PADDING);
        let is_focused = snapshot.is_focused;
        let cursor_line = snapshot.cursor_line;

        let active_line_quad =
            if is_focused && cursor_line >= snapshot.first_line && cursor_line < snapshot.last_line
            {
                let y = cursor_line as f32 * line_height - snapshot.scroll_offset;
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
                point(bounds.left() + px(snapshot.gutter_width), bounds.top()),
                size(px(1.0), bounds.size.height),
            ),
            theme::color(theme::BG),
        );

        let cursor_quad = if !snapshot.selected_empty || !is_focused {
            None
        } else if cursor_line >= snapshot.first_line && cursor_line < snapshot.last_line {
            let local = cursor_line - snapshot.first_line;
            let x = line_layouts.get(local).map_or(px(0.0), |layout| {
                layout.x_for_index(snapshot.cursor_byte_col)
            });
            let y = cursor_line as f32 * line_height - snapshot.scroll_offset;
            Some(fill(
                Bounds::new(
                    point(bounds.left() + gutter_offset + x, bounds.top() + px(y)),
                    size(px(2.0), px(line_height)),
                ),
                theme::color(theme::TEXT),
            ))
        } else {
            None
        };

        let mut selection_quads = Vec::new();
        if !snapshot.selected_empty {
            for (i, span) in snapshot.selection_spans.iter().enumerate() {
                let Some((col_start, col_end)) = span else {
                    continue;
                };
                let Some(layout) = line_layouts.get(i) else {
                    continue;
                };
                let line_idx = snapshot.first_line + i;
                let x1 = layout.x_for_index(*col_start);
                let x2 = layout.x_for_index(*col_end);
                let y = line_idx as f32 * line_height - snapshot.scroll_offset;
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

        // ---- Single write-back: caches + last-frame bookkeeping. ----
        // Conditional so idle frames (no misses, same window) skip mutation.
        let range_changed = snapshot.first_line != snapshot.prev_first_line
            || snapshot.prev_bounds != Some(bounds)
            || (snapshot.prev_line_height - line_height).abs() > f32::EPSILON;
        let needs_cache_write = !miss_shaped.is_empty()
            || !snapshot.refreshed.is_empty()
            || !gutter_miss_shaped.is_empty();
        if range_changed || needs_cache_write {
            let revision = snapshot.revision;
            let highlight_mode = snapshot.highlight_mode;
            let first_line = snapshot.first_line;
            let last_line = snapshot.last_line;
            let last_layouts = line_layouts.clone();
            self.editor.update(cx, |editor, _| {
                for (line_idx, content_hash, shaped) in &miss_shaped {
                    editor.shaped_line_cache.insert(
                        *line_idx,
                        CachedShapedLine {
                            revision,
                            content_hash: *content_hash,
                            font_size_bits,
                            highlight_mode,
                            shaped: shaped.clone(),
                        },
                    );
                }
                for line_idx in &snapshot.refreshed {
                    if let Some(entry) = editor.shaped_line_cache.get_mut(line_idx) {
                        entry.revision = revision;
                    }
                }
                for (key, shaped) in &gutter_miss_shaped {
                    editor.gutter_cache.insert(*key, shaped.clone());
                }
                if editor.shaped_line_cache.len() > MAX_SHAPED_LINE_CACHE {
                    editor.shaped_line_cache.retain(|line_idx, _| {
                        *line_idx >= first_line.saturating_sub(64) && *line_idx < last_line + 64
                    });
                    if editor.shaped_line_cache.len() > MAX_SHAPED_LINE_CACHE {
                        editor.shaped_line_cache.clear();
                        for (i, shaped) in (first_line..last_line).zip(last_layouts.iter()) {
                            // Re-seed with current window; hash recomputed next edit.
                            editor.shaped_line_cache.insert(
                                i,
                                CachedShapedLine {
                                    revision,
                                    content_hash: 0,
                                    font_size_bits,
                                    highlight_mode,
                                    shaped: shaped.clone(),
                                },
                            );
                        }
                    }
                }
                if editor.gutter_cache.len() > MAX_GUTTER_CACHE {
                    editor.gutter_cache.retain(|(line_idx, _, _), _| {
                        *line_idx >= first_line.saturating_sub(64) && *line_idx < last_line + 64
                    });
                }
                // Preserve the last valid layout window for hit-testing and IME.
                editor.last_line_layouts = last_layouts;
                editor.last_first_line = first_line;
                editor.last_bounds = Some(bounds);
                editor.last_line_height = line_height;
            });
        }

        EditorPrepaintState {
            line_layouts,
            gutter_layouts,
            first_line: snapshot.first_line,
            line_height,
            gutter_width: snapshot.gutter_width,
            scroll_offset: snapshot.scroll_offset,
            show_line_numbers: snapshot.show_line_numbers,
            active_line_quad,
            gutter_separator_quad,
            cursor_quad,
            selection_quads,
            focus_handle: snapshot.focus_handle,
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
        // No entity reads here: everything was snapshotted into `prepaint`.
        // (`self.editor.clone()` below is just an `Entity` handle, not a read.)
        window.handle_input(
            &prepaint.focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        if let Some(active_line) = prepaint.active_line_quad.take() {
            window.paint_quad(active_line);
        }

        if prepaint.show_line_numbers {
            window.paint_quad(prepaint.gutter_separator_quad.clone());
        }

        let lh = prepaint.line_height;
        let scroll_offset = prepaint.scroll_offset;
        let line_height_px = px(lh);
        let gutter_offset = px(prepaint.gutter_width + TEXT_PADDING);

        if prepaint.show_line_numbers {
            for (i, shaped) in prepaint.gutter_layouts.iter().enumerate() {
                let line_idx = prepaint.first_line + i;
                let y = line_idx as f32 * lh - scroll_offset;
                let gutter_x = px(prepaint.gutter_width - 10.0) - shaped.width;
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

        // Intentionally no `update()` here: `last_*` bookkeeping moved to the
        // conditional write-back in `prepaint`, so idle paints mutate nothing.
    }
}

/// Local snapshot so `prepaint` reads the entity exactly once.
struct Snapshot {
    revision: u64,
    highlight_mode: u8,
    scroll_offset: f32,
    first_line: usize,
    last_line: usize,
    gutter_width: f32,
    show_line_numbers: bool,
    is_focused: bool,
    focus_handle: FocusHandle,
    cursor_line: usize,
    cursor_byte_col: usize,
    selected_empty: bool,
    selection_spans: Vec<Option<(usize, usize)>>,
    line_slots: Vec<Arc<ShapedLine>>,
    miss_indices: Vec<usize>,
    miss_contents: Vec<String>,
    refreshed: Vec<usize>,
    gutter_slots: Vec<Option<Arc<ShapedLine>>>,
    gutter_misses: Vec<(usize, u32)>,
    prev_first_line: usize,
    prev_bounds: Option<Bounds<Pixels>>,
    prev_line_height: f32,
}
