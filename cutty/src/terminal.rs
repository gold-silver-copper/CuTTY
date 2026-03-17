use std::collections::VecDeque;

use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

pub const DEFAULT_BG: Rgb = Rgb::new(0x12, 0x14, 0x1b);
pub const DEFAULT_FG: Rgb = Rgb::new(0xe6, 0xe9, 0xef);
pub const CURSOR_COLOR: Rgb = Rgb::new(0xff, 0xc8, 0x57);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerminalColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellAttributes {
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellColors {
    pub fg: Rgb,
    pub bg: Rgb,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalCell {
    contents: String,
    attrs: CellAttributes,
    wide: bool,
    wide_continuation: bool,
}

impl TerminalCell {
    pub fn contents(&self) -> &str {
        &self.contents
    }

    pub fn has_contents(&self) -> bool {
        !self.contents.is_empty()
    }

    pub fn is_wide_continuation(&self) -> bool {
        self.wide_continuation
    }

    pub fn fgcolor(&self) -> TerminalColor {
        self.attrs.fg
    }

    pub fn bgcolor(&self) -> TerminalColor {
        self.attrs.bg
    }

    pub fn bold(&self) -> bool {
        self.attrs.bold
    }

    pub fn inverse(&self) -> bool {
        self.attrs.inverse
    }

    fn new(contents: String, attrs: CellAttributes, wide: bool) -> Self {
        Self {
            contents,
            attrs,
            wide,
            wide_continuation: false,
        }
    }

    fn continuation(attrs: CellAttributes) -> Self {
        Self {
            contents: String::new(),
            attrs,
            wide: false,
            wide_continuation: true,
        }
    }

    fn is_blank(&self) -> bool {
        self.contents.is_empty()
            && self.attrs == CellAttributes::default()
            && !self.wide
            && !self.wide_continuation
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferRow {
    cells: Vec<TerminalCell>,
    wrapped: bool,
}

impl BufferRow {
    pub fn cell(&self, col: u16) -> Option<&TerminalCell> {
        self.cells.get(col as usize)
    }

    pub fn wrapped(&self) -> bool {
        self.wrapped
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.wrapped = false;
    }

    fn truncate_visible(&mut self, cols: u16) {
        self.cells.truncate(cols as usize);
        self.trim_trailing_blanks();
    }

    fn set_cell(&mut self, col: usize, cell: TerminalCell) {
        if self.cells.len() <= col {
            self.cells.resize(col + 1, TerminalCell::default());
        }
        self.cells[col] = cell;
    }

    fn clear_cell(&mut self, col: usize) {
        if col < self.cells.len() {
            self.cells[col] = TerminalCell::default();
        }
    }

    fn clear_overwrite(&mut self, col: usize) {
        if let Some(cell) = self.cells.get(col) {
            if cell.wide_continuation && col > 0 {
                self.clear_cell(col - 1);
            } else if cell.wide {
                self.clear_cell(col + 1);
            }
        }
        self.clear_cell(col);
    }

    fn append_to_previous(&mut self, col: usize, c: char) {
        if let Some(target) = col.checked_sub(1).and_then(|idx| self.cells.get_mut(idx)) {
            target.contents.push(c);
        }
    }

    fn clear_range(&mut self, start: usize, end: usize) {
        let end = end.min(self.cells.len());
        for idx in start.min(end)..end {
            self.cells[idx] = TerminalCell::default();
        }
        self.trim_trailing_blanks();
    }

    fn shift_right(&mut self, start: usize, count: usize, width: usize) {
        if start >= width || count == 0 {
            return;
        }

        if self.cells.len() < width {
            self.cells.resize(width, TerminalCell::default());
        }

        for idx in (start..width).rev() {
            if idx >= start + count {
                self.cells[idx] = self.cells[idx - count].clone();
            } else {
                self.cells[idx] = TerminalCell::default();
            }
        }
        self.trim_trailing_blanks();
    }

    fn shift_left(&mut self, start: usize, count: usize, width: usize) {
        if start >= width || count == 0 {
            return;
        }

        if self.cells.len() < width {
            self.cells.resize(width, TerminalCell::default());
        }

        for idx in start..width {
            let source = idx + count;
            self.cells[idx] = if source < width {
                self.cells[source].clone()
            } else {
                TerminalCell::default()
            };
        }
        self.trim_trailing_blanks();
    }

    fn trim_trailing_blanks(&mut self) {
        while self.cells.last().is_some_and(TerminalCell::is_blank) {
            self.cells.pop();
        }
    }

    fn text_range(&self, start: u16, end: u16) -> String {
        let mut text = String::new();
        for col in start..end {
            match self.cells.get(col as usize) {
                Some(cell) if cell.wide_continuation => {}
                Some(cell) if cell.has_contents() => text.push_str(cell.contents()),
                Some(_) | None => text.push(' '),
            }
        }
        text
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CursorState {
    row: u16,
    col: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SavedCursor {
    cursor: CursorState,
    attrs: CellAttributes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalModes {
    application_cursor: bool,
    application_keypad: bool,
    hide_cursor: bool,
    wraparound: bool,
    alternate_screen: bool,
}

impl Default for TerminalModes {
    fn default() -> Self {
        Self {
            application_cursor: false,
            application_keypad: false,
            hide_cursor: false,
            wraparound: true,
            alternate_screen: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScreenBuffer {
    scrollback: VecDeque<BufferRow>,
    screen: Vec<BufferRow>,
    cursor: CursorState,
    saved_cursor: SavedCursor,
    attrs: CellAttributes,
    scroll_top: u16,
    scroll_bottom: u16,
    pending_wrap: bool,
}

impl ScreenBuffer {
    fn new(rows: u16, scrollback_limit: usize) -> Self {
        let mut buffer = Self {
            scrollback: VecDeque::with_capacity(scrollback_limit.min(256)),
            screen: vec![BufferRow::default(); rows as usize],
            cursor: CursorState::default(),
            saved_cursor: SavedCursor::default(),
            attrs: CellAttributes::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            pending_wrap: false,
        };
        buffer.ensure_screen_rows(rows);
        buffer
    }

    fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        self.screen.get(row as usize)
    }

    fn row_mut(&mut self, row: u16) -> &mut BufferRow {
        &mut self.screen[row as usize]
    }

    fn resize(&mut self, rows: u16, scrollback_limit: usize) {
        let current_rows = self.screen.len() as u16;

        if rows > current_rows {
            let mut restored = Vec::new();
            let needed = rows as usize - current_rows as usize;
            for _ in 0..needed {
                let Some(row) = self.scrollback.pop_back() else {
                    break;
                };
                restored.push(row);
            }
            restored.reverse();
            let restored_len = restored.len() as u16;
            if !restored.is_empty() {
                let mut screen = restored;
                screen.append(&mut self.screen);
                self.screen = screen;
                self.cursor.row = self
                    .cursor
                    .row
                    .saturating_add(restored_len)
                    .min(rows.saturating_sub(1));
            }
            while self.screen.len() < rows as usize {
                self.screen.push(BufferRow::default());
            }
        } else if rows < current_rows {
            let remove = current_rows as usize - rows as usize;
            for _ in 0..remove {
                if !self.screen.is_empty() {
                    let removed = self.screen.remove(0);
                    self.push_scrollback(scrollback_limit, removed);
                }
            }
            self.cursor.row = self.cursor.row.saturating_sub(remove as u16);
        }

        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.pending_wrap = false;
        self.ensure_screen_rows(rows);

        if scrollback_limit == 0 {
            self.scrollback.clear();
        }
    }

    fn reset(&mut self, rows: u16, scrollback_limit: usize) {
        self.scrollback = VecDeque::with_capacity(scrollback_limit.min(256));
        self.screen = vec![BufferRow::default(); rows as usize];
        self.cursor = CursorState::default();
        self.saved_cursor = SavedCursor::default();
        self.attrs = CellAttributes::default();
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.pending_wrap = false;
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = SavedCursor {
            cursor: self.cursor,
            attrs: self.attrs,
        };
        self.pending_wrap = false;
    }

    fn restore_cursor(&mut self, rows: u16, cols: u16) {
        self.cursor = self.saved_cursor.cursor;
        self.attrs = self.saved_cursor.attrs;
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    fn ensure_screen_rows(&mut self, rows: u16) {
        while self.screen.len() < rows as usize {
            self.screen.push(BufferRow::default());
        }
        self.screen.truncate(rows as usize);
    }

    fn push_scrollback(&mut self, scrollback_limit: usize, row: BufferRow) {
        if scrollback_limit == 0 {
            return;
        }

        self.scrollback.push_back(row);
        while self.scrollback.len() > scrollback_limit {
            self.scrollback.pop_front();
        }
    }

    fn scroll_up_region(&mut self, rows: u16, scrollback_limit: usize, count: u16) {
        if rows == 0 {
            return;
        }

        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom.min(rows.saturating_sub(1)) as usize;
        for _ in 0..count {
            let removed = self.screen.remove(top);
            if top == 0 && bottom + 1 == rows as usize {
                self.push_scrollback(scrollback_limit, removed);
            }
            self.screen.insert(bottom, BufferRow::default());
        }
    }

    fn scroll_down_region(&mut self, rows: u16, count: u16) {
        if rows == 0 {
            return;
        }

        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom.min(rows.saturating_sub(1)) as usize;
        for _ in 0..count {
            self.screen.remove(bottom);
            self.screen.insert(top, BufferRow::default());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalState {
    rows: u16,
    cols: u16,
    scrollback_limit: usize,
    primary: ScreenBuffer,
    alternate: ScreenBuffer,
    modes: TerminalModes,
    tab_stops: Vec<bool>,
}

impl TerminalState {
    pub fn new(rows: u16, cols: u16, scrollback_limit: usize) -> Self {
        Self {
            rows,
            cols,
            scrollback_limit,
            primary: ScreenBuffer::new(rows, scrollback_limit),
            alternate: ScreenBuffer::new(rows, 0),
            modes: TerminalModes::default(),
            tab_stops: default_tab_stops(cols),
        }
    }

    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    pub fn cursor_position(&self) -> (u16, u16) {
        let cursor = self.active_buffer().cursor;
        (cursor.row, cursor.col)
    }

    pub fn hide_cursor(&self) -> bool {
        self.modes.hide_cursor
    }

    pub fn application_cursor(&self) -> bool {
        self.modes.application_cursor
    }

    pub fn application_keypad(&self) -> bool {
        self.modes.application_keypad
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }

        self.primary.resize(rows, self.scrollback_limit);
        self.alternate.resize(rows, 0);
        self.rows = rows;
        self.cols = cols;
        self.resize_tab_stops(cols);

        let max_col = cols.saturating_sub(1);
        self.primary.cursor.col = self.primary.cursor.col.min(max_col);
        self.alternate.cursor.col = self.alternate.cursor.col.min(max_col);
    }

    pub fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        self.active_buffer().visible_row(row)
    }

    pub fn cell(&self, row: u16, col: u16) -> Option<&TerminalCell> {
        self.visible_row(row)
            .and_then(|buffer_row| buffer_row.cell(col))
    }

    pub fn contents_between(
        &self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> String {
        match start_row.cmp(&end_row) {
            std::cmp::Ordering::Less => {
                let mut contents = String::new();
                for row in start_row..=end_row.min(self.rows.saturating_sub(1)) {
                    let Some(buffer_row) = self.visible_row(row) else {
                        continue;
                    };
                    if row == start_row {
                        contents
                            .push_str(&buffer_row.text_range(start_col.min(self.cols), self.cols));
                        if !buffer_row.wrapped() {
                            contents.push('\n');
                        }
                    } else if row == end_row {
                        contents.push_str(&buffer_row.text_range(0, end_col.min(self.cols)));
                    } else {
                        contents.push_str(&buffer_row.text_range(0, self.cols));
                        if !buffer_row.wrapped() {
                            contents.push('\n');
                        }
                    }
                }
                contents
            }
            std::cmp::Ordering::Equal => self
                .visible_row(start_row)
                .map(|row| row.text_range(start_col.min(self.cols), end_col.min(self.cols)))
                .unwrap_or_default(),
            std::cmp::Ordering::Greater => String::new(),
        }
    }

    pub fn dirty_rows(&self, prev: Option<&Self>) -> Vec<usize> {
        let Some(prev) = prev else {
            return (0..self.rows as usize).collect();
        };

        if prev.size() != self.size() || prev.modes.alternate_screen != self.modes.alternate_screen
        {
            return (0..self.rows as usize).collect();
        }

        let mut dirty = Vec::new();
        for row in 0..self.rows {
            if self.visible_row(row) != prev.visible_row(row) {
                dirty.push(row as usize);
            }
        }
        dirty
    }

    pub fn print(&mut self, c: char) {
        let width = UnicodeWidthChar::width(c).unwrap_or(1);
        if width == 0 {
            let row = self.active_buffer().cursor.row;
            let col = self.active_buffer().cursor.col as usize;
            self.prepare_row_for_write(row);
            self.row_mut(row).append_to_previous(col, c);
            return;
        }
        if self.cols == 0 || width > self.cols as usize {
            return;
        }

        if self.active_buffer().pending_wrap && self.modes.wraparound {
            self.wrap_to_next_line();
        }

        if self.active_buffer().cursor.col as usize + width > self.cols as usize {
            self.wrap_to_next_line();
        }

        let row = self.active_buffer().cursor.row;
        let col = self.active_buffer().cursor.col as usize;
        let attrs = self.active_buffer().attrs;
        self.prepare_row_for_write(row);
        let row_mut = self.row_mut(row);
        row_mut.clear_overwrite(col);
        if width == 2 {
            row_mut.clear_overwrite(col + 1);
        }
        row_mut.set_cell(col, TerminalCell::new(c.to_string(), attrs, width == 2));
        if width == 2 {
            row_mut.set_cell(col + 1, TerminalCell::continuation(attrs));
        }

        let next_col = col + width;
        if next_col >= self.cols as usize {
            let max_col = self.cols.saturating_sub(1);
            let buffer = self.active_buffer_mut();
            buffer.pending_wrap = true;
            buffer.cursor.col = max_col;
        } else {
            self.active_buffer_mut().cursor.col = next_col as u16;
        }
    }

    pub fn linefeed(&mut self) {
        let rows = self.rows;
        let scrollback_limit = self.active_scrollback_limit();
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        if cursor_row == scroll_bottom {
            buffer.scroll_up_region(rows, scrollback_limit, 1);
        } else if cursor_row + 1 < rows {
            buffer.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = 0;
    }

    pub fn backspace(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = buffer.cursor.col.saturating_sub(1);
    }

    pub fn tab(&mut self) {
        self.active_buffer_mut().pending_wrap = false;
        if self.cols == 0 {
            return;
        }

        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().cursor.col =
            self.next_tab_stop(cursor_col)
                .unwrap_or(self.cols.saturating_sub(1) as usize) as u16;
    }

    pub fn move_forward_tabs(&mut self, count: u16) {
        for _ in 0..count.max(1) {
            self.tab();
        }
    }

    pub fn move_backward_tabs(&mut self, count: u16) {
        self.active_buffer_mut().pending_wrap = false;
        if self.cols == 0 {
            return;
        }

        for _ in 0..count.max(1) {
            let cursor_col = self.active_buffer().cursor.col as usize;
            self.active_buffer_mut().cursor.col =
                self.previous_tab_stop(cursor_col).unwrap_or(0) as u16;
        }
    }

    pub fn set_horizontal_tabstop(&mut self) {
        let cursor_col = self.active_buffer().cursor.col as usize;
        if let Some(tab_stop) = self.tab_stops.get_mut(cursor_col) {
            *tab_stop = true;
        }
    }

    pub fn clear_current_tab_stop(&mut self) {
        let cursor_col = self.active_buffer().cursor.col as usize;
        if let Some(tab_stop) = self.tab_stops.get_mut(cursor_col) {
            *tab_stop = false;
        }
    }

    pub fn clear_all_tab_stops(&mut self) {
        self.tab_stops.fill(false);
    }

    pub fn set_default_tab_stops(&mut self, interval: u16) {
        self.tab_stops.fill(false);
        let interval = interval.max(1) as usize;
        for col in (interval..self.cols as usize).step_by(interval) {
            self.tab_stops[col] = true;
        }
    }

    pub fn reset(&mut self) {
        self.primary.reset(self.rows, self.scrollback_limit);
        self.alternate.reset(self.rows, 0);
        self.modes = TerminalModes::default();
        self.tab_stops = default_tab_stops(self.cols);
    }

    pub fn reverse_index(&mut self) {
        let rows = self.rows;
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_top = self.active_buffer().scroll_top;
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        if cursor_row == scroll_top {
            buffer.scroll_down_region(rows, 1);
        } else {
            buffer.cursor.row = buffer.cursor.row.saturating_sub(1);
        }
    }

    pub fn save_cursor(&mut self) {
        self.active_buffer_mut().save_cursor();
    }

    pub fn restore_cursor(&mut self) {
        let rows = self.rows;
        let cols = self.cols;
        self.active_buffer_mut().restore_cursor(rows, cols);
    }

    pub fn cursor_up(&mut self, count: u16) {
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.row = buffer.cursor.row.saturating_sub(count.max(1));
    }

    pub fn cursor_down(&mut self, count: u16) {
        let max_row = self.rows.saturating_sub(1);
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.row = (buffer.cursor.row + count.max(1)).min(max_row);
    }

    pub fn cursor_forward(&mut self, count: u16) {
        let max_col = self.cols.saturating_sub(1);
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = (buffer.cursor.col + count.max(1)).min(max_col);
    }

    pub fn cursor_back(&mut self, count: u16) {
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = buffer.cursor.col.saturating_sub(count.max(1));
    }

    pub fn cursor_next_line(&mut self, count: u16) {
        self.cursor_down(count);
        self.active_buffer_mut().cursor.col = 0;
    }

    pub fn cursor_prev_line(&mut self, count: u16) {
        self.cursor_up(count);
        self.active_buffer_mut().cursor.col = 0;
    }

    pub fn set_cursor_col(&mut self, col: u16) {
        let max_col = self.cols.saturating_sub(1);
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = col.min(max_col);
    }

    pub fn set_cursor_row(&mut self, row: u16) {
        let max_row = self.rows.saturating_sub(1);
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.row = row.min(max_row);
    }

    pub fn set_cursor_position(&mut self, row: u16, col: u16) {
        self.active_buffer_mut().pending_wrap = false;
        self.set_cursor_row(row);
        self.set_cursor_col(col);
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        self.active_buffer_mut().pending_wrap = false;
        match mode {
            3 => self.active_buffer_mut().scrollback.clear(),
            1 => {
                let cursor_row = self.active_buffer().cursor.row;
                for row in 0..cursor_row {
                    self.row_mut(row).clear();
                }
                let cursor_col = self.active_buffer().cursor.col as usize + 1;
                self.prepare_row_for_write(cursor_row);
                self.row_mut(cursor_row).clear_range(0, cursor_col);
            }
            2 => {
                for row in &mut self.active_buffer_mut().screen {
                    row.clear();
                }
            }
            _ => {
                let cursor_row = self.active_buffer().cursor.row;
                let cursor_col = self.active_buffer().cursor.col as usize;
                self.prepare_row_for_write(cursor_row);
                self.row_mut(cursor_row).clear_range(cursor_col, usize::MAX);
                for row in cursor_row + 1..self.rows {
                    self.row_mut(row).clear();
                }
            }
        }
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        self.prepare_row_for_write(cursor_row);
        let row = self.row_mut(cursor_row);
        match mode {
            1 => row.clear_range(0, cursor_col + 1),
            2 => row.clear(),
            _ => row.clear_range(cursor_col, usize::MAX),
        }
    }

    pub fn insert_blank_chars(&mut self, count: u16) {
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        let cols = self.cols as usize;
        self.prepare_row_for_write(cursor_row);
        self.row_mut(cursor_row)
            .shift_right(cursor_col, count.max(1) as usize, cols);
    }

    pub fn delete_chars(&mut self, count: u16) {
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        let cols = self.cols as usize;
        self.prepare_row_for_write(cursor_row);
        self.row_mut(cursor_row)
            .shift_left(cursor_col, count.max(1) as usize, cols);
    }

    pub fn erase_chars(&mut self, count: u16) {
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        self.prepare_row_for_write(cursor_row);
        self.row_mut(cursor_row)
            .clear_range(cursor_col, cursor_col + count.max(1) as usize);
    }

    pub fn insert_lines(&mut self, count: u16) {
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_top = self.active_buffer().scroll_top;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        self.active_buffer_mut().pending_wrap = false;
        if cursor_row < scroll_top || cursor_row > scroll_bottom {
            return;
        }
        let row = cursor_row as usize;
        let bottom = scroll_bottom as usize;
        for _ in 0..count.max(1) {
            self.active_buffer_mut()
                .screen
                .insert(row, BufferRow::default());
            self.active_buffer_mut().screen.remove(bottom + 1);
        }
    }

    pub fn delete_lines(&mut self, count: u16) {
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_top = self.active_buffer().scroll_top;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        self.active_buffer_mut().pending_wrap = false;
        if cursor_row < scroll_top || cursor_row > scroll_bottom {
            return;
        }
        let row = cursor_row as usize;
        let bottom = scroll_bottom as usize;
        for _ in 0..count.max(1) {
            self.active_buffer_mut().screen.remove(row);
            self.active_buffer_mut()
                .screen
                .insert(bottom, BufferRow::default());
        }
    }

    pub fn scroll_up(&mut self, count: u16) {
        let rows = self.rows;
        let scrollback_limit = self.active_scrollback_limit();
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.scroll_up_region(rows, scrollback_limit, count.max(1));
    }

    pub fn scroll_down(&mut self, count: u16) {
        let rows = self.rows;
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.scroll_down_region(rows, count.max(1));
    }

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let rows = self.rows;
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        if top >= bottom || bottom >= rows {
            buffer.scroll_top = 0;
            buffer.scroll_bottom = rows.saturating_sub(1);
        } else {
            buffer.scroll_top = top;
            buffer.scroll_bottom = bottom;
        }
        buffer.cursor = CursorState::default();
    }

    pub fn set_private_mode(&mut self, mode: u16, enabled: bool) {
        match mode {
            1 => self.modes.application_cursor = enabled,
            7 => self.modes.wraparound = enabled,
            25 => self.modes.hide_cursor = !enabled,
            47 | 1047 => self.set_alternate_screen(enabled, false),
            1049 => self.set_alternate_screen(enabled, true),
            _ => {}
        }
    }

    pub fn set_keypad_application_mode(&mut self, enabled: bool) {
        self.modes.application_keypad = enabled;
    }

    pub fn set_attr_reset(&mut self) {
        self.active_buffer_mut().attrs = CellAttributes::default();
    }

    pub fn set_fg(&mut self, color: TerminalColor) {
        self.active_buffer_mut().attrs.fg = color;
    }

    pub fn set_bg(&mut self, color: TerminalColor) {
        self.active_buffer_mut().attrs.bg = color;
    }

    pub fn set_bold(&mut self, enabled: bool) {
        let attrs = &mut self.active_buffer_mut().attrs;
        attrs.bold = enabled;
        if enabled {
            attrs.dim = false;
        }
    }

    pub fn set_dim(&mut self, enabled: bool) {
        let attrs = &mut self.active_buffer_mut().attrs;
        attrs.dim = enabled;
        if enabled {
            attrs.bold = false;
        }
    }

    pub fn set_italic(&mut self, enabled: bool) {
        self.active_buffer_mut().attrs.italic = enabled;
    }

    pub fn set_underline(&mut self, enabled: bool) {
        self.active_buffer_mut().attrs.underline = enabled;
    }

    pub fn set_inverse(&mut self, enabled: bool) {
        self.active_buffer_mut().attrs.inverse = enabled;
    }

    fn wrap_to_next_line(&mut self) {
        let rows = self.rows;
        let scrollback_limit = self.active_scrollback_limit();
        let row = self.active_buffer().cursor.row;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        self.row_mut(row).wrapped = true;
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = 0;
        if row == scroll_bottom {
            buffer.scroll_up_region(rows, scrollback_limit, 1);
        } else {
            buffer.cursor.row = (row + 1).min(rows.saturating_sub(1));
        }
    }

    fn prepare_row_for_write(&mut self, row: u16) {
        let cols = self.cols;
        self.row_mut(row).truncate_visible(cols);
        self.row_mut(row).wrapped = false;
    }

    fn resize_tab_stops(&mut self, cols: u16) {
        let old_len = self.tab_stops.len();
        self.tab_stops.resize(cols as usize, false);
        for col in old_len..cols as usize {
            self.tab_stops[col] = is_default_tab_stop(col);
        }
    }

    fn next_tab_stop(&self, start: usize) -> Option<usize> {
        self.tab_stops
            .iter()
            .enumerate()
            .skip(start.saturating_add(1))
            .find_map(|(col, stop)| stop.then_some(col))
    }

    fn previous_tab_stop(&self, start: usize) -> Option<usize> {
        self.tab_stops
            .iter()
            .take(start)
            .enumerate()
            .rev()
            .find_map(|(col, stop)| stop.then_some(col))
    }

    fn row_mut(&mut self, row: u16) -> &mut BufferRow {
        self.active_buffer_mut().row_mut(row)
    }

    fn active_buffer(&self) -> &ScreenBuffer {
        if self.modes.alternate_screen {
            &self.alternate
        } else {
            &self.primary
        }
    }

    fn active_buffer_mut(&mut self) -> &mut ScreenBuffer {
        if self.modes.alternate_screen {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    fn active_scrollback_limit(&self) -> usize {
        if self.modes.alternate_screen {
            0
        } else {
            self.scrollback_limit
        }
    }

    fn set_alternate_screen(&mut self, enabled: bool, save_cursor: bool) {
        if enabled {
            if self.modes.alternate_screen {
                return;
            }

            if save_cursor {
                self.primary.save_cursor();
            }
            self.alternate.reset(self.rows, 0);
            self.modes.alternate_screen = true;
        } else {
            if !self.modes.alternate_screen {
                return;
            }

            self.modes.alternate_screen = false;
            if save_cursor {
                self.primary.restore_cursor(self.rows, self.cols);
            }
        }
    }
}

pub fn cell_colors(cell: Option<&TerminalCell>) -> CellColors {
    let Some(cell) = cell else {
        return CellColors {
            fg: DEFAULT_FG,
            bg: DEFAULT_BG,
        };
    };

    let mut fg = resolve_fg(cell.fgcolor(), cell.bold());
    let mut bg = resolve_bg(cell.bgcolor());
    if cell.inverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    CellColors { fg, bg }
}

fn resolve_fg(color: TerminalColor, bold: bool) -> Rgb {
    match color {
        TerminalColor::Default => DEFAULT_FG,
        TerminalColor::Indexed(index) if bold && index < 8 => indexed_color(index + 8),
        TerminalColor::Indexed(index) => indexed_color(index),
        TerminalColor::Rgb(r, g, b) => Rgb::new(r, g, b),
    }
}

fn resolve_bg(color: TerminalColor) -> Rgb {
    match color {
        TerminalColor::Default => DEFAULT_BG,
        TerminalColor::Indexed(index) => indexed_color(index),
        TerminalColor::Rgb(r, g, b) => Rgb::new(r, g, b),
    }
}

fn indexed_color(index: u8) -> Rgb {
    const ANSI_16: [Rgb; 16] = [
        Rgb::new(0x22, 0x22, 0x22),
        Rgb::new(0xf3, 0x8b, 0xa8),
        Rgb::new(0xa6, 0xe3, 0xa1),
        Rgb::new(0xf9, 0xe2, 0xaf),
        Rgb::new(0x89, 0xb4, 0xfa),
        Rgb::new(0xf5, 0xc2, 0xe7),
        Rgb::new(0x94, 0xe2, 0xd5),
        Rgb::new(0xba, 0xc2, 0xde),
        Rgb::new(0x58, 0x5b, 0x70),
        Rgb::new(0xf3, 0x8b, 0xa8),
        Rgb::new(0xa6, 0xe3, 0xa1),
        Rgb::new(0xf9, 0xe2, 0xaf),
        Rgb::new(0x89, 0xb4, 0xfa),
        Rgb::new(0xf5, 0xc2, 0xe7),
        Rgb::new(0x94, 0xe2, 0xd5),
        Rgb::new(0xee, 0xef, 0xf7),
    ];

    match index {
        0..=15 => ANSI_16[index as usize],
        16..=231 => {
            let index = index - 16;
            let r = index / 36;
            let g = (index % 36) / 6;
            let b = index % 6;
            let scale = |component: u8| {
                if component == 0 {
                    0
                } else {
                    component * 40 + 55
                }
            };
            Rgb::new(scale(r), scale(g), scale(b))
        }
        232..=255 => {
            let gray = 8 + (index - 232) * 10;
            Rgb::new(gray, gray, gray)
        }
    }
}

fn default_tab_stops(cols: u16) -> Vec<bool> {
    let mut tab_stops = vec![false; cols as usize];
    for col in 0..cols as usize {
        tab_stops[col] = is_default_tab_stop(col);
    }
    tab_stops
}

fn is_default_tab_stop(col: usize) -> bool {
    col != 0 && col % 8 == 0
}

#[cfg(test)]
mod tests {
    use super::{TerminalColor, TerminalState};

    #[test]
    fn preserves_columns_when_resizing_wider_again() {
        let mut terminal = TerminalState::new(3, 12, 16);
        for ch in "abcdefghijkl".chars() {
            terminal.print(ch);
        }

        terminal.resize(3, 6);
        terminal.resize(3, 12);

        assert_eq!(
            terminal.visible_row(0).expect("row").text_range(0, 12),
            "abcdefghijkl"
        );
    }

    #[test]
    fn growing_height_restores_scrollback() {
        let mut terminal = TerminalState::new(2, 8, 16);
        for line in ["1", "2", "3", "4"] {
            for ch in line.chars() {
                terminal.print(ch);
            }
            terminal.carriage_return();
            terminal.linefeed();
        }

        terminal.resize(4, 8);

        assert_eq!(terminal.visible_row(0).expect("row").text_range(0, 1), "2");
        assert_eq!(terminal.visible_row(1).expect("row").text_range(0, 1), "3");
        assert_eq!(terminal.visible_row(2).expect("row").text_range(0, 1), "4");
    }

    #[test]
    fn color_mapping_keeps_defaults() {
        let mut terminal = TerminalState::new(1, 4, 0);
        terminal.set_fg(TerminalColor::Indexed(2));
        terminal.print('x');

        let cell = terminal.cell(0, 0).expect("cell");
        assert_eq!(cell.fgcolor(), TerminalColor::Indexed(2));
    }

    #[test]
    fn alternate_screen_preserves_primary_buffer_and_cursor_for_1049() {
        let mut terminal = TerminalState::new(2, 8, 16);
        for ch in "main".chars() {
            terminal.print(ch);
        }
        let primary_cursor = terminal.cursor_position();

        terminal.set_private_mode(1049, true);
        assert_eq!(terminal.contents_between(0, 0, 0, 4), "    ");
        assert_eq!(terminal.cursor_position(), (0, 0));

        for ch in "alt".chars() {
            terminal.print(ch);
        }
        assert_eq!(terminal.contents_between(0, 0, 0, 3), "alt");

        terminal.set_private_mode(1049, false);
        assert_eq!(terminal.contents_between(0, 0, 0, 4), "main");
        assert_eq!(terminal.cursor_position(), primary_cursor);
    }

    #[test]
    fn alternate_screen_is_cleared_on_each_entry() {
        let mut terminal = TerminalState::new(2, 8, 16);

        terminal.set_private_mode(1047, true);
        terminal.print('x');
        terminal.set_private_mode(1047, false);

        terminal.set_private_mode(47, true);
        assert_eq!(terminal.contents_between(0, 0, 0, 1), " ");
        assert_eq!(terminal.cursor_position(), (0, 0));
    }
}
