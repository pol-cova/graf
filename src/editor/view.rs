mod element;
mod input;

use self::element::{EditorElement, SingleLineInputElement};
use std::ops::Range;

use crate::editor::buffer::{TextBuffer, clamp_str_boundary};

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, KeyBinding,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    Role, ScrollWheelEvent, ShapedLine, Style, TextRun, UTF16Selection, Window, actions, div, fill,
    point, prelude::*, px, relative, rgba, size,
};
use unicode_segmentation::UnicodeSegmentation;

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

    last_line_layouts: Vec<ShapedLine>,
    last_first_line: usize,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_height: f32,
    goal_col: Option<usize>,
    diagnostics: Vec<crate::compiler::diagnostics::Diagnostic>,
    pub is_typst: bool,
    plain_text: bool,
    font_size: f32,
    tab_size: usize,
    line_numbers: bool,
    completion_active: bool,
    context_menu_position: Option<(f32, f32)>,
    single_line: bool,
    /// Per-revision UTF16 offset of each line start, built lazily on the
    /// first IME conversion query. (revision, offsets per line + final total)
    /// Interior-mutable because the IME input handler reads through `&self`.
    utf16_cache: std::cell::RefCell<Option<(u64, Vec<usize>)>>,
}

impl EventEmitter<EditorEvent> for EditorView {}

impl EditorView {
    /// One setter for the language/styling configuration the renderer keys
    /// off, replacing the pair of boolean setters that could disagree.
    pub fn set_kind(
        &mut self,
        kind: crate::project::document::DocumentKind,
        cx: &mut Context<Self>,
    ) {
        let is_typst = kind == crate::project::document::DocumentKind::Typst;
        let plain_text = matches!(kind, crate::project::document::DocumentKind::PlainText);
        if self.is_typst != is_typst || self.plain_text != plain_text {
            self.is_typst = is_typst;
            self.plain_text = plain_text;
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
            goal_col: None,
            diagnostics: Vec::new(),
            is_typst: false,
            plain_text: false,
            font_size: 14.0,
            tab_size: 2,
            line_numbers: true,
            completion_active: false,
            context_menu_position: None,
            single_line: false,
            utf16_cache: std::cell::RefCell::new(None),
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
        self.font_size = font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.tab_size = tab_size.clamp(MIN_TAB_SIZE, MAX_TAB_SIZE);
        self.line_numbers = line_numbers;
        cx.notify();
    }

    pub fn set_diagnostics(
        &mut self,
        diags: Vec<crate::compiler::diagnostics::Diagnostic>,
        cx: &mut Context<Self>,
    ) {
        self.diagnostics = diags;
        cx.notify();
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
        self.goal_col = None;
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

        // One auto-pair model for keyboard and IME alike: a pairable char
        // either wraps the selection or inserts the closing partner.
        if text.chars().count() == 1
            && let Some((open, close)) = text.chars().next().and_then(auto_pair)
        {
            if !self.selected_range.is_empty() {
                let selected = self.buffer.content()[self.selected_range.clone()].to_string();
                let wrapped = format!("{open}{selected}{close}");
                self.buffer.delete(self.selected_range.clone());
                self.buffer.insert(self.selected_range.start, &wrapped);
                self.cursor = self.selected_range.start + wrapped.len();
                self.selected_range = self.cursor..self.cursor;
                self.buffer.end_transaction(self.cursor);
                self.goal_col = None;
                self.ensure_cursor_visible();
                return;
            }
            self.buffer.insert(self.cursor, &format!("{open}{close}"));
            self.cursor += open.len_utf8();
            self.selected_range = self.cursor..self.cursor;
            self.buffer.end_transaction(self.cursor);
            self.goal_col = None;
            self.ensure_cursor_visible();
            return;
        }

        if !self.selected_range.is_empty() {
            self.buffer.delete(self.selected_range.clone());
            self.cursor = self.selected_range.start;
            self.selected_range = self.cursor..self.cursor;
        }

        self.buffer.insert(self.cursor, text);
        self.cursor += text.len();
        self.selected_range = self.cursor..self.cursor;
        self.buffer.end_transaction(self.cursor);
        self.goal_col = None;
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
        self.goal_col = None;
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.cursor = offset.min(self.buffer.len());
        self.selected_range = self.cursor..self.cursor;
        self.selection_reversed = false;
        self.goal_col = None;
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
        let content = self.buffer.content();
        let safe_offset = offset.min(content.len());
        let window_start =
            clamp_str_boundary(content, safe_offset.saturating_sub(BOUNDARY_WINDOW_BYTES));
        match content[window_start..safe_offset]
            .grapheme_indices(true)
            .next_back()
        {
            // Interior cluster of the window (or the document start): a real
            // boundary. A single truncated cluster right after the cut falls
            // back to a whole-document scan.
            Some((idx, _)) if idx > 0 || window_start == 0 => window_start + idx,
            _ => content[..safe_offset]
                .grapheme_indices(true)
                .next_back()
                .map(|(idx, _)| idx)
                .unwrap_or(0),
        }
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let content = self.buffer.content();
        if offset >= content.len() {
            return content.len();
        }
        let safe_offset = offset.min(content.len());
        // Yields at most two clusters regardless of document length: the
        // first cluster after `safe_offset` plus the start of the next one.
        let mut iter = content[safe_offset..].grapheme_indices(true);
        let _first = iter.next();
        iter.next()
            .map(|(idx, _)| safe_offset + idx)
            .unwrap_or(content.len())
    }

    fn previous_word_boundary(&self, offset: usize) -> usize {
        let content = self.buffer.content();
        let safe_offset = offset.min(content.len());
        let window_start =
            clamp_str_boundary(content, safe_offset.saturating_sub(BOUNDARY_WINDOW_BYTES));
        let windowed = content[window_start..safe_offset]
            .split_word_bound_indices()
            .rev()
            .find_map(|(idx, word)| (!word.trim().is_empty()).then_some(idx));
        match windowed {
            // A word sitting at the window cut may be truncated by the left
            // edge; fall back to the whole document to stay exact.
            Some(idx) if idx > 0 || window_start == 0 => window_start + idx,
            _ => content[..safe_offset]
                .split_word_bound_indices()
                .rev()
                .find_map(|(idx, word)| {
                    (idx < safe_offset && !word.trim().is_empty()).then_some(idx)
                })
                .unwrap_or(0),
        }
    }

    fn next_word_boundary(&self, offset: usize) -> usize {
        let content = self.buffer.content();
        if offset >= content.len() {
            return content.len();
        }
        let safe_offset = offset.min(content.len());
        content[safe_offset..]
            .split_word_bound_indices()
            .take_while(|(rel_idx, _)| *rel_idx <= BOUNDARY_WINDOW_BYTES)
            .find_map(|(rel_idx, word)| {
                (rel_idx > 0 && !word.trim().is_empty()).then_some(safe_offset + rel_idx)
            })
            .unwrap_or(content.len())
    }

    fn line_col_for_offset(&self, offset: usize) -> (usize, usize) {
        let line = self.buffer.line_of_offset(offset);
        let line_start = self.buffer.line_start_offset(line);
        let line_str = self.buffer.line_content(line).unwrap_or("");
        let safe_rel = clamp_str_boundary(line_str, offset.saturating_sub(line_start));
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

        // The "goal column" keeps vertical walks on the character column the
        // user started from (dropped when any horizontal move happens).
        let target_col = self.goal_col.unwrap_or(col);

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
        self.goal_col = Some(target_col);
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

    /// UTF16 offsets for every line start plus the document total, memoized
    /// per buffer revision so repeated IME conversions never re-scan.
    fn utf16_line_offsets(&self) -> std::cell::RefMut<'_, Vec<usize>> {
        let revision = self.buffer.revision();
        let mut cache = self.utf16_cache.borrow_mut();
        match cache.as_mut() {
            Some((cached_rev, offsets)) if *cached_rev == revision => {}
            _ => {
                let offsets = compute_utf16_line_offsets(self.buffer.content());
                *cache = Some((revision, offsets));
            }
        }
        std::cell::RefMut::map(cache, |cache| &mut cache.as_mut().expect("just built").1)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let offsets = self.utf16_line_offsets();
        let line = offsets.partition_point(|&start_utf16| start_utf16 <= offset) - 1;
        let line_byte_start = self.buffer.line_start_offset(line);
        let mut target_within_line = offset - offsets[line];
        let mut byte_offset = line_byte_start;
        for character in self.buffer.content()[line_byte_start..].chars() {
            if target_within_line == 0 {
                break;
            }
            if character.len_utf16() > target_within_line {
                // Offset fell inside a surrogate pair; snap to its start.
                break;
            }
            target_within_line -= character.len_utf16();
            byte_offset += character.len_utf8();
        }
        byte_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let safe_offset = offset.min(self.buffer.len());
        let line = self.buffer.line_of_offset(safe_offset);
        let line_byte_start = self.buffer.line_start_offset(line);
        let within_line: usize = self.buffer.content()[line_byte_start..safe_offset]
            .chars()
            .map(char::len_utf16)
            .sum();
        let offsets = self.utf16_line_offsets();
        offsets
            .get(line)
            .copied()
            .unwrap_or_else(|| offsets.last().copied().unwrap_or(0))
            + within_line
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
        self.goal_col = None;
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
        let (line, column) = self.line_col_for_offset(self.cursor);
        let visible_line = line.saturating_sub(self.last_first_line);
        let x = self
            .last_line_layouts
            .get(visible_line)
            .map_or(0.0, |layout| layout.x_for_index(column).as_f32());
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

    let mut position = clamp_str_boundary(content, offset);
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

/// Typeface-independent cap on how far a movement scan looks around the
/// cursor. Arrow keys and word jumps scan this window instead of the whole
/// document; boundaries always exist well within it.
const BOUNDARY_WINDOW_BYTES: usize = 256;

/// UTF16 offset of every line start plus the document total, one pass.
fn compute_utf16_line_offsets(content: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut acc = 0usize;
    offsets.push(0);
    for character in content.chars() {
        acc += character.len_utf16();
        if character == '\n' {
            offsets.push(acc);
        }
    }
    offsets.shrink_to_fit();
    offsets
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let single_line_content = self.buffer.content().to_string();
        let single_line_focused = self.focus_handle.is_focused(window);
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
                    .child(single_line_content)
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

/// One auto-pair table+policy for typed and IME input: pairs `{ ( [ $ " ' * `_`
/// wrap a selection or insert the closing partner; everything else passes
/// through untouched.
pub(crate) fn auto_pair(ch: char) -> Option<(char, char)> {
    Some(match ch {
        '(' => ('(', ')'),
        '[' => ('[', ']'),
        '{' => ('{', '}'),
        '"' => ('"', '"'),
        '\'' => ('\'', '\''),
        '$' => ('$', '$'),
        '*' => ('*', '*'),
        '_' => ('_', '_'),
        '`' => ('`', '`'),
        _ => return None,
    })
}

#[cfg(test)]
mod auto_pair_tests {
    use super::*;

    #[test]
    fn pairs_in_one_table() {
        assert_eq!(auto_pair('('), Some(('(', ')')));
        assert_eq!(auto_pair('"'), Some(('"', '"')));
        assert_eq!(auto_pair('$'), Some(('$', '$')));
        assert_eq!(auto_pair('x'), None);
        assert_eq!(auto_pair('μ'), None);
    }

    #[test]
    fn boundary_clamp_walks_back_to_a_character_edge() {
        let buffer = TextBuffer::from_text("日本語");
        // Offsetting into the middle of the second character clamps back.
        assert_eq!(buffer.clamp_char_boundary(4), 3);
        assert_eq!(buffer.clamp_char_boundary(9), 9);
        assert_eq!(buffer.clamp_char_boundary(100), 9);
        assert_eq!(clamp_str_boundary("ab", 5), 2);
    }
}
