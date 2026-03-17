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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalState {
    rows: u16,
    cols: u16,
    scrollback_limit: usize,
    scrollback: VecDeque<BufferRow>,
    screen: Vec<BufferRow>,
    cursor: CursorState,
    saved_cursor: SavedCursor,
    attrs: CellAttributes,
    scroll_top: u16,
    scroll_bottom: u16,
    application_cursor: bool,
    hide_cursor: bool,
    wraparound: bool,
    pending_wrap: bool,
}

impl TerminalState {
    pub fn new(rows: u16, cols: u16, scrollback_limit: usize) -> Self {
        let mut state = Self {
            rows,
            cols,
            scrollback_limit,
            scrollback: VecDeque::with_capacity(scrollback_limit.min(256)),
            screen: vec![BufferRow::default(); rows as usize],
            cursor: CursorState::default(),
            saved_cursor: SavedCursor::default(),
            attrs: CellAttributes::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            application_cursor: false,
            hide_cursor: false,
            wraparound: true,
            pending_wrap: false,
        };
        state.ensure_screen_rows();
        state
    }

    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    pub fn cursor_position(&self) -> (u16, u16) {
        (self.cursor.row, self.cursor.col)
    }

    pub fn hide_cursor(&self) -> bool {
        self.hide_cursor
    }

    pub fn application_cursor(&self) -> bool {
        self.application_cursor
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }

        if rows > self.rows {
            let mut restored = Vec::new();
            let needed = rows as usize - self.rows as usize;
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
        } else {
            let remove = self.rows as usize - rows as usize;
            for _ in 0..remove {
                if !self.screen.is_empty() {
                    let removed = self.screen.remove(0);
                    self.push_scrollback(removed);
                }
            }
            self.cursor.row = self.cursor.row.saturating_sub(remove as u16);
        }

        self.rows = rows;
        self.cols = cols;
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.pending_wrap = false;
        self.ensure_screen_rows();
    }

    pub fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        self.screen.get(row as usize)
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

        if prev.size() != self.size() {
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
            let row = self.cursor.row;
            let col = self.cursor.col as usize;
            self.prepare_row_for_write(row);
            self.row_mut(row).append_to_previous(col, c);
            return;
        }
        if self.cols == 0 || width > self.cols as usize {
            return;
        }

        if self.pending_wrap && self.wraparound {
            self.wrap_to_next_line();
        }

        if self.cursor.col as usize + width > self.cols as usize {
            self.wrap_to_next_line();
        }

        let row = self.cursor.row;
        let col = self.cursor.col as usize;
        let attrs = self.attrs;
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
            self.pending_wrap = true;
            self.cursor.col = self.cols.saturating_sub(1);
        } else {
            self.cursor.col = next_col as u16;
        }
    }

    pub fn linefeed(&mut self) {
        self.pending_wrap = false;
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up_region(1);
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.pending_wrap = false;
        self.cursor.col = 0;
    }

    pub fn backspace(&mut self) {
        self.pending_wrap = false;
        self.cursor.col = self.cursor.col.saturating_sub(1);
    }

    pub fn tab(&mut self) {
        self.pending_wrap = false;
        let next = ((self.cursor.col / 8) + 1) * 8;
        self.cursor.col = next.min(self.cols.saturating_sub(1));
    }

    pub fn reset(&mut self) {
        self.scrollback.clear();
        self.screen = vec![BufferRow::default(); self.rows as usize];
        self.cursor = CursorState::default();
        self.saved_cursor = SavedCursor::default();
        self.attrs = CellAttributes::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
        self.application_cursor = false;
        self.hide_cursor = false;
        self.wraparound = true;
        self.pending_wrap = false;
    }

    pub fn reverse_index(&mut self) {
        self.pending_wrap = false;
        if self.cursor.row == self.scroll_top {
            self.scroll_down_region(1);
        } else {
            self.cursor.row = self.cursor.row.saturating_sub(1);
        }
    }

    pub fn save_cursor(&mut self) {
        self.saved_cursor = SavedCursor {
            cursor: self.cursor,
            attrs: self.attrs,
        };
        self.pending_wrap = false;
    }

    pub fn restore_cursor(&mut self) {
        self.cursor = self.saved_cursor.cursor;
        self.attrs = self.saved_cursor.attrs;
        self.cursor.row = self.cursor.row.min(self.rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(self.cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    pub fn cursor_up(&mut self, count: u16) {
        self.pending_wrap = false;
        self.cursor.row = self.cursor.row.saturating_sub(count.max(1));
    }

    pub fn cursor_down(&mut self, count: u16) {
        self.pending_wrap = false;
        self.cursor.row = (self.cursor.row + count.max(1)).min(self.rows.saturating_sub(1));
    }

    pub fn cursor_forward(&mut self, count: u16) {
        self.pending_wrap = false;
        self.cursor.col = (self.cursor.col + count.max(1)).min(self.cols.saturating_sub(1));
    }

    pub fn cursor_back(&mut self, count: u16) {
        self.pending_wrap = false;
        self.cursor.col = self.cursor.col.saturating_sub(count.max(1));
    }

    pub fn cursor_next_line(&mut self, count: u16) {
        self.cursor_down(count);
        self.cursor.col = 0;
    }

    pub fn cursor_prev_line(&mut self, count: u16) {
        self.cursor_up(count);
        self.cursor.col = 0;
    }

    pub fn set_cursor_col(&mut self, col: u16) {
        self.pending_wrap = false;
        self.cursor.col = col.min(self.cols.saturating_sub(1));
    }

    pub fn set_cursor_row(&mut self, row: u16) {
        self.pending_wrap = false;
        self.cursor.row = row.min(self.rows.saturating_sub(1));
    }

    pub fn set_cursor_position(&mut self, row: u16, col: u16) {
        self.pending_wrap = false;
        self.set_cursor_row(row);
        self.set_cursor_col(col);
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        self.pending_wrap = false;
        match mode {
            1 => {
                for row in 0..self.cursor.row {
                    self.row_mut(row).clear();
                }
                let cursor_col = self.cursor.col as usize + 1;
                self.prepare_row_for_write(self.cursor.row);
                self.row_mut(self.cursor.row).clear_range(0, cursor_col);
            }
            2 => {
                for row in &mut self.screen {
                    row.clear();
                }
            }
            _ => {
                let cursor_row = self.cursor.row;
                let cursor_col = self.cursor.col as usize;
                self.prepare_row_for_write(self.cursor.row);
                self.row_mut(cursor_row).clear_range(cursor_col, usize::MAX);
                for row in self.cursor.row + 1..self.rows {
                    self.row_mut(row).clear();
                }
            }
        }
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        self.pending_wrap = false;
        let cursor_row = self.cursor.row;
        let cursor_col = self.cursor.col as usize;
        self.prepare_row_for_write(cursor_row);
        let row = self.row_mut(cursor_row);
        match mode {
            1 => row.clear_range(0, cursor_col + 1),
            2 => row.clear(),
            _ => row.clear_range(cursor_col, usize::MAX),
        }
    }

    pub fn insert_blank_chars(&mut self, count: u16) {
        self.pending_wrap = false;
        let cursor_row = self.cursor.row;
        let cursor_col = self.cursor.col as usize;
        let cols = self.cols as usize;
        self.prepare_row_for_write(cursor_row);
        self.row_mut(cursor_row)
            .shift_right(cursor_col, count.max(1) as usize, cols);
    }

    pub fn delete_chars(&mut self, count: u16) {
        self.pending_wrap = false;
        let cursor_row = self.cursor.row;
        let cursor_col = self.cursor.col as usize;
        let cols = self.cols as usize;
        self.prepare_row_for_write(cursor_row);
        self.row_mut(cursor_row)
            .shift_left(cursor_col, count.max(1) as usize, cols);
    }

    pub fn erase_chars(&mut self, count: u16) {
        self.pending_wrap = false;
        let cursor_row = self.cursor.row;
        let cursor_col = self.cursor.col as usize;
        self.prepare_row_for_write(cursor_row);
        self.row_mut(cursor_row)
            .clear_range(cursor_col, cursor_col + count.max(1) as usize);
    }

    pub fn insert_lines(&mut self, count: u16) {
        self.pending_wrap = false;
        if self.cursor.row < self.scroll_top || self.cursor.row > self.scroll_bottom {
            return;
        }
        let row = self.cursor.row as usize;
        let bottom = self.scroll_bottom as usize;
        for _ in 0..count.max(1) {
            self.screen.insert(row, BufferRow::default());
            self.screen.remove(bottom + 1);
        }
    }

    pub fn delete_lines(&mut self, count: u16) {
        self.pending_wrap = false;
        if self.cursor.row < self.scroll_top || self.cursor.row > self.scroll_bottom {
            return;
        }
        let row = self.cursor.row as usize;
        let bottom = self.scroll_bottom as usize;
        for _ in 0..count.max(1) {
            self.screen.remove(row);
            self.screen.insert(bottom, BufferRow::default());
        }
    }

    pub fn scroll_up(&mut self, count: u16) {
        self.pending_wrap = false;
        self.scroll_up_region(count.max(1));
    }

    pub fn scroll_down(&mut self, count: u16) {
        self.pending_wrap = false;
        self.scroll_down_region(count.max(1));
    }

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        self.pending_wrap = false;
        if top >= bottom || bottom >= self.rows {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows.saturating_sub(1);
        } else {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        }
        self.cursor = CursorState::default();
    }

    pub fn set_private_mode(&mut self, mode: u16, enabled: bool) {
        match mode {
            1 => self.application_cursor = enabled,
            7 => self.wraparound = enabled,
            25 => self.hide_cursor = !enabled,
            _ => {}
        }
    }

    pub fn set_attr_reset(&mut self) {
        self.attrs = CellAttributes::default();
    }

    pub fn set_fg(&mut self, color: TerminalColor) {
        self.attrs.fg = color;
    }

    pub fn set_bg(&mut self, color: TerminalColor) {
        self.attrs.bg = color;
    }

    pub fn set_bold(&mut self, enabled: bool) {
        self.attrs.bold = enabled;
        if enabled {
            self.attrs.dim = false;
        }
    }

    pub fn set_dim(&mut self, enabled: bool) {
        self.attrs.dim = enabled;
        if enabled {
            self.attrs.bold = false;
        }
    }

    pub fn set_italic(&mut self, enabled: bool) {
        self.attrs.italic = enabled;
    }

    pub fn set_underline(&mut self, enabled: bool) {
        self.attrs.underline = enabled;
    }

    pub fn set_inverse(&mut self, enabled: bool) {
        self.attrs.inverse = enabled;
    }

    fn ensure_screen_rows(&mut self) {
        while self.screen.len() < self.rows as usize {
            self.screen.push(BufferRow::default());
        }
        self.screen.truncate(self.rows as usize);
    }

    fn wrap_to_next_line(&mut self) {
        self.row_mut(self.cursor.row).wrapped = true;
        self.pending_wrap = false;
        self.cursor.col = 0;
        if self.cursor.row == self.scroll_bottom {
            self.scroll_up_region(1);
        } else {
            self.cursor.row = (self.cursor.row + 1).min(self.rows.saturating_sub(1));
        }
    }

    fn prepare_row_for_write(&mut self, row: u16) {
        let cols = self.cols;
        self.row_mut(row).truncate_visible(cols);
        self.row_mut(row).wrapped = false;
    }

    fn row_mut(&mut self, row: u16) -> &mut BufferRow {
        &mut self.screen[row as usize]
    }

    fn push_scrollback(&mut self, row: BufferRow) {
        if self.scrollback_limit == 0 {
            return;
        }

        self.scrollback.push_back(row);
        while self.scrollback.len() > self.scrollback_limit {
            self.scrollback.pop_front();
        }
    }

    fn scroll_up_region(&mut self, count: u16) {
        if self.rows == 0 {
            return;
        }

        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom.min(self.rows.saturating_sub(1)) as usize;
        for _ in 0..count {
            let removed = self.screen.remove(top);
            if top == 0 && bottom + 1 == self.rows as usize {
                self.push_scrollback(removed);
            }
            self.screen.insert(bottom, BufferRow::default());
        }
    }

    fn scroll_down_region(&mut self, count: u16) {
        if self.rows == 0 {
            return;
        }

        let top = self.scroll_top as usize;
        let bottom = self.scroll_bottom.min(self.rows.saturating_sub(1)) as usize;
        for _ in 0..count {
            self.screen.remove(bottom);
            self.screen.insert(top, BufferRow::default());
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
}
