//! Editor view — the GPUI component for multiline text editing.
//!
//! Implements high-performance text editing with syntax highlighting,
//! word navigation, auto-closing pairs, comment toggling, and undo/redo.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent,
    ShapedLine, Style, TextRun, UTF16Selection, Window, actions, div, fill, point, prelude::*, px,
    relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

use super::buffer::TextBuffer;
use crate::ui::theme;

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

/// Register the editor key bindings matching Zed / macOS defaults.
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

/// The multiline text editor view.
pub struct EditorView {
    focus_handle: FocusHandle,
    buffer: TextBuffer,

    /// Byte offset of the cursor within the buffer.
    cursor: usize,
    /// Selection as a byte range. When empty, start == end == cursor.
    selected_range: Range<usize>,
    /// Whether selection was extended backwards.
    selection_reversed: bool,

    /// IME marked (composing) text range.
    marked_range: Option<Range<usize>>,

    /// Vertical scroll offset in pixels.
    scroll_offset: f32,

    /// Is the mouse currently selecting?
    is_selecting: bool,

    /// Cached shaped lines for the visible range.
    last_line_layouts: Vec<ShapedLine>,
    /// The first visible line index in last render.
    last_first_line: usize,
    /// Bounds of the text area from last render.
    last_bounds: Option<Bounds<Pixels>>,
    /// Line height from last render.
    last_line_height: f32,
    /// Target column (in pixels) for vertical movement to preserve horizontal position.
    goal_x: Option<f32>,
    /// Compiler diagnostics for inline highlighting.
    diagnostics: Vec<crate::compiler::diagnostics::Diagnostic>,
    /// Whether the active document uses Typst syntax.
    pub is_typst: bool,
    font_size: f32,
    tab_size: usize,
    line_numbers: bool,
}

impl EditorView {
    /// Sets whether Typst syntax highlighting is enabled.
    pub fn set_is_typst(&mut self, is_typst: bool, cx: &mut Context<Self>) {
        if self.is_typst != is_typst {
            self.is_typst = is_typst;
            cx.notify();
        }
    }

    /// Create a new editor with initial text content.
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
            is_typst: false,
            font_size: 14.0,
            tab_size: 2,
            line_numbers: true,
        }
    }

    pub fn set_preferences(
        &mut self,
        font_size: f32,
        tab_size: usize,
        line_numbers: bool,
        cx: &mut Context<Self>,
    ) {
        self.font_size = font_size.clamp(10.0, 24.0);
        self.tab_size = tab_size.clamp(1, 8);
        self.line_numbers = line_numbers;
        cx.notify();
    }

    /// Updates active diagnostics for inline gutter error markers.
    pub fn set_diagnostics(
        &mut self,
        diags: Vec<crate::compiler::diagnostics::Diagnostic>,
        cx: &mut Context<Self>,
    ) {
        self.diagnostics = diags;
        cx.notify();
    }

    /// Computes dynamic gutter width in pixels to fit all line digits cleanly.
    pub fn gutter_width(&self) -> f32 {
        if !self.line_numbers {
            return 0.0;
        }
        let line_count = self.buffer.line_count();
        let digits = line_count.to_string().len().max(2);
        (digits as f32 * 9.0 + 26.0).max(48.0)
    }

    /// Jumps the cursor to the specified 1-based line number and scrolls it into view.
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

    /// Returns the active cursor byte offset.
    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn insert_text(&mut self, text: &str) {
        self.buffer.begin_transaction(self.cursor);

        if !self.selected_range.is_empty() {
            self.buffer.delete(self.selected_range.clone());
            self.cursor = self.selected_range.start;
            self.selected_range = self.cursor..self.cursor;
        }

        // Auto-close pairs for common LaTeX delimiters when typed
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

        self.offset_for_line_col(line_idx, col)
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
    fn on_backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor > 0 {
            let prev = self.previous_boundary(self.cursor);
            self.delete_range(prev..self.cursor);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor < self.buffer.len() {
            let next = self.next_boundary(self.cursor);
            self.delete_range(self.cursor..next);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.move_to(self.selected_range.start, cx);
        } else if self.cursor > 0 {
            let prev = self.previous_boundary(self.cursor);
            self.move_to(prev, cx);
        }
    }

    fn on_right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.move_to(self.selected_range.end, cx);
        } else if self.cursor < self.buffer.len() {
            let next = self.next_boundary(self.cursor);
            self.move_to(next, cx);
        }
    }

    fn on_up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, false, cx);
    }

    fn on_down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(1, false, cx);
    }

    fn on_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.cursor > 0 {
            let prev = self.previous_boundary(self.cursor);
            self.select_to(prev, cx);
        }
    }

    fn on_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.cursor < self.buffer.len() {
            let next = self.next_boundary(self.cursor);
            self.select_to(next, cx);
        }
    }

    fn on_move_word_left(&mut self, _: &MoveWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let prev = self.previous_word_boundary(self.cursor);
        self.move_to(prev, cx);
    }

    fn on_move_word_right(&mut self, _: &MoveWordRight, _: &mut Window, cx: &mut Context<Self>) {
        let next = self.next_word_boundary(self.cursor);
        self.move_to(next, cx);
    }

    fn on_select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let prev = self.previous_word_boundary(self.cursor);
        self.select_to(prev, cx);
    }

    fn on_select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.next_word_boundary(self.cursor);
        self.select_to(next, cx);
    }

    fn on_delete_word_left(&mut self, _: &DeleteWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor > 0 {
            let prev = self.previous_word_boundary(self.cursor);
            self.delete_range(prev..self.cursor);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor < self.buffer.len() {
            let next = self.next_word_boundary(self.cursor);
            self.delete_range(self.cursor..next);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_delete_line(&mut self, _: &DeleteLine, _: &mut Window, cx: &mut Context<Self>) {
        let (cursor_line, _) = self.line_col_for_offset(self.cursor);
        let line_start = self.buffer.line_start_offset(cursor_line);
        self.delete_range(line_start..self.cursor);
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_toggle_comment(&mut self, _: &ToggleComment, _: &mut Window, cx: &mut Context<Self>) {
        let (cursor_line, _) = self.line_col_for_offset(self.cursor);
        let line_start = self.buffer.line_start_offset(cursor_line);
        if let Some(content) = self.buffer.line_content(cursor_line) {
            let (prefix, trimmed_prefix) = if self.is_typst {
                ("// ", "//")
            } else {
                ("% ", "%")
            };

            if content.trim_start().starts_with(trimmed_prefix) {
                if let Some(pos) = content.find(trimmed_prefix) {
                    let delete_len = if content[pos..].starts_with(prefix) {
                        prefix.len()
                    } else {
                        trimmed_prefix.len()
                    };
                    self.buffer
                        .delete(line_start + pos..line_start + pos + delete_len);
                }
            } else {
                self.buffer.insert(line_start, prefix);
            }
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    fn on_toggle_bold(&mut self, _: &ToggleBold, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap_or_insert_format(
            if self.is_typst { "*" } else { "\\textbf{" },
            if self.is_typst { "*" } else { "}" },
            cx,
        );
    }

    fn on_toggle_italic(&mut self, _: &ToggleItalic, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap_or_insert_format(
            if self.is_typst { "_" } else { "\\textit{" },
            if self.is_typst { "_" } else { "}" },
            cx,
        );
    }

    fn on_toggle_math(&mut self, _: &ToggleMath, _: &mut Window, cx: &mut Context<Self>) {
        self.wrap_or_insert_format("$", "$", cx);
    }

    fn wrap_or_insert_format(&mut self, prefix: &str, suffix: &str, cx: &mut Context<Self>) {
        self.buffer.begin_transaction(self.cursor);
        if !self.selected_range.is_empty() {
            let start = self.selected_range.start;
            let end = self.selected_range.end;
            let selected_text = self.buffer.content()[start..end].to_string();
            let wrapped = format!("{prefix}{selected_text}{suffix}");
            self.buffer.delete(start..end);
            self.buffer.insert(start, &wrapped);
            self.cursor = start + wrapped.len();
            self.selected_range = self.cursor..self.cursor;
        } else {
            let pos = self.cursor;
            self.buffer.insert(pos, prefix);
            self.buffer.insert(pos + prefix.len(), suffix);
            self.cursor = pos + prefix.len();
            self.selected_range = self.cursor..self.cursor;
        }
        self.buffer.end_transaction(self.cursor);
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, true, cx);
    }

    fn on_select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(1, true, cx);
    }

    fn on_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.buffer.len();
        self.cursor = self.buffer.len();
        self.selection_reversed = false;
        self.goal_x = None;
        cx.notify();
    }

    fn on_home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let (line, _) = self.line_col_for_offset(self.cursor);
        let start = self.buffer.line_start_offset(line);
        self.move_to(start, cx);
    }

    fn on_end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let (line, _) = self.line_col_for_offset(self.cursor);
        let line_len = self.buffer.line_content(line).map_or(0, str::len);
        let start = self.buffer.line_start_offset(line);
        self.move_to(start + line_len, cx);
    }

    fn on_move_to_beginning(
        &mut self,
        _: &MoveToBeginning,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to(0, cx);
        self.scroll_offset = 0.0;
        self.ensure_cursor_visible();
    }

    fn on_move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.buffer.len(), cx);
        self.ensure_cursor_visible();
    }

    fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        let lines = self.visible_lines().saturating_sub(2).max(1);
        self.move_vertically(-(lines as isize), false, cx);
        self.ensure_cursor_visible();
    }

    fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        let lines = self.visible_lines().saturating_sub(2).max(1);
        self.move_vertically(lines as isize, false, cx);
        self.ensure_cursor_visible();
    }

    fn on_enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_text("\n");
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_text(&" ".repeat(self.tab_size));
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert_text(&text);
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.content()[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.content()[self.selected_range.clone()].to_string(),
            ));
            self.delete_range(self.selected_range.clone());
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(cursor_pos) = self.buffer.undo() {
            self.cursor = cursor_pos;
            self.selected_range = self.cursor..self.cursor;
            self.goal_x = None;
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(cursor_pos) = self.buffer.redo() {
            self.cursor = cursor_pos;
            self.selected_range = self.cursor..self.cursor;
            self.goal_x = None;
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    fn on_show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let offset = self.offset_for_position(event.position);

        match event.click_count {
            count if count >= 3 => {
                let (line, _) = self.line_col_for_offset(offset);
                let range = self.buffer.line_range(line).unwrap_or(offset..offset);
                self.selected_range = range.clone();
                self.cursor = range.end;
                self.selection_reversed = false;
                self.is_selecting = false;
                cx.notify();
            }
            2 => {
                self.select_word_at(offset, cx);
                self.is_selecting = false;
            }
            _ => {
                self.is_selecting = true;
                if event.modifiers.shift {
                    self.select_to(offset, cx);
                } else {
                    self.move_to(offset, cx);
                }
            }
        }
    }

    fn select_word_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let range = word_range_at(self.buffer.content(), offset);
        self.selected_range = range.clone();
        self.cursor = range.end;
        self.selection_reversed = false;
        self.goal_x = None;
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.offset_for_position(event.position);
            self.select_to(offset, cx);
        }
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta_y = event
            .delta
            .pixel_delta(px(self.last_line_height))
            .y
            .as_f32();
        self.scroll_offset -= delta_y;
        let max_scroll = (self.buffer.line_count() as f32 * self.last_line_height).max(0.0);
        self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);
        cx.notify();
    }

    /// Returns the current 1-based (line, column) cursor position.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let (line, col) = self.line_col_for_offset(self.cursor_offset());
        (line + 1, col + 1)
    }

    /// Returns the text content of the buffer.
    pub fn text(&self) -> &str {
        self.buffer.content()
    }

    /// Replaces the entire buffer text with new content, resetting cursor and scroll.
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.buffer = TextBuffer::from_text(text);
        self.cursor = 0;
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.scroll_offset = 0.0;
        self.goal_x = None;
        cx.notify();
    }

    /// Inserts completion snippet at the current cursor position.
    pub fn insert_snippet(&mut self, snippet: &str, cx: &mut Context<Self>) {
        self.buffer.begin_transaction(self.cursor);
        self.buffer.insert(self.cursor, snippet);
        self.cursor += snippet.len();
        self.selected_range = self.cursor..self.cursor;
        self.buffer.end_transaction(self.cursor);
        self.ensure_cursor_visible();
        cx.notify();
    }

    /// Returns the current revision of the buffer.
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

/// Left padding inside the text area.
const TEXT_PADDING: f32 = 14.0;

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("editor-view")
            .key_context("Editor")
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
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .cursor(CursorStyle::IBeam)
            .flex()
            .flex_1()
            .flex_col()
            .min_w_0()
            .bg(theme::color(theme::BG))
            .text_color(theme::color(theme::TEXT))
            .child(EditorElement {
                editor: cx.entity(),
            })
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        let start = range.start.min(self.buffer.len());
        let end = range.end.min(self.buffer.len());
        *actual_range = Some(self.range_to_utf16(&(start..end)));
        Some(self.buffer.content()[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        if !range.is_empty() && new_text.len() == 1 {
            let ch = new_text.chars().next().unwrap_or('\0');
            let closing = match ch {
                '(' => Some(')'),
                '[' => Some(']'),
                '{' => Some('}'),
                '"' => Some('"'),
                '\'' => Some('\''),
                '$' => Some('$'),
                '*' => Some('*'),
                '_' => Some('_'),
                '`' => Some('`'),
                _ => None,
            };

            if let Some(close_ch) = closing {
                let selected_text = self.buffer.content()[range.clone()].to_string();
                let wrapped = format!("{ch}{selected_text}{close_ch}");
                self.buffer.begin_transaction(self.cursor);
                self.buffer.delete(range.clone());
                self.buffer.insert(range.start, &wrapped);
                self.cursor = range.start + wrapped.len();
                self.selected_range = self.cursor..self.cursor;
                self.marked_range = None;
                self.buffer.end_transaction(self.cursor);
                self.goal_x = None;
                self.ensure_cursor_visible();
                cx.notify();
                return;
            }
        }

        self.buffer.begin_transaction(self.cursor);
        if !range.is_empty() {
            self.buffer.delete(range.clone());
        }
        if !new_text.is_empty() {
            self.buffer.insert(range.start, new_text);
        }
        self.cursor = range.start + new_text.len();
        self.selected_range = self.cursor..self.cursor;
        self.marked_range = None;
        self.buffer.end_transaction(self.cursor);
        self.goal_x = None;
        self.ensure_cursor_visible();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());

        self.buffer.begin_transaction(self.cursor);
        if !range.is_empty() {
            self.buffer.delete(range.clone());
        }
        if !new_text.is_empty() {
            self.buffer.insert(range.start, new_text);
        }

        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }

        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .map(|nr| nr.start + range.start..nr.end + range.start)
            .unwrap_or_else(|| {
                let pos = range.start + new_text.len();
                pos..pos
            });
        self.cursor = self.selected_range.end;
        self.buffer.end_transaction(self.cursor);
        self.goal_x = None;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (line, col) = self.line_col_for_offset(range.start);
        let local_line = line.checked_sub(self.last_first_line)?;
        let layout = self.last_line_layouts.get(local_line)?;

        let lh = self.last_line_height;
        let x_start = layout.x_for_index(col);
        let end_col = range
            .end
            .saturating_sub(self.buffer.line_start_offset(line));
        let line_len = self.buffer.line_content(line).map_or(0, str::len);
        let x_end = layout.x_for_index(end_col.min(line_len));

        let top = bounds.top() + px(line as f32 * lh - self.scroll_offset);
        let gutter_offset = px(self.gutter_width() + TEXT_PADDING);
        Some(Bounds::from_corners(
            point(bounds.left() + gutter_offset + x_start, top),
            point(bounds.left() + gutter_offset + x_end, top + px(lh)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.offset_for_position(point);
        Some(self.offset_to_utf16(offset))
    }
}

/// The custom GPUI element that handles layout, shaping, and painting of
/// the editor text content. Each frame it shapes only the visible lines.
struct EditorElement {
    editor: Entity<EditorView>,
}

struct EditorPrepaintState {
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
                let runs =
                    crate::editor::syntax::highlight_line(content, style.font(), editor.is_typst);
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

        // Paint current line highlight
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

#[cfg(test)]
mod tests {
    use super::word_range_at;

    #[test]
    fn word_selection_handles_words_punctuation_and_unicode() {
        let text = "alpha beta, café";

        assert_eq!(&text[word_range_at(text, 2)], "alpha");
        assert_eq!(&text[word_range_at(text, 7)], "beta");
        assert_eq!(&text[word_range_at(text, 10)], ", ");
        assert_eq!(&text[word_range_at(text, text.len())], "café");
    }
}
