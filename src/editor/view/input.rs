use super::*;

impl EditorView {
    pub(super) fn on_backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor > 0 {
            let prev = self.previous_boundary(self.cursor);
            self.delete_range(prev..self.cursor);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor < self.buffer.len() {
            let next = self.next_boundary(self.cursor);
            self.delete_range(self.cursor..next);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.move_to(self.selected_range.start, cx);
        } else if self.cursor > 0 {
            let prev = self.previous_boundary(self.cursor);
            self.move_to(prev, cx);
        }
    }

    pub(super) fn on_right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            self.move_to(self.selected_range.end, cx);
        } else if self.cursor < self.buffer.len() {
            let next = self.next_boundary(self.cursor);
            self.move_to(next, cx);
        }
    }

    pub(super) fn on_up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_active {
            cx.emit(EditorEvent::PreviousCompletion);
        } else {
            self.move_vertically(-1, false, cx);
        }
    }

    pub(super) fn on_down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_active {
            cx.emit(EditorEvent::NextCompletion);
        } else {
            self.move_vertically(1, false, cx);
        }
    }

    pub(super) fn on_select_left(
        &mut self,
        _: &SelectLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cursor > 0 {
            let prev = self.previous_boundary(self.cursor);
            self.select_to(prev, cx);
        }
    }

    pub(super) fn on_select_right(
        &mut self,
        _: &SelectRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.cursor < self.buffer.len() {
            let next = self.next_boundary(self.cursor);
            self.select_to(next, cx);
        }
    }

    pub(super) fn on_move_word_left(
        &mut self,
        _: &MoveWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prev = self.previous_word_boundary(self.cursor);
        self.move_to(prev, cx);
    }

    pub(super) fn on_move_word_right(
        &mut self,
        _: &MoveWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.next_word_boundary(self.cursor);
        self.move_to(next, cx);
    }

    pub(super) fn on_select_word_left(
        &mut self,
        _: &SelectWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prev = self.previous_word_boundary(self.cursor);
        self.select_to(prev, cx);
    }

    pub(super) fn on_select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = self.next_word_boundary(self.cursor);
        self.select_to(next, cx);
    }

    pub(super) fn on_delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selected_range.is_empty() {
            self.delete_range(self.selected_range.clone());
        } else if self.cursor > 0 {
            let prev = self.previous_word_boundary(self.cursor);
            self.delete_range(prev..self.cursor);
        }
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_delete_word_right(
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

    pub(super) fn on_delete_line(
        &mut self,
        _: &DeleteLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (cursor_line, _) = self.line_col_for_offset(self.cursor);
        let line_start = self.buffer.line_start_offset(cursor_line);
        self.delete_range(line_start..self.cursor);
        self.ensure_cursor_visible();
        cx.notify();
    }

    pub(super) fn on_toggle_comment(
        &mut self,
        _: &ToggleComment,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn on_toggle_bold(
        &mut self,
        _: &ToggleBold,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wrap_or_insert_format(
            if self.is_typst { "*" } else { "\\textbf{" },
            if self.is_typst { "*" } else { "}" },
            cx,
        );
    }

    pub(super) fn on_toggle_italic(
        &mut self,
        _: &ToggleItalic,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wrap_or_insert_format(
            if self.is_typst { "_" } else { "\\textit{" },
            if self.is_typst { "_" } else { "}" },
            cx,
        );
    }

    pub(super) fn on_toggle_math(
        &mut self,
        _: &ToggleMath,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn on_select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertically(-1, true, cx);
    }

    pub(super) fn on_select_down(
        &mut self,
        _: &SelectDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_vertically(1, true, cx);
    }

    pub(super) fn on_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.buffer.len();
        self.cursor = self.buffer.len();
        self.selection_reversed = false;
        self.goal_x = None;
        cx.notify();
    }

    pub(super) fn on_home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let (line, _) = self.line_col_for_offset(self.cursor);
        let start = self.buffer.line_start_offset(line);
        self.move_to(start, cx);
    }

    pub(super) fn on_end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let (line, _) = self.line_col_for_offset(self.cursor);
        let line_len = self.buffer.line_content(line).map_or(0, str::len);
        let start = self.buffer.line_start_offset(line);
        self.move_to(start + line_len, cx);
    }

    pub(super) fn on_move_to_beginning(
        &mut self,
        _: &MoveToBeginning,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_to(0, cx);
        self.scroll_offset = 0.0;
        self.ensure_cursor_visible();
    }

    pub(super) fn on_move_to_end(&mut self, _: &MoveToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.buffer.len(), cx);
        self.ensure_cursor_visible();
    }

    pub(super) fn on_page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        let lines = self.visible_lines().saturating_sub(2).max(1);
        self.move_vertically(-(lines as isize), false, cx);
        self.ensure_cursor_visible();
    }

    pub(super) fn on_page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        let lines = self.visible_lines().saturating_sub(2).max(1);
        self.move_vertically(lines as isize, false, cx);
        self.ensure_cursor_visible();
    }

    pub(super) fn on_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.completion_active {
            cx.emit(EditorEvent::AcceptCompletion);
        } else {
            self.insert_text("\n");
            self.ensure_cursor_visible();
            cx.notify();
            if self.single_line {
                window.dispatch_action(Box::new(crate::workspace::FocusEditor), cx);
            }
        }
    }

    pub(super) fn on_tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion_active {
            cx.emit(EditorEvent::AcceptCompletion);
        } else {
            self.insert_text(&" ".repeat(self.tab_size));
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    pub(super) fn on_paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert_text(&text);
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    pub(super) fn on_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.content()[self.selected_range.clone()].to_string(),
            ));
        }
    }

    pub(super) fn on_cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buffer.content()[self.selected_range.clone()].to_string(),
            ));
            self.delete_range(self.selected_range.clone());
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    pub(super) fn on_undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(cursor_pos) = self.buffer.undo() {
            self.cursor = cursor_pos;
            self.selected_range = self.cursor..self.cursor;
            self.goal_x = None;
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    pub(super) fn on_redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(cursor_pos) = self.buffer.redo() {
            self.cursor = cursor_pos;
            self.selected_range = self.cursor..self.cursor;
            self.goal_x = None;
            self.ensure_cursor_visible();
            cx.notify();
        }
    }

    pub(super) fn on_show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu_position = None;
        window.focus(&self.focus_handle, cx);
        if self.single_line {
            if event.click_count >= 2 {
                self.selected_range = 0..self.buffer.len();
                self.cursor = self.buffer.len();
                self.selection_reversed = false;
                cx.notify();
            } else {
                self.move_to(self.buffer.len(), cx);
            }
            return;
        }
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

    pub(super) fn on_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let offset = self.offset_for_position(event.position);
        if !self.selected_range.contains(&offset) {
            self.move_to(offset, cx);
        }

        let Some(bounds) = self.last_bounds else {
            return;
        };
        let x = (event.position.x.as_f32() - bounds.left().as_f32())
            .clamp(4.0, (bounds.size.width.as_f32() - 184.0).max(4.0));
        let y = (event.position.y.as_f32() - bounds.top().as_f32())
            .clamp(4.0, (bounds.size.height.as_f32() - 246.0).max(4.0));
        self.context_menu_position = Some((x, y));
        cx.notify();
    }

    fn select_word_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let range = word_range_at(self.buffer.content(), offset);
        self.selected_range = range.clone();
        self.cursor = range.end;
        self.selection_reversed = false;
        self.goal_x = None;
        cx.notify();
    }

    pub(super) fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    pub(super) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let offset = self.offset_for_position(event.position);
            self.select_to(offset, cx);
        }
    }

    pub(super) fn on_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
