//! Movement boundaries and UTF-16 mapping for the editor text surface.

use std::ops::Range;

use super::EditorView;

use crate::editor::buffer::clamp_str_boundary;
use gpui::{Context, Pixels, Point, px};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn word_range_at(content: &str, offset: usize) -> Range<usize> {
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

pub(crate) const TEXT_PADDING: f32 = 14.0;

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
impl EditorView {
    pub(super) fn previous_boundary(&self, offset: usize) -> usize {
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

    pub(super) fn next_boundary(&self, offset: usize) -> usize {
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

    pub(super) fn previous_word_boundary(&self, offset: usize) -> usize {
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

    pub(super) fn next_word_boundary(&self, offset: usize) -> usize {
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

    pub(super) fn line_col_for_offset(&self, offset: usize) -> (usize, usize) {
        let line = self.buffer.line_of_offset(offset);
        let line_start = self.buffer.line_start_offset(line);
        let line_str = self.buffer.line_content(line).unwrap_or("");
        let safe_rel = clamp_str_boundary(line_str, offset.saturating_sub(line_start));
        let char_col = line_str[..safe_rel].chars().count();
        (line, char_col)
    }

    pub(super) fn offset_for_line_col(&self, line: usize, char_col: usize) -> usize {
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

    pub(super) fn move_vertically(
        &mut self,
        delta: isize,
        extend_selection: bool,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn visible_lines(&self) -> usize {
        if self.last_line_height <= 0.0 {
            return 30;
        }
        let height = self.last_bounds.map_or(600.0, |b| b.size.height.as_f32());
        (height / self.last_line_height).ceil() as usize
    }

    pub(super) fn ensure_cursor_visible(&mut self) {
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

    pub(super) fn offset_for_position(&self, position: Point<Pixels>) -> usize {
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
    pub(super) fn utf16_line_offsets(&self) -> std::cell::RefMut<'_, Vec<usize>> {
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

    pub(super) fn offset_from_utf16(&self, offset: usize) -> usize {
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

    pub(super) fn offset_to_utf16(&self, offset: usize) -> usize {
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

    pub(super) fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    pub(super) fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}
