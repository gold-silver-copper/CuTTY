use std::collections::VecDeque;

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

    fn blank(seqno: u64) -> Self {
        Self {
            cells: Vec::new(),
            wrapped: false,
            seqno,
        }
    }

    fn touch(&mut self, seqno: u64) {
        self.seqno = seqno;
    }

    fn clear(&mut self, seqno: u64) {
        self.cells.clear();
        self.wrapped = false;
        self.touch(seqno);
    }

    fn is_blank_line(&self) -> bool {
        !self.wrapped && self.cells.iter().all(TerminalCell::is_blank)
    }

    fn from_cells(cells: Vec<TerminalCell>, wrapped: bool, seqno: u64) -> Self {
        let mut row = Self {
            cells,
            wrapped,
            seqno,
        };
        if !wrapped {
            row.trim_trailing_blanks();
        }
        row
    }

    fn truncate_visible(&mut self, cols: u16, seqno: u64) {
        self.cells.truncate(cols as usize);
        self.trim_trailing_blanks();
        self.touch(seqno);
    }

    fn reflow_columns(&self, cols: u16) -> usize {
        if self.wrapped {
            self.cells.len().max(cols as usize)
        } else {
            self.cells.len()
        }
    }

    fn set_cell(&mut self, col: usize, cell: TerminalCell, seqno: u64) {
        if self.cells.len() <= col {
            self.cells.resize(col + 1, TerminalCell::default());
        }
        self.cells[col] = cell;
        self.touch(seqno);
    }

    fn clear_cell(&mut self, col: usize, seqno: u64) {
        if col < self.cells.len() {
            self.cells[col] = TerminalCell::default();
            self.touch(seqno);
        }
    }

    fn clear_overwrite(&mut self, col: usize, seqno: u64) {
        if let Some(cell) = self.cells.get(col) {
            if cell.wide_continuation && col > 0 {
                self.clear_cell(col - 1, seqno);
            } else if cell.wide {
                self.clear_cell(col + 1, seqno);
            }
        }
        self.clear_cell(col, seqno);
    }

    fn append_to_previous(&mut self, col: usize, c: char, seqno: u64) {
        if let Some(target) = col.checked_sub(1).and_then(|idx| self.cells.get_mut(idx)) {
            target.contents.push(c);
            self.touch(seqno);
        }
    }

    fn clear_range(&mut self, start: usize, end: usize, seqno: u64) {
        let end = end.min(self.cells.len());
        for idx in start.min(end)..end {
            self.cells[idx] = TerminalCell::default();
        }
        self.trim_trailing_blanks();
        self.touch(seqno);
    }

    fn shift_right(&mut self, start: usize, count: usize, width: usize, seqno: u64) {
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
        self.touch(seqno);
    }

    fn shift_left(&mut self, start: usize, count: usize, width: usize, seqno: u64) {
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
        self.touch(seqno);
    }

    fn set_wrapped(&mut self, wrapped: bool, seqno: u64) {
        self.wrapped = wrapped;
        self.touch(seqno);
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct LogicalLine {
    cells: Vec<TerminalCell>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LogicalPosition {
    line: usize,
    col: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PhysicalPosition {
    row: usize,
    col: usize,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScreenBuffer {
    rows: VecDeque<BufferRow>,
    physical_rows: u16,
    stable_row_offset: usize,
    viewport_top: Option<StableRowIndex>,
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
            rows: VecDeque::with_capacity(rows as usize + scrollback_limit.min(256)),
            physical_rows: rows,
            stable_row_offset: 0,
            viewport_top: None,
            cursor: CursorState::default(),
            saved_cursor: SavedCursor::default(),
            attrs: CellAttributes::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            pending_wrap: false,
        };
        buffer.ensure_physical_rows();
        buffer
    }

    fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        (row < self.physical_rows)
            .then(|| self.rows.get(self.phys_row(row)))
            .flatten()
    }

    fn row_mut(&mut self, row: u16) -> &mut BufferRow {
        let phys = self.phys_row(row);
        self.rows.get_mut(phys).expect("visible row in range")
    }

    fn resize_rows(&mut self, rows: u16, scrollback_limit: usize, seqno: u64) {
        let current_rows = self.physical_rows;
        let visible_start = self.visible_start();

        if rows > current_rows {
            let reveal = (rows - current_rows).min(visible_start as u16);
            self.cursor.row = (self.cursor.row + reveal).min(rows.saturating_sub(1));
            self.saved_cursor.cursor.row =
                (self.saved_cursor.cursor.row + reveal).min(rows.saturating_sub(1));
        } else if rows < current_rows {
            let hide = current_rows - rows;
            self.cursor.row = self.cursor.row.saturating_sub(hide);
            self.saved_cursor.cursor.row = self.saved_cursor.cursor.row.saturating_sub(hide);
        }

        self.physical_rows = rows;
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.ensure_physical_rows();
        self.trim_to_capacity(scrollback_limit);
        for row in &mut self.rows {
            row.touch(seqno);
        }
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.saved_cursor.cursor.row = self.saved_cursor.cursor.row.min(rows.saturating_sub(1));
        self.pending_wrap = false;
    }

    fn reflow(&mut self, old_cols: u16, rows: u16, cols: u16, scrollback_limit: usize, seqno: u64) {
        self.prune_trailing_blank_rows();
        let snapshot = collect_reflow_snapshot(self, old_cols);
        let reflowed = reflow_snapshot(&snapshot, cols, seqno);
        self.rebuild_from_reflow(reflowed, rows, cols, scrollback_limit);
    }

    fn reset(&mut self, rows: u16, scrollback_limit: usize, seqno: u64) {
        self.rows = VecDeque::with_capacity(rows as usize + scrollback_limit.min(256));
        self.physical_rows = rows;
        self.stable_row_offset = 0;
        self.viewport_top = None;
        self.cursor = CursorState::default();
        self.saved_cursor = SavedCursor::default();
        self.attrs = CellAttributes::default();
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.pending_wrap = false;
        self.ensure_physical_rows();
        for row in &mut self.rows {
            row.touch(seqno);
        }
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

    fn clear_scrollback(&mut self) {
        let visible_start = self.visible_start();
        for _ in 0..visible_start {
            self.rows.pop_front();
            self.stable_row_offset += 1;
        }
        self.ensure_physical_rows();
    }

    fn viewport_top(&self) -> StableRowIndex {
        self.viewport_top
            .unwrap_or_else(|| self.stable_row_offset + self.bottom_visible_start())
    }

    fn max_viewport_top(&self) -> StableRowIndex {
        self.stable_row_offset + self.bottom_visible_start()
    }

    fn visible_row_to_stable_row(&self, row: u16) -> StableRowIndex {
        self.stable_row_offset + self.visible_start() + row as usize
    }

    fn stable_row_to_visible_row(&self, stable_row: StableRowIndex) -> Option<u16> {
        let top = self.viewport_top();
        let bottom = top + self.physical_rows as usize;
        (stable_row >= top && stable_row < bottom).then_some((stable_row - top) as u16)
    }

    fn set_viewport_top(&mut self, viewport_top: Option<StableRowIndex>) -> bool {
        let previous = self.viewport_top();
        let max_top = self.max_viewport_top();
        let clamped = viewport_top
            .unwrap_or(max_top)
            .clamp(self.stable_row_offset, max_top);
        self.viewport_top = (clamped != max_top).then_some(clamped);
        self.viewport_top() != previous
    }

    fn scroll_viewport_up(&mut self, rows: u16) -> bool {
        let next_top = self.viewport_top().saturating_sub(rows as usize);
        self.set_viewport_top(Some(next_top))
    }

    fn scroll_viewport_down(&mut self, rows: u16) -> bool {
        let next_top = self.viewport_top().saturating_add(rows as usize);
        self.set_viewport_top(Some(next_top))
    }

    fn visible_line_info(&self, row: u16) -> Option<VisibleLineInfo> {
        self.visible_row(row).map(|line| VisibleLineInfo {
            stable_row: self.visible_row_to_stable_row(row),
            seqno: line.seqno(),
        })
    }

    fn line_for_stable_row(&self, stable_row: StableRowIndex) -> Option<&BufferRow> {
        stable_row
            .checked_sub(self.stable_row_offset)
            .and_then(|row| self.rows.get(row))
    }

    fn bottom_visible_start(&self) -> usize {
        self.rows.len().saturating_sub(self.physical_rows as usize)
    }

    fn visible_start(&self) -> usize {
        let bottom = self.bottom_visible_start();
        match self.viewport_top {
            Some(top) => top
                .clamp(self.stable_row_offset, self.stable_row_offset + bottom)
                .saturating_sub(self.stable_row_offset),
            None => bottom,
        }
    }

    fn phys_row(&self, row: u16) -> usize {
        self.visible_start() + row as usize
    }

    fn ensure_physical_rows(&mut self) {
        while self.rows.len() < self.physical_rows as usize {
            self.rows.push_back(BufferRow::blank(0));
        }
    }

    fn trim_to_capacity(&mut self, scrollback_limit: usize) {
        let capacity = self.physical_rows as usize + scrollback_limit;
        while self.rows.len() > capacity.max(self.physical_rows as usize) {
            self.rows.pop_front();
            self.stable_row_offset += 1;
        }
        self.ensure_physical_rows();
    }

    fn scroll_up_region(&mut self, rows: u16, scrollback_limit: usize, count: u16, seqno: u64) {
        if rows == 0 {
            return;
        }

        let top = self.scroll_top.min(rows.saturating_sub(1));
        let bottom = self.scroll_bottom.min(rows.saturating_sub(1));
        let full_screen = top == 0 && bottom + 1 == rows;
        for _ in 0..count {
            if full_screen && scrollback_limit > 0 {
                self.rows.push_back(BufferRow::blank(seqno));
                self.trim_to_capacity(scrollback_limit);
                continue;
            }

            let top_phys = self.phys_row(top);
            let bottom_phys = self.phys_row(bottom);
            if top_phys < self.rows.len() {
                self.rows.remove(top_phys);
                self.rows.insert(bottom_phys, BufferRow::blank(seqno));
            }
        }
    }

    fn scroll_down_region(&mut self, rows: u16, count: u16, seqno: u64) {
        if rows == 0 {
            return;
        }

        let top = self.scroll_top.min(rows.saturating_sub(1));
        let bottom = self.scroll_bottom.min(rows.saturating_sub(1));
        for _ in 0..count {
            let top_phys = self.phys_row(top);
            let bottom_phys = self.phys_row(bottom);
            if bottom_phys < self.rows.len() {
                self.rows.remove(bottom_phys);
                self.rows.insert(top_phys, BufferRow::blank(seqno));
            }
        }
    }

    fn rebuild_from_reflow(
        &mut self,
        reflowed: ReflowedBuffer,
        rows: u16,
        cols: u16,
        scrollback_limit: usize,
    ) {
        let mut rows_deque: VecDeque<BufferRow> = reflowed.rows.into_iter().collect();
        let capacity = rows as usize + scrollback_limit;
        let dropped = rows_deque.len().saturating_sub(capacity.max(rows as usize));
        for _ in 0..dropped {
            rows_deque.pop_front();
        }

        let max_row = rows.saturating_sub(1) as usize;
        let max_col = cols.saturating_sub(1) as usize;
        self.rows = rows_deque;
        self.physical_rows = rows;
        self.stable_row_offset += dropped;
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.pending_wrap = false;
        self.ensure_physical_rows();

        let visible_start = self.visible_start();
        let cursor_phys = reflowed.cursor.row.saturating_sub(dropped);
        let saved_cursor_phys = reflowed.saved_cursor.row.saturating_sub(dropped);
        self.cursor.row = cursor_phys.saturating_sub(visible_start).min(max_row) as u16;
        self.cursor.col = reflowed.cursor.col.min(max_col) as u16;
        self.saved_cursor.cursor.row =
            saved_cursor_phys.saturating_sub(visible_start).min(max_row) as u16;
        self.saved_cursor.cursor.col = reflowed.saved_cursor.col.min(max_col) as u16;
    }

    fn prune_trailing_blank_rows(&mut self) {
        let cursor_phys = self.phys_row(self.cursor.row);
        while self.rows.len() > cursor_phys + 1
            && self.rows.back().is_some_and(BufferRow::is_blank_line)
        {
            self.rows.pop_back();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ReflowSnapshot {
    lines: Vec<LogicalLine>,
    cursor: LogicalPosition,
    saved_cursor: LogicalPosition,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ReflowedBuffer {
    rows: Vec<BufferRow>,
    cursor: PhysicalPosition,
    saved_cursor: PhysicalPosition,
}

fn collect_reflow_snapshot(buffer: &ScreenBuffer, cols: u16) -> ReflowSnapshot {
    let cursor_row = buffer.visible_start() + buffer.cursor.row as usize;
    let saved_cursor_row = buffer.visible_start() + buffer.saved_cursor.cursor.row as usize;
    let mut snapshot = ReflowSnapshot::default();
    let mut current_line = LogicalLine::default();
    let mut current_cursor = None;
    let mut current_saved_cursor = None;

    for (row_index, row) in buffer.rows.iter().enumerate() {
        let line_offset = current_line.cells.len();
        let mut logical_columns = row.reflow_columns(cols);
        if row_index == cursor_row {
            logical_columns = logical_columns.max(buffer.cursor.col as usize);
            current_cursor = Some(line_offset + buffer.cursor.col as usize);
        }
        if row_index == saved_cursor_row {
            logical_columns = logical_columns.max(buffer.saved_cursor.cursor.col as usize);
            current_saved_cursor = Some(line_offset + buffer.saved_cursor.cursor.col as usize);
        }

        current_line.cells.extend(row.cells.iter().cloned());
        current_line
            .cells
            .resize(line_offset + logical_columns, TerminalCell::default());

        if !row.wrapped() {
            finalize_logical_line(
                &mut snapshot,
                &mut current_line,
                &mut current_cursor,
                &mut current_saved_cursor,
            );
        }
    }

    if !current_line.cells.is_empty() || snapshot.lines.is_empty() {
        finalize_logical_line(
            &mut snapshot,
            &mut current_line,
            &mut current_cursor,
            &mut current_saved_cursor,
        );
    }

    snapshot
}

fn finalize_logical_line(
    snapshot: &mut ReflowSnapshot,
    current_line: &mut LogicalLine,
    current_cursor: &mut Option<usize>,
    current_saved_cursor: &mut Option<usize>,
) {
    let line_index = snapshot.lines.len();
    if let Some(col) = current_cursor.take() {
        snapshot.cursor = LogicalPosition {
            line: line_index,
            col,
        };
    }
    if let Some(col) = current_saved_cursor.take() {
        snapshot.saved_cursor = LogicalPosition {
            line: line_index,
            col,
        };
    }
    snapshot.lines.push(std::mem::take(current_line));
}

fn reflow_snapshot(snapshot: &ReflowSnapshot, cols: u16, seqno: u64) -> ReflowedBuffer {
    let width = cols.max(1) as usize;
    let mut reflowed = ReflowedBuffer::default();

    for (line_index, line) in snapshot.lines.iter().enumerate() {
        if line.cells.is_empty() {
            let row_index = reflowed.rows.len();
            reflowed.rows.push(BufferRow::blank(seqno));
            if snapshot.cursor.line == line_index {
                reflowed.cursor = PhysicalPosition {
                    row: row_index,
                    col: 0,
                };
            }
            if snapshot.saved_cursor.line == line_index {
                reflowed.saved_cursor = PhysicalPosition {
                    row: row_index,
                    col: 0,
                };
            }
            continue;
        }

        let mut start = 0;
        while start < line.cells.len() {
            let end = reflow_break(&line.cells, start, width);
            let wrapped = end < line.cells.len();
            let mut cells = line.cells[start..end].to_vec();
            if wrapped && cells.len() < width {
                cells.resize(width, TerminalCell::default());
            }

            let row_index = reflowed.rows.len();
            if logical_position_maps_to_row(
                snapshot.cursor,
                line_index,
                start,
                end,
                line.cells.len(),
            ) {
                reflowed.cursor = PhysicalPosition {
                    row: row_index,
                    col: snapshot.cursor.col.saturating_sub(start),
                };
            }
            if logical_position_maps_to_row(
                snapshot.saved_cursor,
                line_index,
                start,
                end,
                line.cells.len(),
            ) {
                reflowed.saved_cursor = PhysicalPosition {
                    row: row_index,
                    col: snapshot.saved_cursor.col.saturating_sub(start),
                };
            }

            reflowed
                .rows
                .push(BufferRow::from_cells(cells, wrapped, seqno));
            start = end;
        }
    }

    reflowed
}

fn reflow_break(cells: &[TerminalCell], start: usize, width: usize) -> usize {
    if start >= cells.len() {
        return start;
    }

    let mut end = (start + width.max(1)).min(cells.len());
    if end < cells.len() && cells[end].is_wide_continuation() {
        end = end.saturating_sub(1);
        if end == start {
            end = (start + 2).min(cells.len());
        }
    }
    end.max(start + 1).min(cells.len())
}

fn logical_position_maps_to_row(
    position: LogicalPosition,
    line_index: usize,
    start: usize,
    end: usize,
    total: usize,
) -> bool {
    position.line == line_index
        && position.col >= start
        && (position.col < end || (end == total && position.col == end))
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
        Self {
            rows,
            cols,
            scrollback_limit,
            next_seqno: 1,
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

    pub fn viewport_top(&self) -> StableRowIndex {
        self.active_buffer().viewport_top()
    }

    pub fn follow_viewport_bottom(&mut self) -> bool {
        self.active_buffer_mut().set_viewport_top(None)
    }

    pub fn scroll_viewport_up(&mut self, rows: u16) -> bool {
        self.active_buffer_mut().scroll_viewport_up(rows.max(1))
    }

    pub fn scroll_viewport_down(&mut self, rows: u16) -> bool {
        self.active_buffer_mut().scroll_viewport_down(rows.max(1))
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
        if rows == self.rows && cols == self.cols {
            return;
        }

        let seqno = self.bump_seqno();
        let old_cols = self.cols;
        if cols != old_cols {
            self.primary
                .reflow(old_cols, rows, cols, self.scrollback_limit, seqno);
            self.alternate.reflow(old_cols, rows, cols, 0, seqno);
        } else {
            self.primary.resize_rows(rows, self.scrollback_limit, seqno);
            self.alternate.resize_rows(rows, 0, seqno);
        }
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
        if self.rows == 0 {
            return String::new();
        }

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
        let seqno = self.bump_seqno();
        let width = UnicodeWidthChar::width(c).unwrap_or(1);
        if width == 0 {
            let row = self.active_buffer().cursor.row;
            let col = self.active_buffer().cursor.col as usize;
            self.prepare_row_for_write(row, seqno);
            self.row_mut(row).append_to_previous(col, c, seqno);
            return;
        }
        if self.cols == 0 || width > self.cols as usize {
            return;
        }

        if self.active_buffer().pending_wrap && self.modes.wraparound {
            self.wrap_to_next_line(seqno);
        }

        if self.active_buffer().cursor.col as usize + width > self.cols as usize {
            self.wrap_to_next_line(seqno);
        }

        let row = self.active_buffer().cursor.row;
        let col = self.active_buffer().cursor.col as usize;
        let attrs = self.active_buffer().attrs;
        self.prepare_row_for_write(row, seqno);
        let row_mut = self.row_mut(row);
        row_mut.clear_overwrite(col, seqno);
        if width == 2 {
            row_mut.clear_overwrite(col + 1, seqno);
        }
        row_mut.set_cell(
            col,
            TerminalCell::new(c.to_string(), attrs, width == 2),
            seqno,
        );
        if width == 2 {
            row_mut.set_cell(col + 1, TerminalCell::continuation(attrs), seqno);
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
        let seqno = self.bump_seqno();
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        if cursor_row == scroll_bottom {
            buffer.scroll_up_region(rows, scrollback_limit, 1, seqno);
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
        let seqno = self.bump_seqno();
        self.primary.reset(self.rows, self.scrollback_limit, seqno);
        self.alternate.reset(self.rows, 0, seqno);
        self.modes = TerminalModes::default();
        self.tab_stops = default_tab_stops(self.cols);
    }

    pub fn reverse_index(&mut self) {
        let rows = self.rows;
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_top = self.active_buffer().scroll_top;
        let seqno = self.bump_seqno();
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        if cursor_row == scroll_top {
            buffer.scroll_down_region(rows, 1, seqno);
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
        let seqno = self.bump_seqno();
        self.active_buffer_mut().pending_wrap = false;
        match mode {
            3 => self.active_buffer_mut().clear_scrollback(),
            1 => {
                let cursor_row = self.active_buffer().cursor.row;
                for row in 0..cursor_row {
                    self.row_mut(row).clear(seqno);
                }
                let cursor_col = self.active_buffer().cursor.col as usize + 1;
                self.prepare_row_for_write(cursor_row, seqno);
                self.row_mut(cursor_row).clear_range(0, cursor_col, seqno);
            }
            2 => {
                for row in 0..self.rows {
                    self.row_mut(row).clear(seqno);
                }
            }
            _ => {
                let cursor_row = self.active_buffer().cursor.row;
                let cursor_col = self.active_buffer().cursor.col as usize;
                self.prepare_row_for_write(cursor_row, seqno);
                self.row_mut(cursor_row)
                    .clear_range(cursor_col, usize::MAX, seqno);
                for row in cursor_row + 1..self.rows {
                    self.row_mut(row).clear(seqno);
                }
            }
        }
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        let seqno = self.bump_seqno();
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        self.prepare_row_for_write(cursor_row, seqno);
        let row = self.row_mut(cursor_row);
        match mode {
            1 => row.clear_range(0, cursor_col + 1, seqno),
            2 => row.clear(seqno),
            _ => row.clear_range(cursor_col, usize::MAX, seqno),
        }
    }

    pub fn insert_blank_chars(&mut self, count: u16) {
        let seqno = self.bump_seqno();
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        let cols = self.cols as usize;
        self.prepare_row_for_write(cursor_row, seqno);
        self.row_mut(cursor_row)
            .shift_right(cursor_col, count.max(1) as usize, cols, seqno);
    }

    pub fn delete_chars(&mut self, count: u16) {
        let seqno = self.bump_seqno();
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        let cols = self.cols as usize;
        self.prepare_row_for_write(cursor_row, seqno);
        self.row_mut(cursor_row)
            .shift_left(cursor_col, count.max(1) as usize, cols, seqno);
    }

    pub fn erase_chars(&mut self, count: u16) {
        let seqno = self.bump_seqno();
        let cursor_row = self.active_buffer().cursor.row;
        let cursor_col = self.active_buffer().cursor.col as usize;
        self.active_buffer_mut().pending_wrap = false;
        self.prepare_row_for_write(cursor_row, seqno);
        self.row_mut(cursor_row)
            .clear_range(cursor_col, cursor_col + count.max(1) as usize, seqno);
    }

    pub fn insert_lines(&mut self, count: u16) {
        let seqno = self.bump_seqno();
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_top = self.active_buffer().scroll_top;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        self.active_buffer_mut().pending_wrap = false;
        if cursor_row < scroll_top || cursor_row > scroll_bottom {
            return;
        }
        let row = self.active_buffer().phys_row(cursor_row);
        let bottom = self.active_buffer().phys_row(scroll_bottom);
        for _ in 0..count.max(1) {
            let buffer = self.active_buffer_mut();
            buffer.rows.insert(row, BufferRow::blank(seqno));
            buffer.rows.remove(bottom + 1);
        }
    }

    pub fn delete_lines(&mut self, count: u16) {
        let seqno = self.bump_seqno();
        let cursor_row = self.active_buffer().cursor.row;
        let scroll_top = self.active_buffer().scroll_top;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        self.active_buffer_mut().pending_wrap = false;
        if cursor_row < scroll_top || cursor_row > scroll_bottom {
            return;
        }
        let row = self.active_buffer().phys_row(cursor_row);
        let bottom = self.active_buffer().phys_row(scroll_bottom);
        for _ in 0..count.max(1) {
            let buffer = self.active_buffer_mut();
            buffer.rows.remove(row);
            buffer.rows.insert(bottom, BufferRow::blank(seqno));
        }
    }

    pub fn scroll_up(&mut self, count: u16) {
        let rows = self.rows;
        let scrollback_limit = self.active_scrollback_limit();
        let seqno = self.bump_seqno();
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.scroll_up_region(rows, scrollback_limit, count.max(1), seqno);
    }

    pub fn scroll_down(&mut self, count: u16) {
        let rows = self.rows;
        let seqno = self.bump_seqno();
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.scroll_down_region(rows, count.max(1), seqno);
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

    fn wrap_to_next_line(&mut self, seqno: u64) {
        let rows = self.rows;
        let scrollback_limit = self.active_scrollback_limit();
        let row = self.active_buffer().cursor.row;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        self.row_mut(row).set_wrapped(true, seqno);
        let buffer = self.active_buffer_mut();
        buffer.pending_wrap = false;
        buffer.cursor.col = 0;
        if row == scroll_bottom {
            buffer.scroll_up_region(rows, scrollback_limit, 1, seqno);
        } else {
            buffer.cursor.row = (row + 1).min(rows.saturating_sub(1));
        }
    }

    fn prepare_row_for_write(&mut self, row: u16, seqno: u64) {
        let cols = self.cols;
        self.row_mut(row).truncate_visible(cols, seqno);
        self.row_mut(row).set_wrapped(false, seqno);
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
            let seqno = self.bump_seqno();
            self.alternate.reset(self.rows, 0, seqno);
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
    fn resizing_narrower_reflows_logical_lines() {
        let mut terminal = TerminalState::new(4, 12, 16);
        for ch in "abcdefghijkl".chars() {
            terminal.print(ch);
        }

        terminal.resize(4, 6);

        assert_eq!(
            terminal.visible_row(0).expect("row").text_range(0, 6),
            "abcdef"
        );
        assert!(terminal.visible_row(0).expect("row").wrapped());
        assert_eq!(
            terminal.visible_row(1).expect("row").text_range(0, 6),
            "ghijkl"
        );
        assert!(!terminal.visible_row(1).expect("row").wrapped());
    }

    #[test]
    fn resizing_keeps_cursor_on_same_logical_column() {
        let mut terminal = TerminalState::new(4, 12, 16);
        for ch in "abcdefghijklmnop".chars() {
            terminal.print(ch);
        }

        assert_eq!(terminal.cursor_position(), (1, 4));

        terminal.resize(4, 6);
        assert_eq!(terminal.cursor_position(), (2, 4));

        terminal.resize(4, 12);
        assert_eq!(terminal.cursor_position(), (1, 4));
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
    fn scrolling_viewport_up_reveals_scrollback_without_mutating_contents() {
        let mut terminal = TerminalState::new(2, 8, 16);
        for line in ["1", "2", "3", "4"] {
            for ch in line.chars() {
                terminal.print(ch);
            }
            terminal.carriage_return();
            terminal.linefeed();
        }

        assert_eq!(terminal.visible_row(0).expect("row").text_range(0, 1), "4");
        assert!(terminal.scroll_viewport_up(1));
        assert_eq!(terminal.visible_row(0).expect("row").text_range(0, 1), "3");
        assert_eq!(terminal.visible_row(1).expect("row").text_range(0, 1), "4");
        assert_eq!(
            terminal
                .line_for_stable_row(0)
                .expect("row")
                .text_range(0, 1),
            "1"
        );
        assert_eq!(
            terminal
                .line_for_stable_row(1)
                .expect("row")
                .text_range(0, 1),
            "2"
        );
        assert_eq!(
            terminal
                .line_for_stable_row(2)
                .expect("row")
                .text_range(0, 1),
            "3"
        );
        assert_eq!(
            terminal
                .line_for_stable_row(3)
                .expect("row")
                .text_range(0, 1),
            "4"
        );
    }

    #[test]
    fn scrolling_viewport_down_returns_to_bottom_follow_mode() {
        let mut terminal = TerminalState::new(2, 8, 16);
        for line in ["1", "2", "3", "4"] {
            for ch in line.chars() {
                terminal.print(ch);
            }
            terminal.carriage_return();
            terminal.linefeed();
        }

        assert!(terminal.scroll_viewport_up(2));
        let off_bottom_top = terminal.viewport_top();

        assert!(terminal.scroll_viewport_down(1));
        assert_eq!(terminal.viewport_top(), off_bottom_top + 1);
        assert!(terminal.scroll_viewport_down(8));
        assert_eq!(terminal.visible_row(0).expect("row").text_range(0, 1), "4");
        assert!(!terminal.scroll_viewport_down(1));
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

        terminal.set_private_mode(1003, false);
        assert_eq!(
            terminal.mouse_tracking_mode(),
            MouseTrackingMode::ButtonMotion
        );

        terminal.set_private_mode(1002, false);
        assert_eq!(terminal.mouse_tracking_mode(), MouseTrackingMode::Normal);

        terminal.set_private_mode(1000, false);
        assert_eq!(terminal.mouse_tracking_mode(), MouseTrackingMode::Disabled);
    }
}
