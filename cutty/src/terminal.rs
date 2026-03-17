pub mod grid;

use self::grid::{GridCell, GridRow, Line, RowStorage};
use unicode_width::UnicodeWidthChar;

pub type StableRowIndex = usize;

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

    fn blank() -> Self {
        Self::default()
    }

    fn is_blank(&self) -> bool {
        self.contents.is_empty()
            && self.attrs == CellAttributes::default()
            && !self.wide
            && !self.wide_continuation
    }
}

impl GridCell for TerminalCell {
    fn is_empty(&self) -> bool {
        self.is_blank()
    }

    fn reset(&mut self, template: &Self) {
        *self = template.clone();
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferRow {
    cells: Vec<TerminalCell>,
    occ: usize,
    wrapped: bool,
    seqno: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleLineInfo {
    pub stable_row: StableRowIndex,
    pub seqno: u64,
}

impl BufferRow {
    pub fn cell(&self, col: u16) -> Option<&TerminalCell> {
        self.cells.get(col as usize)
    }

    pub fn wrapped(&self) -> bool {
        self.wrapped
    }

    pub fn seqno(&self) -> u64 {
        self.seqno
    }

    fn from_cells(cells: Vec<TerminalCell>, occ: usize, wrapped: bool, seqno: u64) -> Self {
        Self {
            cells,
            occ,
            wrapped,
            seqno,
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
struct GridCursor {
    row: u16,
    col: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SavedCursor {
    cursor: GridCursor,
    attrs: CellAttributes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseTrackingMode {
    #[default]
    Disabled,
    Normal,
    ButtonMotion,
    AnyMotion,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MouseModes {
    normal_tracking: bool,
    button_motion: bool,
    any_motion: bool,
    sgr: bool,
}

impl MouseModes {
    fn tracking_mode(self) -> MouseTrackingMode {
        if self.any_motion {
            MouseTrackingMode::AnyMotion
        } else if self.button_motion {
            MouseTrackingMode::ButtonMotion
        } else if self.normal_tracking {
            MouseTrackingMode::Normal
        } else {
            MouseTrackingMode::Disabled
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalModes {
    application_cursor: bool,
    application_keypad: bool,
    hide_cursor: bool,
    wraparound: bool,
    alternate_screen: bool,
    bracketed_paste: bool,
    focus_reporting: bool,
    mouse: MouseModes,
}

impl Default for TerminalModes {
    fn default() -> Self {
        Self {
            application_cursor: false,
            application_keypad: false,
            hide_cursor: false,
            wraparound: true,
            alternate_screen: false,
            bracketed_paste: false,
            focus_reporting: false,
            mouse: MouseModes::default(),
        }
    }
}

type StableRowId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Row {
    id: StableRowId,
    cells: GridRow<TerminalCell>,
    occ: usize,
    wrapped: bool,
    seqno: u64,
}

impl Row {
    fn blank(id: StableRowId, width: usize, seqno: u64) -> Self {
        Self {
            id,
            cells: GridRow::new(width.max(1)),
            occ: 0,
            wrapped: false,
            seqno,
        }
    }

    fn snapshot(&self) -> BufferRow {
        BufferRow::from_cells(self.cells.to_vec(), self.occ, self.wrapped, self.seqno)
    }

    fn touch(&mut self, seqno: u64) {
        self.seqno = seqno;
    }

    fn grow(&mut self, width: usize) {
        self.cells.grow(width);
    }

    fn reset_blank(&mut self, width: usize, id: StableRowId, seqno: u64) {
        self.cells.grow(width.max(1));
        self.cells.reset(&TerminalCell::blank());
        self.id = id;
        self.occ = 0;
        self.wrapped = false;
        self.seqno = seqno;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RenderSnapshot {
    rows: Vec<BufferRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScreenBuffer {
    rows: RowStorage<Row>,
    render: RenderSnapshot,
    visible_rows: u16,
    stable_row_offset: usize,
    display_offset: usize,
    cursor: GridCursor,
    saved_cursor: SavedCursor,
    attrs: CellAttributes,
    scroll_top: u16,
    scroll_bottom: u16,
    pending_wrap: bool,
    current_input_start: Option<StableRowId>,
    next_row_id: StableRowId,
}

impl ScreenBuffer {
    fn new(rows: u16, cols: u16, seqno: u64) -> Self {
        let visible_rows = rows.max(1);
        let width = cols.max(1) as usize;
        let mut next_row_id: StableRowId = 1;
        let rows_storage = RowStorage::with_capacity(visible_rows as usize, || {
            let id = next_row_id;
            next_row_id = next_row_id.saturating_add(1);
            Row::blank(id, width, seqno)
        });
        let mut buffer = Self {
            rows: rows_storage,
            render: RenderSnapshot::default(),
            visible_rows,
            stable_row_offset: 0,
            display_offset: 0,
            cursor: GridCursor::default(),
            saved_cursor: SavedCursor::default(),
            attrs: CellAttributes::default(),
            scroll_top: 0,
            scroll_bottom: visible_rows.saturating_sub(1),
            pending_wrap: false,
            current_input_start: None,
            next_row_id,
        };
        buffer.current_input_start = buffer
            .rows
            .get(buffer.rows.len().saturating_sub(1))
            .map(|row| row.id);
        buffer.rebuild_snapshot();
        buffer
    }

    fn reset(&mut self, rows: u16, cols: u16, seqno: u64) {
        self.render.rows.clear();
        self.visible_rows = rows.max(1);
        self.stable_row_offset = 0;
        self.display_offset = 0;
        self.cursor = GridCursor::default();
        self.saved_cursor = SavedCursor::default();
        self.attrs = CellAttributes::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.visible_rows.saturating_sub(1);
        self.pending_wrap = false;
        self.current_input_start = None;
        let width = cols.max(1) as usize;
        let mut next_row_id: StableRowId = 1;
        self.rows = RowStorage::with_capacity(self.visible_rows as usize, || {
            let id = next_row_id;
            next_row_id = next_row_id.saturating_add(1);
            Row::blank(id, width, seqno)
        });
        self.next_row_id = next_row_id;
        self.current_input_start = self
            .rows
            .get(self.rows.len().saturating_sub(1))
            .map(|row| row.id);
        self.rebuild_snapshot();
    }

    fn live_start(&self) -> usize {
        self.rows
            .len()
            .saturating_sub(self.visible_rows.max(1) as usize)
    }

    fn cursor_absolute_index(&self) -> usize {
        (self.live_start() + self.cursor.row as usize).min(self.rows.len().saturating_sub(1))
    }

    fn alloc_row_id(&mut self) -> StableRowId {
        let id = self.next_row_id;
        self.next_row_id = self.next_row_id.saturating_add(1);
        id
    }

    fn current_input_absolute_index(&self) -> Option<usize> {
        let id = self.current_input_start?;
        self.rows.iter().position(|row| row.id == id)
    }

    fn reconcile_current_input_start(&mut self) {
        if self
            .current_input_start
            .is_some_and(|id| !self.rows.iter().any(|row| row.id == id))
        {
            self.current_input_start = self
                .rows
                .get(self.rows.len().saturating_sub(1))
                .map(|row| row.id);
        }
    }

    fn fill_blank_rows(&mut self, count: usize, width: usize, seqno: u64) {
        let mut next_row_id = self.next_row_id;
        self.rows.initialize(count, || {
            let id = next_row_id;
            next_row_id = next_row_id.saturating_add(1);
            Row::blank(id, width, seqno)
        });
        self.next_row_id = next_row_id;
    }

    fn rebuild_snapshot(&mut self) {
        self.render.rows = self.rows.iter().map(Row::snapshot).collect();
        self.clamp_display_offset();
    }

    fn history_size(&self) -> usize {
        self.rows.len().saturating_sub(self.visible_rows as usize)
    }

    fn clamp_display_offset(&mut self) {
        self.display_offset = self.display_offset.min(self.history_size());
    }

    fn viewport_top(&self) -> StableRowIndex {
        self.stable_row_offset + self.history_size().saturating_sub(self.display_offset)
    }

    fn set_viewport_bottom_follow(&mut self) -> bool {
        let changed = self.display_offset != 0;
        self.display_offset = 0;
        changed
    }

    fn scroll_viewport_up(&mut self, rows: u16) -> bool {
        let next = (self.display_offset + rows.max(1) as usize).min(self.history_size());
        if next == self.display_offset {
            return false;
        }
        self.display_offset = next;
        true
    }

    fn scroll_viewport_down(&mut self, rows: u16) -> bool {
        let next = self.display_offset.saturating_sub(rows.max(1) as usize);
        if next == self.display_offset {
            return false;
        }
        self.display_offset = next;
        true
    }

    fn visible_row_to_stable_row(&self, row: u16) -> StableRowIndex {
        let max_row = self.visible_rows.saturating_sub(1);
        self.viewport_top() + row.min(max_row) as usize
    }

    fn stable_row_to_visible_row(&self, stable_row: StableRowIndex) -> Option<u16> {
        let top = self.viewport_top();
        let bottom = top + self.visible_rows as usize;
        (stable_row >= top && stable_row < bottom).then_some((stable_row - top) as u16)
    }

    fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        let stable = self.visible_row_to_stable_row(row);
        self.line_for_stable_row(stable)
    }

    fn line_for_stable_row(&self, stable_row: StableRowIndex) -> Option<&BufferRow> {
        let index = stable_row.checked_sub(self.stable_row_offset)?;
        self.render.rows.get(index)
    }

    fn visible_line_info(&self, row: u16) -> Option<VisibleLineInfo> {
        let stable_row = self.visible_row_to_stable_row(row);
        self.line_for_stable_row(stable_row)
            .map(|line| VisibleLineInfo {
                stable_row,
                seqno: line.seqno(),
            })
    }

    fn visible_cursor_position(&self) -> Option<(u16, u16)> {
        let stable_cursor_row = self.stable_row_offset + self.cursor_absolute_index();
        self.stable_row_to_visible_row(stable_cursor_row)
            .map(|row| (row, self.cursor.col))
    }

    fn absolute_screen_row(&self, row: u16) -> usize {
        (self.live_start() + row.min(self.visible_rows.saturating_sub(1)) as usize)
            .min(self.rows.len().saturating_sub(1))
    }

    fn row_mut(&mut self, absolute: usize) -> &mut Row {
        self.rows.get_mut(absolute).expect("row exists")
    }

    fn row(&self, absolute: usize) -> &Row {
        self.rows.get(absolute).expect("row exists")
    }

    fn resize_height(&mut self, rows: u16, cols: u16, scrollback_limit: usize, seqno: u64) {
        let rows = rows.max(1);
        let width = cols.max(1) as usize;
        match self.visible_rows.cmp(&rows) {
            std::cmp::Ordering::Less => {
                self.grow_visible_rows(rows, width, scrollback_limit, seqno)
            }
            std::cmp::Ordering::Greater => {
                self.shrink_visible_rows(rows, width, scrollback_limit, seqno)
            }
            std::cmp::Ordering::Equal => {}
        }

        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.saved_cursor.cursor.row = self.saved_cursor.cursor.row.min(rows.saturating_sub(1));
        self.reconcile_current_input_start();
        self.rebuild_snapshot();
    }

    fn grow_visible_rows(&mut self, rows: u16, width: usize, scrollback_limit: usize, seqno: u64) {
        let lines_added = rows.saturating_sub(self.visible_rows) as usize;
        if lines_added == 0 {
            return;
        }

        let mut next_row_id = self.next_row_id;
        self.rows.grow_visible_lines(rows as usize, || {
            let id = next_row_id;
            next_row_id = next_row_id.saturating_add(1);
            Row::blank(id, width, seqno)
        });
        self.next_row_id = next_row_id;
        self.visible_rows = rows;

        let from_history = self.history_size().min(lines_added);
        if from_history != lines_added {
            let delta = lines_added - from_history;
            self.scroll_up_range(
                0,
                rows.saturating_sub(1),
                delta,
                width,
                scrollback_limit,
                seqno,
            );
        }

        self.cursor.row =
            (self.cursor.row as usize + from_history).min(rows.saturating_sub(1) as usize) as u16;
        self.saved_cursor.cursor.row = (self.saved_cursor.cursor.row as usize + from_history)
            .min(rows.saturating_sub(1) as usize) as u16;
        self.display_offset = self.display_offset.saturating_sub(lines_added);
    }

    fn shrink_visible_rows(
        &mut self,
        rows: u16,
        width: usize,
        scrollback_limit: usize,
        seqno: u64,
    ) {
        let target = rows.max(1);
        let required_scrolling = (self.cursor.row as usize + 1).saturating_sub(target as usize);
        if required_scrolling > 0 {
            let old_visible = self.visible_rows;
            self.scroll_top = 0;
            self.scroll_bottom = old_visible.saturating_sub(1);
            self.scroll_up_region(
                width as u16,
                required_scrolling as u16,
                scrollback_limit,
                seqno,
            );
            self.cursor.row = self.cursor.row.min(target.saturating_sub(1));
        }

        self.saved_cursor.cursor.row = self.saved_cursor.cursor.row.min(target.saturating_sub(1));
        self.rows
            .rotate((self.visible_rows.saturating_sub(target)) as isize);
        self.rows.shrink_visible_lines(target as usize);
        self.visible_rows = target;
    }

    fn trim_scrollback(&mut self, limit: usize) {
        let max_total = self.visible_rows as usize + limit;
        if self.rows.len() <= max_total {
            return;
        }

        let remove = self.rows.len() - max_total;
        let mut rows = self.rows.take_all();
        rows.drain(0..remove);
        self.rows.replace_inner(rows);
        self.stable_row_offset += remove;
        self.display_offset = self.display_offset.min(self.history_size());
        self.reconcile_current_input_start();
        self.rebuild_snapshot();
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = SavedCursor {
            cursor: self.cursor,
            attrs: self.attrs,
        };
    }

    fn restore_cursor(&mut self, cols: u16) {
        self.cursor = self.saved_cursor.cursor;
        self.attrs = self.saved_cursor.attrs;
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    fn set_cursor_position(&mut self, row: u16, col: u16, cols: u16) {
        self.cursor.row = row.min(self.visible_rows.saturating_sub(1));
        self.cursor.col = col.min(cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    fn move_cursor_to_row(&mut self, row: u16) {
        self.cursor.row = row.min(self.visible_rows.saturating_sub(1));
        self.pending_wrap = false;
    }

    fn move_cursor_to_col(&mut self, col: u16, cols: u16) {
        self.cursor.col = col.min(cols.saturating_sub(1));
        self.pending_wrap = false;
    }

    fn clear_pending_wrap(&mut self) {
        self.pending_wrap = false;
    }

    fn current_absolute_row(&self) -> usize {
        self.absolute_screen_row(self.cursor.row)
    }

    fn line_to_absolute(&self, line: Line) -> usize {
        (line.0 + self.history_size() as i32) as usize
    }

    fn reset_line(&mut self, line: Line, width: usize, seqno: u64) {
        let id = self.alloc_row_id();
        let absolute = self.line_to_absolute(line);
        self.row_mut(absolute).reset_blank(width, id, seqno);
    }

    fn scroll_up_range(
        &mut self,
        start: u16,
        end: u16,
        positions: usize,
        width: usize,
        scrollback_limit: usize,
        seqno: u64,
    ) {
        let start = start.min(self.visible_rows.saturating_sub(1)) as usize;
        let end = end.min(self.visible_rows.saturating_sub(1)) as usize + 1;
        let region_len = end.saturating_sub(start);
        let positions = positions.max(1).min(region_len.max(1));

        if region_len <= positions && start != 0 {
            for line in start..end {
                self.reset_line(Line(line as i32), width, seqno);
            }
            self.reconcile_current_input_start();
            self.rebuild_snapshot();
            return;
        }

        if self.display_offset != 0 {
            self.display_offset = (self.display_offset + positions).min(scrollback_limit);
        }

        if start == 0 {
            let add = positions.min(scrollback_limit.saturating_sub(self.history_size()));
            if add > 0 {
                let mut next_row_id = self.next_row_id;
                self.rows.initialize(add, || {
                    let id = next_row_id;
                    next_row_id = next_row_id.saturating_add(1);
                    Row::blank(id, width, seqno)
                });
                self.next_row_id = next_row_id;
            }

            self.rows.rotate(-(positions as isize));

            let screen_lines = self.visible_rows as i32;
            for line in ((end as i32)..screen_lines).rev() {
                self.rows.swap(Line(line), Line(line - positions as i32));
            }
        } else {
            for line in start as i32..(end as i32 - positions as i32) {
                self.rows.swap(Line(line), Line(line + positions as i32));
            }
        }

        for line in (end - positions)..end {
            self.reset_line(Line(line as i32), width, seqno);
        }

        self.reconcile_current_input_start();
        self.rebuild_snapshot();
    }

    fn scroll_down_range(
        &mut self,
        start: u16,
        end: u16,
        positions: usize,
        width: usize,
        scrollback_limit: usize,
        seqno: u64,
    ) {
        let start = start.min(self.visible_rows.saturating_sub(1)) as usize;
        let end = end.min(self.visible_rows.saturating_sub(1)) as usize + 1;
        let region_len = end.saturating_sub(start);
        let positions = positions.max(1).min(region_len.max(1));

        if region_len <= positions {
            for line in start..end {
                self.reset_line(Line(line as i32), width, seqno);
            }
            self.reconcile_current_input_start();
            self.rebuild_snapshot();
            return;
        }

        if scrollback_limit == 0 {
            let screen_lines = self.visible_rows as i32;
            for line in (end as i32..screen_lines).map(Line) {
                self.rows.swap(line, line - positions as i32);
            }

            self.rows.rotate_down(positions);

            for line in (0..positions).map(Line::from) {
                self.reset_line(line, width, seqno);
            }

            for line in (0..start).map(Line::from) {
                self.rows.swap(line, line + positions);
            }
        } else {
            for line in ((start + positions)..end).rev().map(Line::from) {
                self.rows.swap(line, line - positions);
            }

            for line in (start..(start + positions)).rev().map(Line::from) {
                self.reset_line(line, width, seqno);
            }
        }

        self.reconcile_current_input_start();
        self.rebuild_snapshot();
    }

    fn clear_row_range(
        &mut self,
        absolute: usize,
        start: usize,
        end: usize,
        width: usize,
        seqno: u64,
    ) {
        let full_row = start == 0 && end >= width;
        let row = self.row_mut(absolute);
        let mut cells = expanded_row_cells(row, width);
        for col in start.min(width)..end.min(width) {
            clear_overlap(&mut cells, col, width);
            cells[col] = TerminalCell::blank();
        }
        row.touch(seqno);
        if full_row {
            row.wrapped = false;
        }
        normalize_cells(&mut cells, width);
        row.cells = GridRow::from_vec(finalize_row_cells(cells, row.wrapped, width), width);
        row.occ = row_occupied_columns(row.cells.as_slice());
    }

    fn replace_row_cells(
        &mut self,
        absolute: usize,
        cells: Vec<TerminalCell>,
        wrapped: bool,
        width: usize,
        seqno: u64,
    ) {
        let row = self.row_mut(absolute);
        row.cells = GridRow::from_vec(finalize_row_cells(cells, wrapped, width), width);
        row.occ = row_occupied_columns(row.cells.as_slice());
        row.wrapped = wrapped;
        row.touch(seqno);
    }

    fn put_char(
        &mut self,
        c: char,
        cols: u16,
        seqno: u64,
        wraparound: bool,
        scrollback_limit: usize,
    ) {
        let width = cols.max(1) as usize;
        let char_width = UnicodeWidthChar::width(c).unwrap_or(1);
        if char_width == 0 {
            self.append_combining(c, width, seqno);
            return;
        }
        if char_width > width {
            return;
        }

        if self.pending_wrap && wraparound {
            self.soft_wrap(cols, seqno, scrollback_limit);
        }

        if self.cursor.col as usize + char_width > width {
            if wraparound {
                self.soft_wrap(cols, seqno, scrollback_limit);
            } else {
                self.cursor.col = width.saturating_sub(char_width) as u16;
            }
        }

        let absolute = self.current_absolute_row();
        if self.current_input_start.is_none() {
            self.current_input_start = Some(self.row(absolute).id);
        }
        let attrs = self.attrs;
        let col = self.cursor.col as usize;
        let row = self.row_mut(absolute);
        let mut cells = expanded_row_cells(row, width);
        clear_overlap(&mut cells, col, width);
        if char_width == 2 {
            clear_overlap(&mut cells, col + 1, width);
        }
        cells[col] = TerminalCell::new(c.to_string(), attrs, char_width == 2);
        if char_width == 2 {
            cells[col + 1] = TerminalCell::continuation(attrs);
        }
        normalize_cells(&mut cells, width);
        row.cells = GridRow::from_vec(finalize_row_cells(cells, row.wrapped, width), width);
        row.occ = row_occupied_columns(row.cells.as_slice());
        row.touch(seqno);

        let next_col = col + char_width;
        if next_col >= width {
            self.cursor.col = cols.saturating_sub(1);
            self.pending_wrap = wraparound;
        } else {
            self.cursor.col = next_col as u16;
            self.pending_wrap = false;
        }
    }

    fn append_combining(&mut self, c: char, width: usize, seqno: u64) {
        let absolute = self.current_absolute_row();
        let col = self.cursor.col as usize;
        let row = self.row_mut(absolute);
        let mut cells = expanded_row_cells(row, width);
        if let Some(target) = find_combining_target(&cells, col) {
            cells[target].contents.push(c);
            row.touch(seqno);
            row.cells = GridRow::from_vec(finalize_row_cells(cells, row.wrapped, width), width);
            row.occ = row_occupied_columns(row.cells.as_slice());
        }
    }

    fn soft_wrap(&mut self, cols: u16, seqno: u64, scrollback_limit: usize) {
        let width = cols.max(1) as usize;
        let absolute = self.current_absolute_row();
        let row = self.row_mut(absolute);
        let mut cells = expanded_row_cells(row, width);
        normalize_cells(&mut cells, width);
        row.cells = GridRow::from_vec(finalize_row_cells(cells, true, width), width);
        row.occ = row_occupied_columns(row.cells.as_slice());
        row.wrapped = true;
        row.touch(seqno);

        if self.cursor.row == self.scroll_bottom {
            self.scroll_up_region(cols, 1, scrollback_limit, seqno);
            self.cursor.row = self.scroll_bottom;
        } else {
            self.cursor.row = self.cursor.row.saturating_add(1);
        }
        self.cursor.col = 0;
        self.pending_wrap = false;
    }

    fn insert_blank_chars(&mut self, cols: u16, count: u16, seqno: u64) {
        let width = cols.max(1) as usize;
        let count = count.max(1) as usize;
        let absolute = self.current_absolute_row();
        let cursor_col = self.cursor.col as usize;
        let wrapped = self.row(absolute).wrapped;
        let mut cells = expanded_row_cells(self.row(absolute), width);
        for idx in (cursor_col..width).rev() {
            cells[idx] = if idx >= cursor_col + count {
                cells[idx - count].clone()
            } else {
                TerminalCell::blank()
            };
        }
        normalize_cells(&mut cells, width);
        self.replace_row_cells(absolute, cells, wrapped, width, seqno);
    }

    fn delete_chars(&mut self, cols: u16, count: u16, seqno: u64) {
        let width = cols.max(1) as usize;
        let count = count.max(1) as usize;
        let absolute = self.current_absolute_row();
        let cursor_col = self.cursor.col as usize;
        let wrapped = self.row(absolute).wrapped;
        let mut cells = expanded_row_cells(self.row(absolute), width);
        for idx in cursor_col..width {
            cells[idx] = if idx + count < width {
                cells[idx + count].clone()
            } else {
                TerminalCell::blank()
            };
        }
        normalize_cells(&mut cells, width);
        self.replace_row_cells(absolute, cells, wrapped, width, seqno);
    }

    fn erase_chars(&mut self, cols: u16, count: u16, seqno: u64) {
        let width = cols.max(1) as usize;
        let absolute = self.current_absolute_row();
        let start = self.cursor.col as usize;
        self.clear_row_range(absolute, start, start + count.max(1) as usize, width, seqno);
    }

    fn insert_lines(&mut self, count: u16, cols: u16, seqno: u64) {
        if self.cursor.row < self.scroll_top || self.cursor.row > self.scroll_bottom {
            return;
        }
        self.scroll_down_range(
            self.cursor.row,
            self.scroll_bottom,
            count.max(1) as usize,
            cols.max(1) as usize,
            0,
            seqno,
        );
    }

    fn delete_lines(&mut self, count: u16, cols: u16, seqno: u64) {
        if self.cursor.row < self.scroll_top || self.cursor.row > self.scroll_bottom {
            return;
        }
        self.scroll_up_range(
            self.cursor.row,
            self.scroll_bottom,
            count.max(1) as usize,
            cols.max(1) as usize,
            0,
            seqno,
        );
    }

    fn scroll_up_region(&mut self, cols: u16, count: u16, scrollback_limit: usize, seqno: u64) {
        self.scroll_up_range(
            self.scroll_top,
            self.scroll_bottom,
            count.max(1) as usize,
            cols.max(1) as usize,
            scrollback_limit,
            seqno,
        );
        self.trim_scrollback(scrollback_limit);
    }

    fn scroll_down_region(&mut self, cols: u16, count: u16, scrollback_limit: usize, seqno: u64) {
        self.scroll_down_range(
            self.scroll_top,
            self.scroll_bottom,
            count.max(1) as usize,
            cols.max(1) as usize,
            scrollback_limit,
            seqno,
        );
    }

    fn clear_scrollback(&mut self, seqno: u64) {
        let live_start = self.live_start();
        let removed = live_start;
        let mut rows = self.rows.take_all();
        rows.drain(0..removed);
        self.rows.replace_inner(rows);
        self.stable_row_offset += removed;
        self.display_offset = 0;
        self.reconcile_current_input_start();
        let _ = seqno;
        self.rebuild_snapshot();
    }

    fn resize_width(&mut self, old_cols: u16, new_cols: u16, seqno: u64, allow_rewrap: bool) {
        let old_width = old_cols.max(1) as usize;
        let new_width = new_cols.max(1) as usize;
        let old_display_offset = self.display_offset;
        if old_width == new_width || self.rows.is_empty() {
            self.rebuild_snapshot();
            return;
        }

        if !allow_rewrap {
            for row in self.rows.iter_mut() {
                if new_width < old_width {
                    let mut cells = row.cells.to_vec();
                    cells.truncate(new_width);
                    row.cells = GridRow::from_vec(
                        finalize_row_cells(cells, row.wrapped, new_width),
                        new_width,
                    );
                } else {
                    row.grow(new_width);
                }
                row.occ = row_occupied_columns(row.cells.as_slice());
                row.touch(seqno);
            }
            if self.rows.len() < self.visible_rows as usize {
                let add = self.visible_rows as usize - self.rows.len();
                self.fill_blank_rows(add, new_width, seqno);
            }
            self.cursor.col = self.cursor.col.min(new_cols.saturating_sub(1));
            self.saved_cursor.cursor.col =
                self.saved_cursor.cursor.col.min(new_cols.saturating_sub(1));
            self.pending_wrap = false;
            self.rebuild_snapshot();
            return;
        }

        let old_rows: Vec<Row> = self.rows.take_all();
        let old_live_start = old_rows.len().saturating_sub(self.visible_rows as usize);
        let cursor_abs = old_live_start + self.cursor.row as usize;
        let saved_abs = old_live_start + self.saved_cursor.cursor.row as usize;
        let input_start_abs = self
            .current_input_start
            .and_then(|id| old_rows.iter().position(|row| row.id == id));
        let viewport_abs = self
            .viewport_top()
            .saturating_sub(self.stable_row_offset)
            .min(old_rows.len().saturating_sub(1));

        let mut new_rows = Vec::new();
        let mut cursor_new_abs = 0usize;
        let mut cursor_new_col = self.cursor.col as usize;
        let mut saved_new_abs = 0usize;
        let mut saved_new_col = self.saved_cursor.cursor.col as usize;
        let mut input_start_new_abs = None;
        let mut viewport_new_abs = 0usize;

        let mut index = 0;
        while index < old_rows.len() {
            let group_start = index;
            let mut logical_cells = Vec::new();
            let mut offsets = Vec::new();
            loop {
                let row = &old_rows[index];
                offsets.push(logical_cells.len());

                let mut segment_len = row.occ;
                if index == cursor_abs {
                    segment_len = segment_len.max(self.cursor.col as usize);
                }
                if index == saved_abs {
                    segment_len = segment_len.max(self.saved_cursor.cursor.col as usize);
                }
                let mut segment = row.cells.to_vec();
                segment.resize(segment_len, TerminalCell::blank());
                logical_cells.extend(segment);

                let wrapped = row.wrapped;
                index += 1;
                if !wrapped || index >= old_rows.len() {
                    break;
                }
            }

            if logical_cells.is_empty() {
                logical_cells.push(TerminalCell::blank());
            }

            let segments = wrap_logical_row(&logical_cells, new_width);
            let group_output_start = new_rows.len();
            let segment_count = segments.len();
            for (segment_index, segment) in segments.into_iter().enumerate() {
                let wrapped = segment_index + 1 < segment_count;
                let id = self.alloc_row_id();
                let cells = finalize_row_cells(segment, wrapped, new_width);
                let occ = row_occupied_columns(&cells);
                new_rows.push(Row {
                    id,
                    cells: GridRow::from_vec(cells, new_width),
                    occ,
                    wrapped,
                    seqno,
                });
            }

            if cursor_abs >= group_start && cursor_abs < index {
                let logical_col = offsets[cursor_abs - group_start] + self.cursor.col as usize;
                let (row_offset, col) =
                    map_logical_col_to_rows(&logical_cells, logical_col, new_width);
                cursor_new_abs = group_output_start + row_offset;
                cursor_new_col = col;
            }
            if saved_abs >= group_start && saved_abs < index {
                let logical_col =
                    offsets[saved_abs - group_start] + self.saved_cursor.cursor.col as usize;
                let (row_offset, col) =
                    map_logical_col_to_rows(&logical_cells, logical_col, new_width);
                saved_new_abs = group_output_start + row_offset;
                saved_new_col = col;
            }
            if let Some(input_abs) =
                input_start_abs.filter(|abs| *abs >= group_start && *abs < index)
            {
                let logical_col = offsets[input_abs - group_start];
                let (row_offset, _) =
                    map_logical_col_to_rows(&logical_cells, logical_col, new_width);
                input_start_new_abs = Some(group_output_start + row_offset);
            }
            if viewport_abs >= group_start && viewport_abs < index {
                let logical_col = offsets[viewport_abs - group_start];
                let (row_offset, _) =
                    map_logical_col_to_rows(&logical_cells, logical_col, new_width);
                viewport_new_abs = group_output_start + row_offset;
            }
        }

        self.rows.replace_inner(new_rows);
        if self.rows.len() < self.visible_rows as usize {
            let add = self.visible_rows as usize - self.rows.len();
            self.fill_blank_rows(add, new_width, seqno);
        }

        let live_start = self.live_start();
        self.cursor.row = cursor_new_abs
            .saturating_sub(live_start)
            .min(self.visible_rows.saturating_sub(1) as usize) as u16;
        self.cursor.col = cursor_new_col.min(new_width.saturating_sub(1)) as u16;
        self.saved_cursor.cursor.row = saved_new_abs
            .saturating_sub(live_start)
            .min(self.visible_rows.saturating_sub(1) as usize)
            as u16;
        self.saved_cursor.cursor.col = saved_new_col.min(new_width.saturating_sub(1)) as u16;
        self.current_input_start =
            input_start_new_abs.and_then(|absolute| self.rows.get(absolute).map(|row| row.id));
        self.display_offset = if old_display_offset == 0 {
            0
        } else {
            self.history_size().saturating_sub(viewport_new_abs)
        };
        self.pending_wrap = false;
        self.rebuild_snapshot();
    }
}

pub struct TerminalState {
    rows: u16,
    cols: u16,
    scrollback_limit: usize,
    next_seqno: u64,
    primary: ScreenBuffer,
    alternate: ScreenBuffer,
    modes: TerminalModes,
    tab_stops: Vec<bool>,
}

impl TerminalState {
    pub fn new(rows: u16, cols: u16, scrollback_limit: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            rows,
            cols,
            scrollback_limit,
            next_seqno: 1,
            primary: ScreenBuffer::new(rows, cols, 0),
            alternate: ScreenBuffer::new(rows, cols, 0),
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

    pub fn visible_cursor_position(&self) -> Option<(u16, u16)> {
        self.active_buffer().visible_cursor_position()
    }

    pub fn viewport_top(&self) -> StableRowIndex {
        self.active_buffer().viewport_top()
    }

    pub fn follow_viewport_bottom(&mut self) -> bool {
        self.active_buffer_mut().set_viewport_bottom_follow()
    }

    pub fn scroll_viewport_up(&mut self, rows: u16) -> bool {
        self.active_buffer_mut().scroll_viewport_up(rows)
    }

    pub fn scroll_viewport_down(&mut self, rows: u16) -> bool {
        self.active_buffer_mut().scroll_viewport_down(rows)
    }

    pub fn visible_row_to_stable_row(&self, row: u16) -> StableRowIndex {
        self.active_buffer().visible_row_to_stable_row(row)
    }

    pub fn stable_row_to_visible_row(&self, stable_row: StableRowIndex) -> Option<u16> {
        self.active_buffer().stable_row_to_visible_row(stable_row)
    }

    pub fn visible_line_info(&self, row: u16) -> Option<VisibleLineInfo> {
        self.active_buffer().visible_line_info(row)
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

    pub fn bracketed_paste(&self) -> bool {
        self.modes.bracketed_paste
    }

    pub fn focus_reporting(&self) -> bool {
        self.modes.focus_reporting
    }

    pub fn mouse_tracking_mode(&self) -> MouseTrackingMode {
        self.modes.mouse.tracking_mode()
    }

    pub fn mouse_reporting_enabled(&self) -> bool {
        self.mouse_tracking_mode() != MouseTrackingMode::Disabled
    }

    pub fn sgr_mouse(&self) -> bool {
        self.modes.mouse.sgr
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if rows == self.rows && cols == self.cols {
            return;
        }

        let seqno = self.bump_seqno();
        if cols != self.cols {
            self.primary.resize_width(self.cols, cols, seqno, true);
            self.alternate.resize_width(self.cols, cols, seqno, false);
        }
        if rows != self.rows {
            self.primary
                .resize_height(rows, cols, self.scrollback_limit, seqno);
            self.alternate.resize_height(rows, cols, 0, seqno);
        }

        self.rows = rows;
        self.cols = cols;
        self.resize_tab_stops(cols);
        self.primary.pending_wrap = false;
        self.alternate.pending_wrap = false;
        self.primary.trim_scrollback(self.scrollback_limit);
    }

    pub fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        self.active_buffer().visible_row(row)
    }

    pub fn line_for_stable_row(&self, stable_row: StableRowIndex) -> Option<&BufferRow> {
        self.active_buffer().line_for_stable_row(stable_row)
    }

    pub fn cell(&self, row: u16, col: u16) -> Option<&TerminalCell> {
        self.visible_row(row)
            .and_then(|buffer_row| buffer_row.cell(col))
    }

    pub fn cell_at_stable_row(
        &self,
        stable_row: StableRowIndex,
        col: u16,
    ) -> Option<&TerminalCell> {
        self.line_for_stable_row(stable_row)
            .and_then(|buffer_row| buffer_row.cell(col))
    }

    pub fn contents_between(
        &self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> String {
        let max_row = self.rows.saturating_sub(1);
        self.contents_between_stable(
            self.visible_row_to_stable_row(start_row.min(max_row)),
            start_col,
            self.visible_row_to_stable_row(end_row.min(max_row)),
            end_col,
        )
    }

    pub fn contents_between_stable(
        &self,
        start_row: StableRowIndex,
        start_col: u16,
        end_row: StableRowIndex,
        end_col: u16,
    ) -> String {
        match start_row.cmp(&end_row) {
            std::cmp::Ordering::Less => {
                let mut contents = String::new();
                for row in start_row..=end_row {
                    let Some(buffer_row) = self.line_for_stable_row(row) else {
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
                .line_for_stable_row(start_row)
                .map(|row| row.text_range(start_col.min(self.cols), end_col.min(self.cols)))
                .unwrap_or_default(),
            std::cmp::Ordering::Greater => String::new(),
        }
    }

    pub fn print(&mut self, c: char) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let wraparound = self.modes.wraparound;
        let scrollback_limit = self.active_scrollback_limit();
        self.active_buffer_mut()
            .put_char(c, cols, seqno, wraparound, scrollback_limit);
        self.active_buffer_mut().rebuild_snapshot();
    }

    pub fn linefeed(&mut self) {
        let seqno = self.bump_seqno();
        let cols = self.cols;
        let scrollback_limit = self.active_scrollback_limit();
        let buffer = self.active_buffer_mut();
        if buffer.cursor.row == buffer.scroll_bottom {
            buffer.scroll_up_region(cols, 1, scrollback_limit, seqno);
            buffer.cursor.row = buffer.scroll_bottom;
        } else {
            buffer.cursor.row = buffer.cursor.row.saturating_add(1);
        }
        buffer.pending_wrap = false;
        buffer.current_input_start = Some(buffer.row(buffer.current_absolute_row()).id);
    }

    pub fn carriage_return(&mut self) {
        self.active_buffer_mut().cursor.col = 0;
        self.active_buffer_mut().clear_pending_wrap();
    }

    pub fn backspace(&mut self) {
        let buffer = self.active_buffer_mut();
        buffer.cursor.col = buffer.cursor.col.saturating_sub(1);
        buffer.clear_pending_wrap();
    }

    pub fn tab(&mut self) {
        if self.cols == 0 {
            return;
        }
        let cols = self.cols;
        let cursor_col = self.active_buffer().cursor.col as usize;
        let next = self
            .next_tab_stop(cursor_col)
            .unwrap_or(self.cols.saturating_sub(1) as usize) as u16;
        self.active_buffer_mut().move_cursor_to_col(next, cols);
    }

    pub fn move_forward_tabs(&mut self, count: u16) {
        for _ in 0..count.max(1) {
            self.tab();
        }
    }

    pub fn move_backward_tabs(&mut self, count: u16) {
        let cols = self.cols;
        let mut cursor_col = self.active_buffer().cursor.col as usize;
        for _ in 0..count.max(1) {
            cursor_col = self.previous_tab_stop(cursor_col).unwrap_or(0);
        }
        self.active_buffer_mut()
            .move_cursor_to_col(cursor_col as u16, cols);
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
        let seqno = self.bump_seqno();
        self.primary.reset(self.rows, self.cols, seqno);
        self.alternate.reset(self.rows, self.cols, seqno);
        self.modes = TerminalModes::default();
        self.tab_stops = default_tab_stops(self.cols);
    }

    pub fn reverse_index(&mut self) {
        let seqno = self.bump_seqno();
        let cols = self.cols;
        let scrollback_limit = self.active_scrollback_limit();
        let buffer = self.active_buffer_mut();
        if buffer.cursor.row == buffer.scroll_top {
            buffer.scroll_down_region(cols, 1, scrollback_limit, seqno);
        } else {
            buffer.cursor.row = buffer.cursor.row.saturating_sub(1);
        }
        buffer.pending_wrap = false;
    }

    pub fn save_cursor(&mut self) {
        self.active_buffer_mut().save_cursor();
    }

    pub fn restore_cursor(&mut self) {
        let cols = self.cols;
        self.active_buffer_mut().restore_cursor(cols);
    }

    pub fn cursor_up(&mut self, count: u16) {
        let buffer = self.active_buffer_mut();
        let top = if buffer.cursor.row >= buffer.scroll_top
            && buffer.cursor.row <= buffer.scroll_bottom
        {
            buffer.scroll_top
        } else {
            0
        };
        buffer.cursor.row = buffer.cursor.row.saturating_sub(count.max(1)).max(top);
        buffer.pending_wrap = false;
    }

    pub fn cursor_down(&mut self, count: u16) {
        let rows = self.rows;
        let buffer = self.active_buffer_mut();
        let bottom = if buffer.cursor.row >= buffer.scroll_top
            && buffer.cursor.row <= buffer.scroll_bottom
        {
            buffer.scroll_bottom
        } else {
            rows.saturating_sub(1)
        };
        buffer.cursor.row = (buffer.cursor.row + count.max(1)).min(bottom);
        buffer.pending_wrap = false;
    }

    pub fn cursor_forward(&mut self, count: u16) {
        let cols = self.cols;
        let buffer = self.active_buffer_mut();
        buffer.cursor.col = (buffer.cursor.col + count.max(1)).min(cols.saturating_sub(1));
        buffer.pending_wrap = false;
    }

    pub fn cursor_back(&mut self, count: u16) {
        let buffer = self.active_buffer_mut();
        buffer.cursor.col = buffer.cursor.col.saturating_sub(count.max(1));
        buffer.pending_wrap = false;
    }

    pub fn cursor_next_line(&mut self, count: u16) {
        self.cursor_down(count);
        self.carriage_return();
    }

    pub fn cursor_prev_line(&mut self, count: u16) {
        self.cursor_up(count);
        self.carriage_return();
    }

    pub fn set_cursor_col(&mut self, col: u16) {
        let cols = self.cols;
        self.active_buffer_mut().move_cursor_to_col(col, cols);
    }

    pub fn set_cursor_row(&mut self, row: u16) {
        self.active_buffer_mut().move_cursor_to_row(row);
    }

    pub fn set_cursor_position(&mut self, row: u16, col: u16) {
        let cols = self.cols;
        self.active_buffer_mut().set_cursor_position(row, col, cols);
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        let cols = self.cols.max(1) as usize;
        let seqno = self.bump_seqno();
        let rows = self.rows;
        let buffer = self.active_buffer_mut();
        match mode {
            3 => buffer.clear_scrollback(seqno),
            1 => {
                for row in 0..buffer.cursor.row {
                    let absolute = buffer.absolute_screen_row(row);
                    buffer.clear_row_range(absolute, 0, cols, cols, seqno);
                }
                let absolute = buffer.current_absolute_row();
                buffer.clear_row_range(absolute, 0, buffer.cursor.col as usize + 1, cols, seqno);
            }
            2 => {
                for row in 0..rows {
                    let absolute = buffer.absolute_screen_row(row);
                    buffer.clear_row_range(absolute, 0, cols, cols, seqno);
                }
            }
            _ => {
                let absolute = buffer.current_absolute_row();
                if buffer.cursor.col == 0 {
                    let mut clear_start = buffer.current_input_absolute_index().unwrap_or(absolute);
                    if clear_start > absolute {
                        clear_start = absolute;
                    }
                    while clear_start > 0 && buffer.row(clear_start - 1).wrapped {
                        clear_start -= 1;
                    }
                    for row_abs in clear_start..absolute {
                        buffer.clear_row_range(row_abs, 0, cols, cols, seqno);
                    }
                }
                buffer.clear_row_range(absolute, buffer.cursor.col as usize, cols, cols, seqno);
                for row in (buffer.cursor.row + 1)..rows {
                    let absolute = buffer.absolute_screen_row(row);
                    buffer.clear_row_range(absolute, 0, cols, cols, seqno);
                }
            }
        }
        buffer.rebuild_snapshot();
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        let cols = self.cols.max(1) as usize;
        let seqno = self.bump_seqno();
        let buffer = self.active_buffer_mut();
        let absolute = buffer.current_absolute_row();
        match mode {
            1 => buffer.clear_row_range(absolute, 0, buffer.cursor.col as usize + 1, cols, seqno),
            2 => buffer.clear_row_range(absolute, 0, cols, cols, seqno),
            _ => buffer.clear_row_range(absolute, buffer.cursor.col as usize, cols, cols, seqno),
        }
        buffer.rebuild_snapshot();
    }

    pub fn insert_blank_chars(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut()
            .insert_blank_chars(cols, count, seqno);
        self.active_buffer_mut().rebuild_snapshot();
    }

    pub fn delete_chars(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut().delete_chars(cols, count, seqno);
        self.active_buffer_mut().rebuild_snapshot();
    }

    pub fn erase_chars(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut().erase_chars(cols, count, seqno);
        self.active_buffer_mut().rebuild_snapshot();
    }

    pub fn insert_lines(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut().insert_lines(count, cols, seqno);
    }

    pub fn delete_lines(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut().delete_lines(count, cols, seqno);
    }

    pub fn scroll_up(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let limit = self.active_scrollback_limit();
        self.active_buffer_mut()
            .scroll_up_region(cols, count, limit, seqno);
    }

    pub fn scroll_down(&mut self, count: u16) {
        let cols = self.cols;
        let scrollback_limit = self.active_scrollback_limit();
        let seqno = self.bump_seqno();
        self.active_buffer_mut()
            .scroll_down_region(cols, count, scrollback_limit, seqno);
    }

    pub fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let rows = self.rows;
        let buffer = self.active_buffer_mut();
        if top >= bottom || bottom >= rows {
            buffer.scroll_top = 0;
            buffer.scroll_bottom = rows.saturating_sub(1);
        } else {
            buffer.scroll_top = top;
            buffer.scroll_bottom = bottom;
        }
        buffer.cursor = GridCursor::default();
        buffer.pending_wrap = false;
    }

    pub fn set_private_mode(&mut self, mode: u16, enabled: bool) {
        match mode {
            1 => self.modes.application_cursor = enabled,
            7 => self.modes.wraparound = enabled,
            25 => self.modes.hide_cursor = !enabled,
            1000 => self.modes.mouse.normal_tracking = enabled,
            1002 => self.modes.mouse.button_motion = enabled,
            1003 => self.modes.mouse.any_motion = enabled,
            1004 => self.modes.focus_reporting = enabled,
            1006 => self.modes.mouse.sgr = enabled,
            47 | 1047 => self.set_alternate_screen(enabled, false),
            1049 => self.set_alternate_screen(enabled, true),
            2004 => self.modes.bracketed_paste = enabled,
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

    fn bump_seqno(&mut self) -> u64 {
        let seqno = self.next_seqno;
        self.next_seqno = self.next_seqno.saturating_add(1);
        seqno
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
            let seqno = self.bump_seqno();
            self.alternate.reset(self.rows, self.cols, seqno);
            self.modes.alternate_screen = true;
        } else {
            if !self.modes.alternate_screen {
                return;
            }

            self.modes.alternate_screen = false;
            if save_cursor {
                self.primary.restore_cursor(self.cols);
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

fn expanded_row_cells(row: &Row, width: usize) -> Vec<TerminalCell> {
    let mut cells = row.cells.to_vec();
    cells.resize(width, TerminalCell::blank());
    cells
}

fn finalize_row_cells(
    mut cells: Vec<TerminalCell>,
    wrapped: bool,
    width: usize,
) -> Vec<TerminalCell> {
    normalize_cells(&mut cells, width);
    let _ = wrapped;
    cells.resize(width, TerminalCell::blank());
    cells
}

fn clear_overlap(cells: &mut [TerminalCell], col: usize, width: usize) {
    if col >= width {
        return;
    }
    if cells[col].wide_continuation {
        if col > 0 && cells[col - 1].wide {
            cells[col - 1] = TerminalCell::blank();
        }
        cells[col] = TerminalCell::blank();
    }
    if cells[col].wide {
        cells[col] = TerminalCell::blank();
        if col + 1 < width {
            cells[col + 1] = TerminalCell::blank();
        }
    }
    if col > 0 && cells[col - 1].wide {
        cells[col - 1] = TerminalCell::blank();
        cells[col] = TerminalCell::blank();
    }
}

fn normalize_cells(cells: &mut Vec<TerminalCell>, width: usize) {
    cells.resize(width, TerminalCell::blank());
    let mut col = 0;
    while col < width {
        if cells[col].wide_continuation {
            if col == 0 || !cells[col - 1].wide {
                cells[col] = TerminalCell::blank();
            }
            col += 1;
            continue;
        }

        if cells[col].wide {
            if col + 1 >= width {
                cells[col] = TerminalCell::blank();
                col += 1;
                continue;
            }
            let attrs = cells[col].attrs;
            cells[col + 1] = TerminalCell::continuation(attrs);
            col += 2;
            continue;
        }

        if col + 1 < width && cells[col + 1].wide_continuation {
            cells[col + 1] = TerminalCell::blank();
        }
        col += 1;
    }
}

fn find_combining_target(cells: &[TerminalCell], col: usize) -> Option<usize> {
    let mut index = col.min(cells.len());
    while index > 0 {
        index -= 1;
        let cell = &cells[index];
        if cell.wide_continuation {
            continue;
        }
        if cell.has_contents() {
            return Some(index);
        }
    }
    None
}

fn wrap_logical_row(cells: &[TerminalCell], width: usize) -> Vec<Vec<TerminalCell>> {
    if cells.is_empty() {
        return vec![Vec::new()];
    }

    let mut wrapped = Vec::new();
    let mut start = 0;
    while start < cells.len() {
        let end = reflow_break(cells, start, width);
        wrapped.push(cells[start..end].to_vec());
        start = end;
    }
    wrapped
}

fn map_logical_col_to_rows(
    cells: &[TerminalCell],
    logical_col: usize,
    width: usize,
) -> (usize, usize) {
    let segments = wrap_logical_row(cells, width);
    let mut remaining = logical_col;
    for (row_index, segment) in segments.iter().enumerate() {
        let is_last = row_index + 1 == segments.len();
        let span = segment.len();
        if remaining < span || (is_last && remaining <= span) {
            return (row_index, remaining.min(width.saturating_sub(1)));
        }
        remaining = remaining.saturating_sub(span);
    }

    let last_row = segments.len().saturating_sub(1);
    let last_col = segments
        .last()
        .map(|segment| segment.len().min(width.saturating_sub(1)))
        .unwrap_or(0);
    (last_row, last_col)
}

fn row_occupied_columns(cells: &[TerminalCell]) -> usize {
    cells
        .iter()
        .rposition(|cell| !cell.is_blank())
        .map_or(0, |index| index + 1)
}

fn reflow_break(cells: &[TerminalCell], start: usize, width: usize) -> usize {
    if start >= cells.len() {
        return start;
    }

    let mut end = (start + width.max(1)).min(cells.len());
    if end < cells.len() && cells[end].is_wide_continuation() {
        end = end.saturating_sub(1);
        if end > start && cells[end - 1].wide {
            end = end.saturating_sub(1);
        }
    }
    end.max(start + 1).min(cells.len())
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
    for (col, tab_stop) in tab_stops.iter_mut().enumerate() {
        *tab_stop = is_default_tab_stop(col);
    }
    tab_stops
}

fn is_default_tab_stop(col: usize) -> bool {
    col != 0 && col.is_multiple_of(8)
}

#[cfg(test)]
mod tests {
    use super::{MouseTrackingMode, TerminalColor, TerminalState};

    fn write_line(terminal: &mut TerminalState, text: &str) {
        for ch in text.chars() {
            terminal.print(ch);
        }
        terminal.carriage_return();
        terminal.linefeed();
    }

    #[test]
    fn repeated_mixed_resizes_preserve_history() {
        let mut terminal = TerminalState::new(3, 10, 64);
        for line in ["alpha bravo", "charlie delta", "echo foxtrot", "golf hotel"] {
            write_line(&mut terminal, line);
        }

        for (rows, cols) in [(4, 6), (2, 12), (5, 5), (3, 10), (1, 1), (4, 9), (3, 10)] {
            terminal.resize(rows, cols);
        }

        let all = terminal.contents_between_stable(0, 0, terminal.viewport_top() + 6, 10);
        assert!(all.contains("alpha"));
        assert!(all.contains("golf"));
    }

    #[test]
    fn shell_style_prompt_redraw_during_resize_does_not_duplicate_prompt() {
        let mut terminal = TerminalState::new(4, 16, 64);
        write_line(&mut terminal, "old output");
        for ch in "(base) kisaczka@Mac ~ % ".chars() {
            terminal.print(ch);
        }

        terminal.resize(4, 10);
        terminal.cursor_up(2);
        terminal.carriage_return();
        terminal.erase_in_display(0);
        for ch in "(base) kisaczka@Mac ~ % ".chars() {
            terminal.print(ch);
        }
        terminal.resize(4, 20);

        let text = terminal.contents_between_stable(
            terminal.viewport_top(),
            0,
            terminal.viewport_top() + 3,
            20,
        );
        assert_eq!(text.matches("(base) kisaczka@Mac ~ %").count(), 1);
    }

    #[test]
    fn widen_after_narrow_restores_hidden_columns() {
        let mut terminal = TerminalState::new(2, 12, 16);
        for ch in "abcdefghijkl".chars() {
            terminal.print(ch);
        }

        terminal.resize(2, 5);
        terminal.resize(2, 12);

        assert_eq!(
            terminal.visible_row(0).expect("row").text_range(0, 12),
            "abcdefghijkl"
        );
    }

    #[test]
    fn viewport_scrollback_survives_resize() {
        let mut terminal = TerminalState::new(2, 8, 32);
        for line in ["one", "two", "three", "four", "five"] {
            write_line(&mut terminal, line);
        }

        terminal.resize(3, 5);
        assert!(terminal.scroll_viewport_up(2));
        let top = terminal.viewport_top();
        assert_eq!(
            terminal
                .line_for_stable_row(top)
                .expect("row")
                .text_range(0, 3),
            "two"
        );
    }

    #[test]
    fn wide_chars_rewrap_cleanly_near_wrap_boundary() {
        let mut terminal = TerminalState::new(3, 4, 16);
        for ch in ['a', 'b', '界', 'c', 'd'] {
            terminal.print(ch);
        }

        terminal.resize(3, 3);
        terminal.resize(3, 4);

        let first = terminal.visible_row(0).expect("row").text_range(0, 4);
        let second = terminal.visible_row(1).expect("row").text_range(0, 4);
        assert_eq!(first, "ab界");
        assert_eq!(second.trim_end(), "cd");
    }

    #[test]
    fn alternate_screen_enter_exit_with_resize_keeps_primary_history() {
        let mut terminal = TerminalState::new(2, 8, 32);
        for line in ["one", "two", "three"] {
            write_line(&mut terminal, line);
        }

        terminal.set_private_mode(1049, true);
        terminal.resize(3, 6);
        for ch in "alt".chars() {
            terminal.print(ch);
        }
        terminal.set_private_mode(1049, false);
        terminal.resize(2, 8);

        let text = terminal.contents_between_stable(0, 0, 3, 8);
        assert!(text.contains("one"));
        assert!(text.contains("three"));
    }

    #[test]
    fn alternate_screen_resize_does_not_rewrap_like_primary() {
        let mut terminal = TerminalState::new(2, 6, 32);
        terminal.set_private_mode(1049, true);
        for ch in "abcdef".chars() {
            terminal.print(ch);
        }

        terminal.resize(2, 3);

        assert_eq!(
            terminal.visible_row(0).expect("row").text_range(0, 3),
            "abc"
        );
        assert_eq!(
            terminal.visible_row(1).expect("row").text_range(0, 3),
            "   "
        );
    }

    #[test]
    fn resize_redraw_stress_keeps_prompt_singleton() {
        let mut terminal = TerminalState::new(6, 24, 4096);
        write_line(&mut terminal, "output 1");
        write_line(&mut terminal, "output 2");

        let prompt = "(base) kisaczka@Mac ~ % ";
        fn move_to_wrapped_line_start(terminal: &mut TerminalState) {
            let (cursor_row, _) = terminal.cursor_position();
            let mut steps = 0u16;
            while steps < cursor_row {
                let previous = cursor_row - steps - 1;
                let Some(row) = terminal.visible_row(previous) else {
                    break;
                };
                if !row.wrapped() {
                    break;
                }
                steps += 1;
            }
            if steps > 0 {
                terminal.cursor_up(steps);
            }
            terminal.carriage_return();
        }

        for _ in 0..20 {
            for ch in prompt.chars() {
                terminal.print(ch);
            }
            for (rows, cols) in [(6, 9), (5, 7), (4, 5), (3, 3), (2, 2), (1, 1), (6, 24)] {
                terminal.resize(rows, cols);
                move_to_wrapped_line_start(&mut terminal);
                terminal.erase_in_display(0);
                for ch in prompt.chars() {
                    terminal.print(ch);
                }
            }
        }

        let history = terminal.contents_between_stable(0, 0, terminal.viewport_top() + 32, 24);
        let visible = terminal.contents_between(0, 0, terminal.size().0.saturating_sub(1), 24);
        assert!(history.contains("output 1"));
        assert!(history.contains("output 2"));
        assert_eq!(visible.matches("(base) kisaczka@Mac ~ %").count(), 1);
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

    #[test]
    fn private_modes_toggle_bracketed_paste_and_focus_reporting() {
        let mut terminal = TerminalState::new(2, 8, 16);

        terminal.set_private_mode(2004, true);
        terminal.set_private_mode(1004, true);

        assert!(terminal.bracketed_paste());
        assert!(terminal.focus_reporting());

        terminal.set_private_mode(2004, false);
        terminal.set_private_mode(1004, false);

        assert!(!terminal.bracketed_paste());
        assert!(!terminal.focus_reporting());
    }

    #[test]
    fn mouse_tracking_prefers_highest_enabled_mode() {
        let mut terminal = TerminalState::new(2, 8, 16);

        terminal.set_private_mode(1000, true);
        assert_eq!(terminal.mouse_tracking_mode(), MouseTrackingMode::Normal);

        terminal.set_private_mode(1002, true);
        assert_eq!(
            terminal.mouse_tracking_mode(),
            MouseTrackingMode::ButtonMotion
        );

        terminal.set_private_mode(1003, true);
        assert_eq!(terminal.mouse_tracking_mode(), MouseTrackingMode::AnyMotion);
    }
}
