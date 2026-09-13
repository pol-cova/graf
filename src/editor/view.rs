mod element;
mod input;

use self::element::{EditorElement, SingleLineInputElement};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    Role, ScrollWheelEvent, ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window,
    actions, div, fill, point, prelude::*, px, relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

use super::buffer::TextBuffer;
use crate::ui::theme;

pub(crate) const MIN_FONT_SIZE: f32 = 10.0;
pub(crate) const MAX_FONT_SIZE: f32 = 24.0;
pub(crate) const MIN_TAB_SIZE: usize = 1;
pub(crate) const MAX_TAB_SIZE: usize = 8;

actions!(
    editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        MoveWordLeft,
        MoveWordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteLine,
        ToggleComment,
        Home,
        End,
        MoveToBeginning,
        MoveToEnd,
        PageUp,
        PageDown,
        Paste,
        Cut,
        Copy,
        Undo,
        Redo,
        Enter,
        Tab,
        ToggleBold,
        ToggleItalic,
        ToggleMath,
        ShowCharacterPalette,
    ]
);

#[derive(Debug, Clone, Copy)]
pub enum EditorEvent {
    NextCompletion,
    PreviousCompletion,
    AcceptCompletion,
    FindReferences,
}

pub fn register_bindings(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("Editor")),
        KeyBinding::new("delete", Delete, Some("Editor")),
        KeyBinding::new("left", Left, Some("Editor")),
        KeyBinding::new("right", Right, Some("Editor")),
        KeyBinding::new("up", Up, Some("Editor")),
        KeyBinding::new("down", Down, Some("Editor")),
        KeyBinding::new("shift-left", SelectLeft, Some("Editor")),
        KeyBinding::new("shift-right", SelectRight, Some("Editor")),
        KeyBinding::new("shift-up", SelectUp, Some("Editor")),
        KeyBinding::new("shift-down", SelectDown, Some("Editor")),
        KeyBinding::new("alt-left", MoveWordLeft, Some("Editor")),
        KeyBinding::new("alt-right", MoveWordRight, Some("Editor")),
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some("Editor")),
        KeyBinding::new("alt-shift-right", SelectWordRight, Some("Editor")),
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some("Editor")),
        KeyBinding::new("alt-delete", DeleteWordRight, Some("Editor")),
        KeyBinding::new("cmd-backspace", DeleteLine, Some("Editor")),
        KeyBinding::new("cmd-/", ToggleComment, Some("Editor")),
        KeyBinding::new("cmd-b", ToggleBold, Some("Editor")),
        KeyBinding::new("cmd-e", ToggleItalic, Some("Editor")),
        KeyBinding::new("cmd-m", ToggleMath, Some("Editor")),
        KeyBinding::new("cmd-a", SelectAll, Some("Editor")),
        KeyBinding::new("home", Home, Some("Editor")),
        KeyBinding::new("cmd-left", Home, Some("Editor")),
        KeyBinding::new("end", End, Some("Editor")),
        KeyBinding::new("cmd-right", End, Some("Editor")),
        KeyBinding::new("cmd-up", MoveToBeginning, Some("Editor")),
        KeyBinding::new("cmd-down", MoveToEnd, Some("Editor")),
        KeyBinding::new("pageup", PageUp, Some("Editor")),
        KeyBinding::new("pagedown", PageDown, Some("Editor")),
        KeyBinding::new("cmd-v", Paste, Some("Editor")),
        KeyBinding::new("cmd-c", Copy, Some("Editor")),
        KeyBinding::new("cmd-x", Cut, Some("Editor")),
        KeyBinding::new("cmd-z", Undo, Some("Editor")),
        KeyBinding::new("cmd-shift-z", Redo, Some("Editor")),
        KeyBinding::new("enter", Enter, Some("Editor")),
        KeyBinding::new("tab", Tab, Some("Editor")),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, Some("Editor")),
    ]);
}

pub struct EditorView {
    focus_handle: FocusHandle,
    buffer: TextBuffer,

    cursor: usize,
    selected_range: Range<usize>,
    selection_reversed: bool,

    marked_range: Option<Range<usize>>,

    scroll_offset: f32,

    is_selecting: bool,

    last_line_layouts: Vec<Arc<ShapedLine>>,
    last_first_line: usize,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_height: f32,
    goal_x: Option<f32>,
    diagnostics: Vec<crate::compiler::diagnostics::Diagnostic>,
    /// 0-indexed line -> worst severity on that line. Rebuilt once per
    /// `set_diagnostics` so prepaint/paint do O(1) lookups instead of
    /// O(V*D) scans.
    diagnostic_severity_by_line: HashMap<usize, crate::compiler::diagnostics::Severity>,
    /// Visible-line shaping cache keyed by line index. Hit when the stored
    /// content hash + style fingerprint match, so scrolling / focus changes
    /// reuse `Arc<ShapedLine>` without reshaping or `SharedString` allocs.
    /// Only dirty lines (hash mismatch) are reshaped.
    shaped_line_cache: HashMap<usize, CachedShapedLine>,
    /// Gutter number cache keyed by (line, color, font_size_bits).
    /// Shaped once in prepaint, painted without reshaping.
    gutter_cache: HashMap<(usize, u32, u32), Arc<ShapedLine>>,
    /// Single-line overlay text. Keyed by buffer revision (plus length +
    /// content hash for safety); cloned as `Arc` on hits so `render()`
    /// performs no full-doc `to_string` when nothing changed.
    single_line_cached: Option<(u64, u64, SharedString)>,
    pub is_typst: bool,
    plain_text: bool,
    font_size: f32,
    tab_size: usize,
    line_numbers: bool,
    completion_active: bool,
    context_menu_position: Option<(f32, f32)>,
    single_line: bool,
}

/// Cached shaped line with the keys needed to decide a hit without
/// reshaping. `revision` is the buffer revision at shape time: when it
/// matches the current revision the content at this index cannot have
/// changed, so the cached `Arc` is reused without even hashing.
#[derive(Clone)]
pub(crate) struct CachedShapedLine {
    pub revision: u64,
    pub content_hash: u64,
    pub font_size_bits: u32,
    pub highlight_mode: u8,
    pub shaped: Arc<ShapedLine>,
}

/// Upper bounds keep the caches from growing without limit; prepaint prunes
/// out-of-window entries once exceeded (LRU-ish by visibility).
pub(crate) const MAX_SHAPED_LINE_CACHE: usize = 512;
pub(crate) const MAX_GUTTER_CACHE: usize = 512;

/// 0 = plain text, 1 = LaTeX, 2 = Typst. Part of the shaping fingerprint.
#[inline]
pub(crate) fn highlight_mode_for(plain_text: bool, is_typst: bool) -> u8 {
    if plain_text {
        0
    } else if is_typst {
        2
    } else {
        1
    }
}

#[inline]
pub(crate) fn hash_str(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Build the 0-indexed line -> worst-severity map once per diagnostics
/// update. Errors dominate warnings on the same line.
pub(crate) fn build_diagnostic_severity_map(
    diags: &[crate::compiler::diagnostics::Diagnostic],
) -> HashMap<usize, crate::compiler::diagnostics::Severity> {
    use crate::compiler::diagnostics::Severity;
    let mut map = HashMap::new();
    for diag in diags {
        if let Some(one_indexed) = diag.line {
            let line_0 = one_indexed.saturating_sub(1);
            map.entry(line_0)
                .and_modify(|existing: &mut Severity| {
                    if *existing != Severity::Error && diag.severity == Severity::Error {
                        *existing = Severity::Error;
                    }
                })
                .or_insert(diag.severity);
        }
    }
    map
}

/// Resolve the gutter number color so prepaint can cache by the resolved
/// value instead of re-scanning diagnostics per line in paint.
pub(crate) fn gutter_color_for(
    line_idx: usize,
    cursor_line: usize,
    is_focused: bool,
    severity: Option<crate::compiler::diagnostics::Severity>,
) -> u32 {
    use crate::compiler::diagnostics::Severity;
    match severity {
        Some(Severity::Error) => crate::ui::theme::ACCENT_RED,
        Some(Severity::Warning) => crate::ui::theme::ACCENT_ORANGE,
        None => {
            if is_focused && line_idx == cursor_line {
                crate::ui::theme::TEXT
            } else {
                crate::ui::theme::TEXT_MUTED
            }
        }
    }
}

/// Clamp a UTF-8 byte column to the line and snap down to a char boundary.
#[inline]
pub(crate) fn snap_byte_col(line: &str, byte_col: usize) -> usize {
    let mut clamped = byte_col.min(line.len());
    while clamped > 0 && !line.is_char_boundary(clamped) {
        clamped -= 1;
    }
    clamped
}

/// Byte-column of a buffer offset relative to its line start.
/// `ShapedLine::x_for_index` / `closest_index_for_x` both use UTF-8 byte
/// indices, so shaping paths must use this (not the char column).
#[inline]
pub(crate) fn line_byte_col(buffer: &TextBuffer, offset: usize) -> (usize, usize) {
    let line = buffer.line_of_offset(offset);
    let start = buffer.line_start_offset(line);
    (line, offset.saturating_sub(start))
}

/// Inverse of [`line_byte_col`]: byte column clamped + snapped to boundary.
pub(crate) fn offset_for_line_byte_col(buffer: &TextBuffer, line: usize, byte_col: usize) -> usize {
    let line = line.min(buffer.line_count().saturating_sub(1));
    let start = buffer.line_start_offset(line);
    let content = buffer.line_content(line).unwrap_or("");
    start + snap_byte_col(content, byte_col)
}

impl EventEmitter<EditorEvent> for EditorView {}

impl EditorView {
    pub fn set_is_typst(&mut self, is_typst: bool, cx: &mut Context<Self>) {
        if self.is_typst != is_typst {
            self.is_typst = is_typst;
            self.shaped_line_cache.clear();
            cx.notify();
        }
    }

    pub fn set_plain_text(&mut self, plain_text: bool, cx: &mut Context<Self>) {
        if self.plain_text != plain_text {
            self.plain_text = plain_text;
            self.shaped_line_cache.clear();
            cx.notify();
        }
    }

    pub fn with_text(cx: &mut Context<Self>, text: impl Into<String>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buffer: TextBuffer::from_text(text),
            cursor: 0,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            scroll_offset: 0.0,
            is_selecting: false,
            last_line_layouts: Vec::new(),
            last_first_line: 0,
            last_bounds: None,
            last_line_height: 22.0,
            goal_x: None,
            diagnostics: Vec::new(),
            diagnostic_severity_by_line: HashMap::new(),
            shaped_line_cache: HashMap::new(),
            gutter_cache: HashMap::new(),
            single_line_cached: None,
            is_typst: false,
            plain_text: false,
            font_size: 14.0,
            tab_size: 2,
            line_numbers: true,
            completion_active: false,
            context_menu_position: None,
            single_line: false,
        }
    }

    pub fn set_single_line(&mut self, single_line: bool) {
        self.single_line = single_line;
    }

    pub fn set_preferences(
        &mut self,
        font_size: f32,
        tab_size: usize,
        line_numbers: bool,
        cx: &mut Context<Self>,
    ) {
        let new_font_size = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let new_tab_size = tab_size.clamp(MIN_TAB_SIZE, MAX_TAB_SIZE);
        let font_changed = (new_font_size - self.font_size).abs() > f32::EPSILON;
        if font_changed || new_tab_size != self.tab_size || line_numbers != self.line_numbers {
            self.font_size = new_font_size;
            self.tab_size = new_tab_size;
            self.line_numbers = line_numbers;
            if font_changed {
                self.shaped_line_cache.clear();
                self.gutter_cache.clear();
            }
            cx.notify();
        }
    }

    pub fn set_diagnostics(
        &mut self,
        diags: Vec<crate::compiler::diagnostics::Diagnostic>,
        cx: &mut Context<Self>,
    ) {
        self.diagnostics = diags;
        self.diagnostic_severity_by_line = build_diagnostic_severity_map(&self.diagnostics);
        // Severity feeds the gutter color key; drop stale shaped numbers.
        self.gutter_cache.clear();
        cx.notify();
    }

    /// O(1) severity lookup for a 0-indexed line.
    pub(crate) fn diagnostic_severity_for_line(
        &self,
        line_idx: usize,
    ) -> Option<crate::compiler::diagnostics::Severity> {
        self.diagnostic_severity_by_line.get(&line_idx).copied()
    }

    /// Cached single-line text for the overlay in `render()`.
    ///
    /// Keyed by buffer revision (cleared on `set_text`, which resets the
    /// revision) with the content hash stored alongside for safety. Hits
    /// clone the inner `Arc` without touching the document; misses allocate
    /// exactly once. Empty content returns a static string without alloc.
    pub(crate) fn single_line_shared(&mut self) -> SharedString {
        let revision = self.buffer.revision();
        if let Some((cached_revision, _, cached)) = &self.single_line_cached
            && *cached_revision == revision
        {
            return cached.clone();
        }
        let content = self.buffer.content();
        let shared: SharedString = if content.is_empty() {
            "".into()
        } else {
            content.into()
        };
        let content_hash = hash_str(content);
        self.single_line_cached = Some((revision, content_hash, shared.clone()));
        shared
    }

    pub fn gutter_width(&self) -> f32 {
        if !self.line_numbers {
            return 0.0;
        }
        let line_count = self.buffer.line_count();
        let digits = line_count.to_string().len().max(2);
        (digits as f32 * 9.0 + 26.0).max(48.0)
    }

    pub fn jump_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let line_0 = line.saturating_sub(1);
        let offset = self.buffer.line_start_offset(line_0);
        self.move_to(offset, cx);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn select_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let start = range.start.min(self.buffer.len());
        let end = range.end.min(self.buffer.len()).max(start);
        self.selected_range = start..end;
        self.cursor = end;
        self.selection_reversed = false;
        self.goal_x = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn insert_text(&mut self, text: &str) {
        self.context_menu_position = None;
        self.buffer.begin_transaction(self.cursor);

        if !self.selected_range.is_empty() {
            self.buffer.delete(self.selected_range.clone());
            self.cursor = self.selected_range.start;
            self.selected_range = self.cursor..self.cursor;
        }

        let (insert_payload, move_delta) = match text {
            "{" => ("{}", 1),
            "(" => ("()", 1),
            "[" => ("[]", 1),
            "$" => ("$$", 1),
            _ => (text, text.len()),
        };

        self.buffer.insert(self.cursor, insert_payload);
        self.cursor += move_delta;
        self.selected_range = self.cursor..self.cursor;
        self.buffer.end_transaction(self.cursor);
        self.goal_x = None;
    }

    fn delete_range(&mut self, range: Range<usize>) {
        if range.is_empty() {
            return;
        }
        self.buffer.begin_transaction(self.cursor);
        self.buffer.delete(range.clone());
        self.cursor = range.start;
        self.selected_range = self.cursor..self.cursor;
        self.buffer.end_transaction(self.cursor);
        self.goal_x = None;
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.cursor = offset.min(self.buffer.len());
        self.selected_range = self.cursor..self.cursor;
        self.selection_reversed = false;
        self.goal_x = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = offset.min(self.buffer.len());
        let anchor = if self.selection_reversed {
            self.selected_range.end
        } else {
            self.selected_range.start
        };

        if offset < anchor {
            self.selected_range = offset..anchor;
            self.selection_reversed = true;
        } else {
            self.selected_range = anchor..offset;
            self.selection_reversed = false;
        }
        self.cursor = offset;
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.buffer
            .content()
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.buffer
            .content()
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.buffer.len())
    }

    fn previous_word_boundary(&self, offset: usize) -> usize {
        let content = self.buffer.content();
        let safe_offset = offset.min(content.len());
        content[..safe_offset]
            .split_word_bound_indices()
            .rev()
            .find_map(|(idx, word)| {
                if idx < safe_offset && !word.trim().is_empty() {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    fn next_word_boundary(&self, offset: usize) -> usize {
        let content = self.buffer.content();
        if offset >= content.len() {
            return content.len();
        }
        content[offset..]
            .split_word_bound_indices()
            .find_map(|(rel_idx, word)| {
                let abs_idx = offset + rel_idx;
                if abs_idx > offset && !word.trim().is_empty() {
                    Some(abs_idx)
                } else {
                    None
                }
            })
            .unwrap_or(content.len())
    }

    fn line_col_for_offset(&self, offset: usize) -> (usize, usize) {
        let line = self.buffer.line_of_offset(offset);
        let line_start = self.buffer.line_start_offset(line);
        let line_str = self.buffer.line_content(line).unwrap_or("");
        let rel_byte = offset.saturating_sub(line_start).min(line_str.len());
        let mut safe_rel = rel_byte;
        while safe_rel > 0 && !line_str.is_char_boundary(safe_rel) {
            safe_rel -= 1;
        }
        let char_col = line_str[..safe_rel].chars().count();
        (line, char_col)
    }

    fn offset_for_line_col(&self, line: usize, char_col: usize) -> usize {
        let line = line.min(self.buffer.line_count().saturating_sub(1));
        let line_start = self.buffer.line_start_offset(line);
        let line_str = self.buffer.line_content(line).unwrap_or("");
        let mut byte_offset = 0;
        for (i, ch) in line_str.chars().enumerate() {
            if i >= char_col {
                break;
            }
            byte_offset += ch.len_utf8();
        }
        line_start + byte_offset
    }

    fn move_vertically(&mut self, delta: isize, extend_selection: bool, cx: &mut Context<Self>) {
        let current = self.cursor_offset();
        let (line, col) = self.line_col_for_offset(current);

        let target_col = self.goal_x.map(|gx| gx as usize).unwrap_or(col);
        let goal_x = self.goal_x.unwrap_or(col as f32);

        let new_line = if delta < 0 {
            line.saturating_sub((-delta) as usize)
        } else {
            (line + delta as usize).min(self.buffer.line_count().saturating_sub(1))
        };

        let new_offset = self.offset_for_line_col(new_line, target_col);

        if extend_selection {
            self.select_to(new_offset, cx);
        } else {
            self.move_to(new_offset, cx);
        }
        self.goal_x = Some(goal_x);
    }

    fn visible_lines(&self) -> usize {
        if self.last_line_height <= 0.0 {
            return 30;
        }
        let height = self.last_bounds.map_or(600.0, |b| b.size.height.as_f32());
        (height / self.last_line_height).ceil() as usize
    }

    fn ensure_cursor_visible(&mut self) {
        let (cursor_line, _) = self.line_col_for_offset(self.cursor);
        let lh = self.last_line_height;
        if lh <= 0.0 {
            return;
        }
        let cursor_top = cursor_line as f32 * lh;
        let cursor_bot = cursor_top + lh;

        if cursor_top < self.scroll_offset {
            self.scroll_offset = cursor_top;
        }
        let view_h = self.last_bounds.map_or(600.0, |b| b.size.height.as_f32());
        if cursor_bot > self.scroll_offset + view_h {
            self.scroll_offset = cursor_bot - view_h;
        }
        self.scroll_offset = self.scroll_offset.max(0.0);
    }

    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        let lh = self.last_line_height;
        if lh <= 0.0 {
            return 0;
        }

        let local_y = (position.y.as_f32() - bounds.top().as_f32() + self.scroll_offset).max(0.0);
        let line_idx = ((local_y / lh) as usize).min(self.buffer.line_count().saturating_sub(1));

        let gutter_offset = self.gutter_width() + TEXT_PADDING;
        let text_x = (position.x.as_f32() - bounds.left().as_f32() - gutter_offset).max(0.0);

        let col = line_idx
            .checked_sub(self.last_first_line)
            .and_then(|li| self.last_line_layouts.get(li))
            .map_or(0, |layout| layout.closest_index_for_x(px(text_x)));

        // `closest_index_for_x` returns a UTF-8 byte index; convert with the
        // byte-column helper so multibyte lines hit-test correctly.
        offset_for_line_byte_col(&self.buffer, line_idx, col)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.buffer.content().chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let offset = offset.min(self.buffer.len());
        self.buffer.content()[..offset]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl EditorView {
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let (line, col) = self.line_col_for_offset(self.cursor_offset());
        (line + 1, col + 1)
    }

    pub fn text(&self) -> &str {
        self.buffer.content()
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer = TextBuffer::from_text(text);
        self.cursor = 0;
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.scroll_offset = 0.0;
        self.goal_x = None;
        // Revision resets to 0, which could collide with the cached single-line
        // revision key: drop per-content caches so stale shapes never reuse.
        self.single_line_cached = None;
        self.shaped_line_cache.clear();
        self.gutter_cache.clear();
        self.last_line_layouts.clear();
        cx.notify();
    }

    pub fn set_input_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.set_text(text, cx);
        self.cursor = self.buffer.len();
        self.selected_range = self.cursor..self.cursor;
        cx.notify();
    }

    pub fn insert_snippet(&mut self, snippet: &str, cx: &mut Context<Self>) {
        let start = self.cursor;
        self.buffer.begin_transaction(start);
        if snippet.contains('}') && self.buffer.content()[start..].starts_with('}') {
            self.buffer.delete(start..start + 1);
        }
        self.buffer.insert(start, snippet);
        let cursor_in_snippet = snippet
            .find("\n    \n")
            .map(|index| index + 5)
            .or_else(|| snippet.find("{}").map(|index| index + 1))
            .unwrap_or(snippet.len());
        self.cursor = start + cursor_in_snippet;
        self.selected_range = self.cursor..self.cursor;
        self.buffer.end_transaction(self.cursor);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub fn dismiss_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu_position.take().is_some() {
            cx.notify();
        }
    }

    pub fn set_completion_active(&mut self, active: bool) {
        self.completion_active = active;
    }

    pub fn selected_text(&self) -> Option<String> {
        (!self.selected_range.is_empty())
            .then(|| self.buffer.content()[self.selected_range.clone()].to_string())
    }

    pub fn reference_at_cursor(&self) -> Option<String> {
        if !self.selected_range.is_empty() {
            let selected = self.buffer.content()[self.selected_range.clone()].trim();
            return (!selected.is_empty()).then(|| selected.to_string());
        }

        let content = self.buffer.content();
        let allowed = |character: char| {
            character.is_alphanumeric() || matches!(character, '_' | ':' | '-' | '.')
        };
        let mut start = self.cursor.min(content.len());
        while start > 0 {
            let Some((previous, character)) = content[..start].char_indices().next_back() else {
                break;
            };
            if !allowed(character) {
                break;
            }
            start = previous;
        }
        let mut end = self.cursor.min(content.len());
        for character in content[end..].chars() {
            if !allowed(character) {
                break;
            }
            end += character.len_utf8();
        }
        (start < end).then(|| content[start..end].to_string())
    }

    pub fn completion_anchor(&self) -> (f32, f32) {
        let (line, byte_col) = line_byte_col(&self.buffer, self.cursor);
        let visible_line = line.saturating_sub(self.last_first_line);
        let x = self
            .last_line_layouts
            .get(visible_line)
            .map_or(0.0, |layout| layout.x_for_index(byte_col).as_f32());
        let y = line as f32 * self.last_line_height - self.scroll_offset + self.last_line_height;
        let desired_x = self.gutter_width() + TEXT_PADDING + x;
        let max_x = self.last_bounds.map_or(desired_x, |bounds| {
            (bounds.size.width.as_f32() - 320.0).max(0.0)
        });
        (desired_x.min(max_x), y.max(0.0))
    }

    pub fn revision(&self) -> u64 {
        self.buffer.revision()
    }
}

fn word_range_at(content: &str, offset: usize) -> Range<usize> {
    if content.is_empty() {
        return 0..0;
    }

    let mut position = offset.min(content.len());
    while position > 0 && !content.is_char_boundary(position) {
        position -= 1;
    }
    if position == content.len() {
        position = content
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }

    let is_word = |character: char| character.is_alphanumeric() || character == '_';
    let selected_is_word = content[position..].chars().next().is_some_and(is_word);

    let mut start = position;
    while start > 0 {
        let Some((previous, character)) = content[..start].char_indices().next_back() else {
            break;
        };
        if is_word(character) != selected_is_word {
            break;
        }
        start = previous;
    }

    let mut end = position;
    for (relative, character) in content[position..].char_indices() {
        if is_word(character) != selected_is_word {
            break;
        }
        end = position + relative + character.len_utf8();
    }

    start..end
}

const TEXT_PADDING: f32 = 14.0;

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Single-line text comes from the revision-keyed `SharedString` cache:
        // no full-doc `to_string` when nothing changed, and no alloc at all
        // for the empty fast path.
        let single_line_content: Option<SharedString> =
            self.single_line.then(|| self.single_line_shared());
        let single_line_focused = self.single_line && self.focus_handle.is_focused(window);
        let mut root = div()
            .id("editor-view")
            .key_context("Editor")
            .role(Role::TextInput)
            .aria_label(if self.single_line {
                "Search"
            } else {
                "Document editor"
            })
            .track_focus(&self.focus_handle)
            .font_family("Menlo")
            .text_size(px(self.font_size))
            .line_height(px(23.0))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_select_left))
            .on_action(cx.listener(Self::on_select_right))
            .on_action(cx.listener(Self::on_select_up))
            .on_action(cx.listener(Self::on_select_down))
            .on_action(cx.listener(Self::on_move_word_left))
            .on_action(cx.listener(Self::on_move_word_right))
            .on_action(cx.listener(Self::on_select_word_left))
            .on_action(cx.listener(Self::on_select_word_right))
            .on_action(cx.listener(Self::on_delete_word_left))
            .on_action(cx.listener(Self::on_delete_word_right))
            .on_action(cx.listener(Self::on_delete_line))
            .on_action(cx.listener(Self::on_toggle_comment))
            .on_action(cx.listener(Self::on_toggle_bold))
            .on_action(cx.listener(Self::on_toggle_italic))
            .on_action(cx.listener(Self::on_toggle_math))
            .on_action(cx.listener(Self::on_select_all))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_move_to_beginning))
            .on_action(cx.listener(Self::on_move_to_end))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_enter))
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_context_menu))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .cursor(CursorStyle::IBeam)
            .relative()
            .flex()
            .flex_1()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .bg(theme::color(theme::BG))
            .text_color(theme::color(theme::TEXT));

        if self.single_line {
            root = root.child(SingleLineInputElement {
                editor: cx.entity(),
            });
            let overlay_text: SharedString = single_line_content.unwrap_or_else(|| "".into());
            root = root.child(
                div()
                    .absolute()
                    .left(px(10.0))
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT))
                    .child(overlay_text)
                    .when(single_line_focused, |line| {
                        line.child(
                            div()
                                .w(px(1.0))
                                .h(px(15.0))
                                .ml(px(1.0))
                                .bg(theme::color(theme::TEXT)),
                        )
                    }),
            );
        } else {
            root = root.child(EditorElement {
                editor: cx.entity(),
            });
        }

        if let Some((x, y)) = self.context_menu_position {
            let menu_row = || {
                div()
                    .w_full()
                    .px_3()
                    .py_1p5()
                    .text_xs()
                    .text_color(theme::color(theme::TEXT))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::color(theme::HOVER_BG)))
            };
            let separator = || div().h(px(1.0)).my_1().bg(theme::color(theme::BORDER));

            root = root.child(
                div()
                    .id("editor-context-menu")
                    .role(Role::Menu)
                    .aria_label("Editor actions")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(180.0))
                    .py_1()
                    .rounded_xs()
                    .border_1()
                    .border_color(theme::color(theme::BORDER))
                    .bg(theme::color(theme::BG_SURFACE))
                    .shadow_lg()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, _, cx| cx.stop_propagation()),
                    )
                    .child(
                        menu_row()
                            .id("context-undo")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    this.on_undo(&Undo, window, cx);
                                }),
                            )
                            .child("Undo"),
                    )
                    .child(
                        menu_row()
                            .id("context-redo")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    this.on_redo(&Redo, window, cx);
                                }),
                            )
                            .child("Redo"),
                    )
                    .child(separator())
                    .child(
                        menu_row()
                            .id("context-cut")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    this.on_cut(&Cut, window, cx);
                                }),
                            )
                            .child("Cut"),
                    )
                    .child(
                        menu_row()
                            .id("context-copy")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    this.on_copy(&Copy, window, cx);
                                    cx.notify();
                                }),
                            )
                            .child("Copy"),
                    )
                    .child(
                        menu_row()
                            .id("context-paste")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    this.on_paste(&Paste, window, cx);
                                }),
                            )
                            .child("Paste"),
                    )
                    .child(
                        menu_row()
                            .id("context-select-all")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    this.on_select_all(&SelectAll, window, cx);
                                }),
                            )
                            .child("Select All"),
                    )
                    .child(separator())
                    .child(
                        menu_row()
                            .id("context-find")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.context_menu_position = None;
                                    window.dispatch_action(
                                        Box::new(crate::workspace::ToggleFind),
                                        cx,
                                    );
                                }),
                            )
                            .child("Find"),
                    )
                    .child(
                        menu_row()
                            .id("context-find-references")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.context_menu_position = None;
                                    cx.emit(EditorEvent::FindReferences);
                                    cx.notify();
                                }),
                            )
                            .child("Find All References"),
                    ),
            );
        }

        root
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_diagnostic_severity_map, gutter_color_for, hash_str, highlight_mode_for,
        line_byte_col, offset_for_line_byte_col, snap_byte_col, word_range_at,
    };
    use crate::compiler::diagnostics::{Diagnostic, DiagnosticSource, Severity};
    use crate::editor::buffer::TextBuffer;
    use crate::ui::theme;

    #[test]
    fn word_selection_handles_words_punctuation_and_unicode() {
        let text = "alpha beta, café";

        assert_eq!(&text[word_range_at(text, 2)], "alpha");
        assert_eq!(&text[word_range_at(text, 7)], "beta");
        assert_eq!(&text[word_range_at(text, 10)], ", ");
        assert_eq!(&text[word_range_at(text, text.len())], "café");
    }

    #[test]
    fn byte_columns_stay_consistent_for_multibyte_lines() {
        let buffer = TextBuffer::from_text("café\nこんにちは\nplain");
        // "café": c(1) a(1) f(1) é(2) = 5 bytes, 4 chars.
        let line0 = buffer.line_content(0).unwrap();
        assert_eq!(line0.len(), 5);
        let start0 = buffer.line_start_offset(0);
        let end_of_cafe = start0 + line0.len();
        let (line, byte_col) = line_byte_col(&buffer, end_of_cafe);
        assert_eq!((line, byte_col), (0, 5));
        assert_eq!(offset_for_line_byte_col(&buffer, 0, 5), end_of_cafe);
        // Mid-character byte column snaps down to the char boundary.
        assert_eq!(snap_byte_col(line0, 4), 3);
        assert_eq!(offset_for_line_byte_col(&buffer, 0, 4), start0 + 3);
        // CJK line: 5 chars x 3 bytes.
        let line1 = buffer.line_content(1).unwrap();
        assert_eq!(line1.chars().count(), 5);
        assert_eq!(line1.len(), 15);
        let start1 = buffer.line_start_offset(1);
        let (line, byte_col) = line_byte_col(&buffer, start1 + 6);
        assert_eq!((line, byte_col), (1, 6));
        assert_eq!(offset_for_line_byte_col(&buffer, 1, 7), start1 + 6);
        // Clamping beyond end of line.
        assert_eq!(
            offset_for_line_byte_col(&buffer, 0, 10_000),
            start0 + line0.len()
        );
    }

    #[test]
    fn selection_byte_spans_avoid_char_byte_mixing() {
        // Regression for `line_len` bytes mixed with char columns: the end of
        // "éa" is byte 3 but char 2; byte helpers must agree.
        let buffer = TextBuffer::from_text("éa\nxy");
        let line0 = buffer.line_content(0).unwrap();
        assert_eq!(line0.len(), 3);
        let sel_end = buffer.line_start_offset(0) + line0.len();
        let (_, end_byte) = line_byte_col(&buffer, sel_end);
        assert_eq!(end_byte, line0.len());
        assert_eq!(snap_byte_col(line0, end_byte), line0.len());
    }

    #[test]
    fn diagnostic_map_keeps_worst_severity_per_line() {
        let diags = vec![
            Diagnostic::new(
                1,
                Severity::Warning,
                DiagnosticSource::Parser,
                None,
                Some(3),
                "w",
            ),
            Diagnostic::new(
                2,
                Severity::Error,
                DiagnosticSource::Parser,
                None,
                Some(3),
                "e",
            ),
            Diagnostic::new(
                3,
                Severity::Warning,
                DiagnosticSource::Parser,
                None,
                Some(5),
                "w2",
            ),
            Diagnostic::new(
                4,
                Severity::Error,
                DiagnosticSource::Parser,
                None,
                None,
                "no line",
            ),
        ];
        let map = build_diagnostic_severity_map(&diags);
        assert_eq!(map.get(&2), Some(&Severity::Error));
        assert_eq!(map.get(&4), Some(&Severity::Warning));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn gutter_colors_prioritize_diagnostics_then_active_line() {
        assert_eq!(
            gutter_color_for(0, 0, true, Some(Severity::Error)),
            theme::ACCENT_RED
        );
        assert_eq!(
            gutter_color_for(0, 1, true, Some(Severity::Warning)),
            theme::ACCENT_ORANGE
        );
        assert_eq!(gutter_color_for(2, 2, true, None), theme::TEXT);
        assert_eq!(gutter_color_for(2, 3, true, None), theme::TEXT_MUTED);
        assert_eq!(gutter_color_for(2, 2, false, None), theme::TEXT_MUTED);
    }

    #[test]
    fn highlight_modes_and_hashes_are_stable() {
        assert_eq!(highlight_mode_for(true, false), 0);
        assert_eq!(highlight_mode_for(false, false), 1);
        assert_eq!(highlight_mode_for(false, true), 2);
        assert_eq!(hash_str("hello"), hash_str("hello"));
        assert_ne!(hash_str("hello"), hash_str("world"));
    }
}
