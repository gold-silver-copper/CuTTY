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

    fn from_cells(mut cells: Vec<TerminalCell>, wrapped: bool, seqno: u64) -> Self {
        if !wrapped {
            trim_trailing_blank_cells(&mut cells);
        }
        Self {
            cells,
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
struct CursorState {
    row: u16,
    col: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CanonicalCursor {
    line: usize,
    col: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SavedCursor {
    cursor: CanonicalCursor,
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

type StableLineId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineBreakKind {
    Hard,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LogicalLine {
    id: StableLineId,
    cells: Vec<TerminalCell>,
    break_kind: LineBreakKind,
    seqno: u64,
}

impl LogicalLine {
    fn blank(id: StableLineId, seqno: u64) -> Self {
        Self {
            id,
            cells: Vec::new(),
            break_kind: LineBreakKind::Open,
            seqno,
        }
    }

    fn touch(&mut self, seqno: u64) {
        self.seqno = seqno;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ViewportAnchor {
    line_id: StableLineId,
    row_in_line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectedRow {
    stable_row: StableRowIndex,
    line_index: Option<usize>,
    line_id: Option<StableLineId>,
    row_in_line: usize,
    start_col: usize,
    end_col: usize,
    snapshot: BufferRow,
}

impl ProjectedRow {
    fn blank(stable_row: StableRowIndex) -> Self {
        Self {
            stable_row,
            line_index: None,
            line_id: None,
            row_in_line: 0,
            start_col: 0,
            end_col: 0,
            snapshot: BufferRow::blank(0),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Projection {
    rows: Vec<ProjectedRow>,
    live_start: usize,
    viewport_top: usize,
    cursor_live: CursorState,
    saved_cursor_live: CursorState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScreenBuffer {
    lines: VecDeque<LogicalLine>,
    physical_rows: u16,
    stable_row_offset: usize,
    viewport_anchor: Option<ViewportAnchor>,
    cursor: CanonicalCursor,
    saved_cursor: SavedCursor,
    attrs: CellAttributes,
    scroll_top: u16,
    scroll_bottom: u16,
    pending_wrap: bool,
    next_line_id: StableLineId,
    projection: Projection,
}

impl ScreenBuffer {
    fn new(rows: u16, seqno: u64) -> Self {
        let mut buffer = Self {
            lines: VecDeque::new(),
            physical_rows: rows,
            stable_row_offset: 0,
            viewport_anchor: None,
            cursor: CanonicalCursor::default(),
            saved_cursor: SavedCursor::default(),
            attrs: CellAttributes::default(),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            pending_wrap: false,
            next_line_id: 1,
            projection: Projection::default(),
        };
        let id = buffer.alloc_line_id();
        buffer.lines.push_back(LogicalLine::blank(id, seqno));
        buffer
    }

    fn resize_rows(&mut self, rows: u16) {
        self.physical_rows = rows;
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
    }

    fn reset(&mut self, rows: u16, seqno: u64) {
        self.lines.clear();
        self.physical_rows = rows;
        self.stable_row_offset = 0;
        self.viewport_anchor = None;
        self.cursor = CanonicalCursor::default();
        self.saved_cursor = SavedCursor::default();
        self.attrs = CellAttributes::default();
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.pending_wrap = false;
        self.next_line_id = 1;
        let id = self.alloc_line_id();
        self.lines.push_back(LogicalLine::blank(id, seqno));
        self.projection = Projection::default();
    }

    fn save_cursor(&mut self) {
        self.saved_cursor = SavedCursor {
            cursor: self.cursor,
            attrs: self.attrs,
        };
        self.pending_wrap = false;
    }

    fn restore_cursor(&mut self) {
        self.cursor = self.saved_cursor.cursor;
        self.attrs = self.saved_cursor.attrs;
        self.pending_wrap = false;
    }

    fn clear_scrollback(&mut self, cols: u16) {
        self.rebuild_projection(cols);
        let drop_rows = self.projection.live_start;
        if drop_rows == 0 {
            return;
        }

        while self.projection.live_start > 0 && self.lines.len() > 1 {
            let first_rows = projected_row_count_for_line(&self.lines[0], cols);
            self.lines.pop_front();
            self.stable_row_offset += first_rows;
            self.cursor.line = self.cursor.line.saturating_sub(1);
            self.saved_cursor.cursor.line = self.saved_cursor.cursor.line.saturating_sub(1);
            self.rebuild_projection(cols);
            if self.projection.live_start == 0 {
                break;
            }
        }
        self.viewport_anchor = None;
    }

    fn viewport_top(&self) -> StableRowIndex {
        self.stable_row_offset + self.projection.viewport_top
    }

    fn max_viewport_top(&self) -> StableRowIndex {
        self.stable_row_offset + self.projection.live_start
    }

    fn visible_row_to_stable_row(&self, row: u16) -> StableRowIndex {
        self.stable_row_offset + self.projection.viewport_top + row as usize
    }

    fn stable_row_to_visible_row(&self, stable_row: StableRowIndex) -> Option<u16> {
        let top = self.viewport_top();
        let bottom = top + self.physical_rows as usize;
        (stable_row >= top && stable_row < bottom).then_some((stable_row - top) as u16)
    }

    fn set_viewport_top(&mut self, viewport_top: Option<StableRowIndex>) -> bool {
        let previous = self.viewport_top();
        let max_top = self.max_viewport_top();
        let absolute = viewport_top
            .unwrap_or(max_top)
            .clamp(self.stable_row_offset, max_top)
            .saturating_sub(self.stable_row_offset);
        self.projection.viewport_top = absolute;
        self.viewport_anchor = if absolute == self.projection.live_start {
            None
        } else {
            self.anchor_for_absolute_row(absolute)
        };
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

    fn visible_row(&self, row: u16) -> Option<&BufferRow> {
        self.projection
            .rows
            .get(self.projection.viewport_top + row as usize)
            .map(|row| &row.snapshot)
    }

    fn line_for_stable_row(&self, stable_row: StableRowIndex) -> Option<&BufferRow> {
        stable_row
            .checked_sub(self.stable_row_offset)
            .and_then(|row| self.projection.rows.get(row))
            .map(|row| &row.snapshot)
    }

    fn alloc_line_id(&mut self) -> StableLineId {
        let id = self.next_line_id;
        self.next_line_id = self.next_line_id.saturating_add(1);
        id
    }

    fn anchor_for_absolute_row(&self, absolute_row: usize) -> Option<ViewportAnchor> {
        self.projection.rows.get(absolute_row).and_then(|row| {
            row.line_id.map(|line_id| ViewportAnchor {
                line_id,
                row_in_line: row.row_in_line,
            })
        })
    }

    fn rebuild_projection(&mut self, cols: u16) {
        let width = cols.max(1) as usize;
        let mut rows = Vec::new();

        for (line_index, line) in self.lines.iter().enumerate() {
            if line.cells.is_empty() {
                rows.push(ProjectedRow {
                    stable_row: self.stable_row_offset + rows.len(),
                    line_index: Some(line_index),
                    line_id: Some(line.id),
                    row_in_line: 0,
                    start_col: 0,
                    end_col: 0,
                    snapshot: BufferRow::blank(line.seqno),
                });
                continue;
            }

            let mut start = 0;
            let mut row_in_line = 0;
            while start < line.cells.len() {
                let end = reflow_break(&line.cells, start, width);
                let wrapped = end < line.cells.len();
                rows.push(ProjectedRow {
                    stable_row: self.stable_row_offset + rows.len(),
                    line_index: Some(line_index),
                    line_id: Some(line.id),
                    row_in_line,
                    start_col: start,
                    end_col: end,
                    snapshot: BufferRow::from_cells(
                        line.cells[start..end].to_vec(),
                        wrapped,
                        line.seqno,
                    ),
                });
                start = end;
                row_in_line += 1;
            }
        }

        let min_rows = self.physical_rows.max(1) as usize;
        while rows.len() < min_rows {
            rows.push(ProjectedRow::blank(self.stable_row_offset + rows.len()));
        }

        let live_start = rows.len().saturating_sub(self.physical_rows as usize);
        let max_viewport_top = live_start;
        let viewport_top = self
            .viewport_anchor
            .and_then(|anchor| {
                rows.iter().position(|row| {
                    row.line_id == Some(anchor.line_id) && row.row_in_line == anchor.row_in_line
                })
            })
            .unwrap_or(live_start)
            .min(max_viewport_top);

        self.projection = Projection {
            cursor_live: self.locate_cursor_live(&rows, live_start, self.cursor),
            saved_cursor_live: self.locate_cursor_live(&rows, live_start, self.saved_cursor.cursor),
            rows,
            live_start,
            viewport_top,
        };
    }

    fn locate_cursor_live(
        &self,
        rows: &[ProjectedRow],
        live_start: usize,
        cursor: CanonicalCursor,
    ) -> CursorState {
        let Some(line) = self.lines.get(cursor.line) else {
            return CursorState::default();
        };
        let mut candidate = None;
        for (absolute, row) in rows.iter().enumerate() {
            if row.line_index != Some(cursor.line) {
                continue;
            }

            candidate = Some((absolute, row));
            if line.cells.is_empty() && cursor.col == 0 {
                break;
            }

            if cursor.col >= row.start_col
                && (cursor.col < row.end_col
                    || (!row.snapshot.wrapped() && cursor.col <= row.end_col))
            {
                break;
            }
        }

        let Some((absolute, row)) = candidate else {
            return CursorState::default();
        };

        CursorState {
            row: absolute.saturating_sub(live_start) as u16,
            col: cursor.col.saturating_sub(row.start_col) as u16,
        }
    }

    fn ensure_line_exists(&mut self, index: usize, seqno: u64) {
        while self.lines.len() <= index {
            let id = self.alloc_line_id();
            self.lines.push_back(LogicalLine::blank(id, seqno));
        }
    }

    fn ensure_line_extent(&mut self, line_index: usize, col: usize, seqno: u64) {
        self.ensure_line_exists(line_index, seqno);
        let line = self.lines.get_mut(line_index).expect("line exists");
        if line.cells.len() <= col {
            line.cells.resize(col + 1, TerminalCell::default());
        }
        line.touch(seqno);
    }

    fn set_cursor_screen_position(&mut self, cols: u16, row: u16, col: u16, seqno: u64) {
        self.rebuild_projection(cols);
        let target_row = row.min(self.physical_rows.saturating_sub(1));
        let absolute_row = self.projection.live_start + target_row as usize;
        let line_index = self.materialize_absolute_row(cols, absolute_row, seqno);
        self.rebuild_projection(cols);
        let projected = self
            .projection
            .rows
            .get(absolute_row)
            .cloned()
            .expect("materialized row exists");
        let max_col = cols.saturating_sub(1);
        let cursor_col = col.min(max_col) as usize;
        self.cursor = CanonicalCursor {
            line: line_index,
            col: projected.start_col + cursor_col,
        };
        self.ensure_line_extent(self.cursor.line, self.cursor.col, seqno);
        self.pending_wrap = false;
        self.rebuild_projection(cols);
    }

    fn move_cursor_to_row(&mut self, cols: u16, row: u16, seqno: u64) {
        self.rebuild_projection(cols);
        let current_col = self.projection.cursor_live.col;
        self.set_cursor_screen_position(cols, row, current_col, seqno);
    }

    fn move_cursor_to_col(&mut self, cols: u16, col: u16, seqno: u64) {
        self.rebuild_projection(cols);
        let current_row = self.projection.cursor_live.row;
        self.set_cursor_screen_position(cols, current_row, col, seqno);
    }

    fn current_screen_row(&mut self, cols: u16) -> u16 {
        self.rebuild_projection(cols);
        self.projection.cursor_live.row
    }

    fn current_screen_col(&mut self, cols: u16) -> u16 {
        self.rebuild_projection(cols);
        self.projection.cursor_live.col
    }

    fn current_row_end(&mut self, cols: u16) -> usize {
        self.rebuild_projection(cols);
        let absolute = self.projection.live_start + self.projection.cursor_live.row as usize;
        self.projection
            .rows
            .get(absolute)
            .map(|row| row.end_col)
            .unwrap_or(self.cursor.col)
    }

    fn current_row_start(&mut self, cols: u16) -> usize {
        self.rebuild_projection(cols);
        let absolute = self.projection.live_start + self.projection.cursor_live.row as usize;
        self.projection
            .rows
            .get(absolute)
            .map(|row| row.start_col)
            .unwrap_or(0)
    }

    fn materialize_absolute_row(&mut self, cols: u16, absolute_row: usize, seqno: u64) -> usize {
        self.rebuild_projection(cols);
        while absolute_row >= self.projection.rows.len() {
            let id = self.alloc_line_id();
            self.lines.push_back(LogicalLine::blank(id, seqno));
            self.rebuild_projection(cols);
        }

        while self.projection.rows[absolute_row].line_index.is_none() {
            let id = self.alloc_line_id();
            self.lines.push_back(LogicalLine::blank(id, seqno));
            self.rebuild_projection(cols);
        }

        let mut row = self.projection.rows[absolute_row].clone();
        if row.start_col > 0 {
            let line_index = row.line_index.expect("canonical row");
            self.split_line_at(line_index, row.start_col, seqno);
            self.rebuild_projection(cols);
            row = self.projection.rows[absolute_row].clone();
        }

        if row.snapshot.wrapped() {
            let line_index = row.line_index.expect("canonical row");
            self.split_line_at(line_index, row.end_col, seqno);
            self.rebuild_projection(cols);
            row = self.projection.rows[absolute_row].clone();
        }

        row.line_index.expect("standalone row line")
    }

    fn split_line_at(&mut self, line_index: usize, col: usize, seqno: u64) {
        let Some(line_len) = self.lines.get(line_index).map(|line| line.cells.len()) else {
            return;
        };
        if col >= line_len {
            return;
        }

        let tail_id = self.alloc_line_id();
        let line = self.lines.get_mut(line_index).expect("line exists");
        let tail_cells = line.cells.split_off(col);
        let tail_break = line.break_kind;
        line.break_kind = LineBreakKind::Hard;
        line.touch(seqno);

        self.lines.insert(
            line_index + 1,
            LogicalLine {
                id: tail_id,
                cells: tail_cells,
                break_kind: tail_break,
                seqno,
            },
        );

        if self.cursor.line > line_index {
            self.cursor.line += 1;
        } else if self.cursor.line == line_index && self.cursor.col >= col {
            self.cursor.line += 1;
            self.cursor.col -= col;
        }

        if self.saved_cursor.cursor.line > line_index {
            self.saved_cursor.cursor.line += 1;
        } else if self.saved_cursor.cursor.line == line_index && self.saved_cursor.cursor.col >= col
        {
            self.saved_cursor.cursor.line += 1;
            self.saved_cursor.cursor.col -= col;
        }
    }

    fn append_blank_line_after(&mut self, line_index: usize, seqno: u64) -> usize {
        let id = self.alloc_line_id();
        self.lines
            .insert(line_index + 1, LogicalLine::blank(id, seqno));
        if self.cursor.line > line_index {
            self.cursor.line += 1;
        }
        if self.saved_cursor.cursor.line > line_index {
            self.saved_cursor.cursor.line += 1;
        }
        line_index + 1
    }

    fn canonicalize_live_region(&mut self, cols: u16, top: u16, bottom: u16, seqno: u64) {
        if top > bottom {
            return;
        }
        self.rebuild_projection(cols);
        for screen_row in (top..=bottom).rev() {
            let absolute = self.projection.live_start + screen_row as usize;
            self.materialize_absolute_row(cols, absolute, seqno);
        }
        self.rebuild_projection(cols);
    }

    fn trim_scrollback(&mut self, cols: u16, scrollback_limit: usize) {
        if scrollback_limit == 0 {
            while self.lines.len() > self.physical_rows.max(1) as usize && self.cursor.line > 0 {
                let dropped_rows = projected_row_count_for_line(&self.lines[0], cols);
                self.lines.pop_front();
                self.stable_row_offset += dropped_rows;
                self.cursor.line -= 1;
                if self.saved_cursor.cursor.line > 0 {
                    self.saved_cursor.cursor.line -= 1;
                }
            }
            return;
        }

        let capacity = self.physical_rows as usize + scrollback_limit;
        while self.projected_row_count(cols) > capacity.max(self.physical_rows as usize)
            && self.lines.len() > 1
            && self.cursor.line > 0
        {
            let dropped_rows = projected_row_count_for_line(&self.lines[0], cols);
            self.lines.pop_front();
            self.stable_row_offset += dropped_rows;
            self.cursor.line -= 1;
            if self.saved_cursor.cursor.line > 0 {
                self.saved_cursor.cursor.line -= 1;
            }
        }
    }

    fn projected_row_count(&self, cols: u16) -> usize {
        self.lines
            .iter()
            .map(|line| projected_row_count_for_line(line, cols))
            .sum::<usize>()
            .max(1)
    }

    fn ensure_hard_break_after_cursor_row(&mut self, cols: u16, seqno: u64) {
        self.rebuild_projection(cols);
        let absolute = self.projection.live_start + self.projection.cursor_live.row as usize;
        let line_index = self.materialize_absolute_row(cols, absolute, seqno);
        self.rebuild_projection(cols);
        let absolute = self.projection.live_start + self.projection.cursor_live.row as usize;
        let row = self.projection.rows[absolute].clone();
        if row.snapshot.wrapped() {
            self.split_line_at(line_index, row.end_col, seqno);
            self.rebuild_projection(cols);
        }
        let line = self.line_mut(line_index);
        line.break_kind = LineBreakKind::Hard;
        line.touch(seqno);
    }

    fn soft_wrap_cursor(&mut self, cols: u16, seqno: u64) {
        let next_col = self.current_row_end(cols);
        self.ensure_line_extent(self.cursor.line, next_col, seqno);
        self.cursor.col = next_col;
        self.pending_wrap = false;
        self.rebuild_projection(cols);
    }

    fn line_cells_mut(&mut self, line_index: usize) -> &mut Vec<TerminalCell> {
        &mut self.lines.get_mut(line_index).expect("line exists").cells
    }

    fn line_mut(&mut self, line_index: usize) -> &mut LogicalLine {
        self.lines.get_mut(line_index).expect("line exists")
    }

    fn clear_cell(&mut self, line_index: usize, col: usize, seqno: u64) {
        let cells = self.line_cells_mut(line_index);
        if col < cells.len() {
            cells[col] = TerminalCell::default();
            trim_trailing_blank_cells(cells);
            self.line_mut(line_index).touch(seqno);
        }
    }

    fn clear_overwrite(&mut self, line_index: usize, col: usize, seqno: u64) {
        let prior = self
            .lines
            .get(line_index)
            .and_then(|line| line.cells.get(col))
            .cloned();
        if let Some(cell) = prior {
            if cell.wide_continuation && col > 0 {
                self.clear_cell(line_index, col - 1, seqno);
            } else if cell.wide {
                self.clear_cell(line_index, col + 1, seqno);
            }
        }
        self.clear_cell(line_index, col, seqno);
    }

    fn set_cell(&mut self, line_index: usize, col: usize, cell: TerminalCell, seqno: u64) {
        self.ensure_line_extent(line_index, col, seqno);
        let cells = self.line_cells_mut(line_index);
        cells[col] = cell;
        self.line_mut(line_index).touch(seqno);
    }

    fn append_combining(&mut self, line_index: usize, col: usize, c: char, seqno: u64) {
        if let Some(target) = col
            .checked_sub(1)
            .and_then(|prev| self.line_cells_mut(line_index).get_mut(prev))
        {
            target.contents.push(c);
            self.line_mut(line_index).touch(seqno);
        }
    }

    fn clear_range(&mut self, line_index: usize, start: usize, end: usize, seqno: u64) {
        let cells = self.line_cells_mut(line_index);
        let end = end.min(cells.len());
        for cell in cells.iter_mut().take(end).skip(start.min(end)) {
            *cell = TerminalCell::default();
        }
        trim_trailing_blank_cells(cells);
        self.line_mut(line_index).touch(seqno);
    }

    fn shift_right(
        &mut self,
        line_index: usize,
        start: usize,
        count: usize,
        width: usize,
        seqno: u64,
    ) {
        if start >= width || count == 0 {
            return;
        }
        self.ensure_line_extent(line_index, width.saturating_sub(1), seqno);
        let cells = self.line_cells_mut(line_index);
        for idx in (start..width).rev() {
            if idx >= start + count {
                cells[idx] = cells[idx - count].clone();
            } else {
                cells[idx] = TerminalCell::default();
            }
        }
        trim_trailing_blank_cells(cells);
        self.line_mut(line_index).touch(seqno);
    }

    fn shift_left(
        &mut self,
        line_index: usize,
        start: usize,
        count: usize,
        width: usize,
        seqno: u64,
    ) {
        if start >= width || count == 0 {
            return;
        }
        self.ensure_line_extent(line_index, width.saturating_sub(1), seqno);
        let cells = self.line_cells_mut(line_index);
        for idx in start..width {
            let source = idx + count;
            cells[idx] = if source < width {
                cells[source].clone()
            } else {
                TerminalCell::default()
            };
        }
        trim_trailing_blank_cells(cells);
        self.line_mut(line_index).touch(seqno);
    }

    fn scroll_up_region(&mut self, cols: u16, scrollback_limit: usize, count: u16, seqno: u64) {
        if self.physical_rows == 0 {
            return;
        }
        let top = self.scroll_top.min(self.physical_rows.saturating_sub(1));
        let bottom = self.scroll_bottom.min(self.physical_rows.saturating_sub(1));
        if top > bottom {
            return;
        }

        let full_screen = top == 0 && bottom + 1 == self.physical_rows;
        if full_screen {
            self.rebuild_projection(cols);
            let bottom_absolute = self.projection.live_start + bottom as usize;
            let bottom_line = self.materialize_absolute_row(cols, bottom_absolute, seqno);
            for _ in 0..count.max(1) {
                self.append_blank_line_after(bottom_line, seqno);
            }
            self.trim_scrollback(cols, scrollback_limit);
            self.rebuild_projection(cols);
            return;
        }

        self.canonicalize_live_region(cols, top, bottom, seqno);
        for _ in 0..count.max(1) {
            self.rebuild_projection(cols);
            let top_absolute = self.projection.live_start + top as usize;
            let bottom_absolute = self.projection.live_start + bottom as usize;
            let top_line = self.projection.rows[top_absolute]
                .line_index
                .expect("canonicalized top line");
            let bottom_line = self.projection.rows[bottom_absolute]
                .line_index
                .expect("canonicalized bottom line");
            self.lines.remove(top_line);
            let id = self.alloc_line_id();
            self.lines
                .insert(bottom_line, LogicalLine::blank(id, seqno));
            if self.cursor.line > top_line {
                self.cursor.line -= 1;
            }
            if self.saved_cursor.cursor.line > top_line {
                self.saved_cursor.cursor.line -= 1;
            }
        }
        self.rebuild_projection(cols);
    }

    fn scroll_down_region(&mut self, cols: u16, count: u16, seqno: u64) {
        if self.physical_rows == 0 {
            return;
        }
        let top = self.scroll_top.min(self.physical_rows.saturating_sub(1));
        let bottom = self.scroll_bottom.min(self.physical_rows.saturating_sub(1));
        if top > bottom {
            return;
        }

        self.canonicalize_live_region(cols, top, bottom, seqno);
        for _ in 0..count.max(1) {
            self.rebuild_projection(cols);
            let top_absolute = self.projection.live_start + top as usize;
            let bottom_absolute = self.projection.live_start + bottom as usize;
            let top_line = self.projection.rows[top_absolute]
                .line_index
                .expect("canonicalized top line");
            let bottom_line = self.projection.rows[bottom_absolute]
                .line_index
                .expect("canonicalized bottom line");
            self.lines.remove(bottom_line);
            let id = self.alloc_line_id();
            self.lines.insert(top_line, LogicalLine::blank(id, seqno));
            if self.cursor.line >= top_line {
                self.cursor.line += 1;
            }
            if self.saved_cursor.cursor.line >= top_line {
                self.saved_cursor.cursor.line += 1;
            }
        }
        self.rebuild_projection(cols);
    }
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
        let mut terminal = Self {
            rows,
            cols,
            scrollback_limit,
            next_seqno: 1,
            primary: ScreenBuffer::new(rows, 0),
            alternate: ScreenBuffer::new(rows, 0),
            modes: TerminalModes::default(),
            tab_stops: default_tab_stops(cols),
        };
        terminal.refresh_projections();
        terminal
    }

    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    pub fn cursor_position(&self) -> (u16, u16) {
        let cursor = self.active_buffer().projection.cursor_live;
        (cursor.row, cursor.col)
    }

    pub fn viewport_top(&self) -> StableRowIndex {
        self.active_buffer().viewport_top()
    }

    pub fn follow_viewport_bottom(&mut self) -> bool {
        let changed = self.active_buffer_mut().set_viewport_top(None);
        self.refresh_projections();
        changed
    }

    pub fn scroll_viewport_up(&mut self, rows: u16) -> bool {
        let changed = self.active_buffer_mut().scroll_viewport_up(rows.max(1));
        self.refresh_projections();
        changed
    }

    pub fn scroll_viewport_down(&mut self, rows: u16) -> bool {
        let changed = self.active_buffer_mut().scroll_viewport_down(rows.max(1));
        self.refresh_projections();
        changed
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

        self.rows = rows;
        self.cols = cols;
        self.primary.resize_rows(rows);
        self.alternate.resize_rows(rows);
        self.resize_tab_stops(cols);
        self.primary.pending_wrap = false;
        self.alternate.pending_wrap = false;
        self.refresh_projections();
        self.primary.trim_scrollback(cols, self.scrollback_limit);
        self.refresh_projections();
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
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let width = UnicodeWidthChar::width(c).unwrap_or(1);
        if width == 0 {
            let cursor = self.active_buffer().cursor;
            self.active_buffer_mut()
                .append_combining(cursor.line, cursor.col, c, seqno);
            self.refresh_projections();
            return;
        }
        if cols == 0 || width > cols as usize {
            return;
        }

        if self.active_buffer().pending_wrap && self.modes.wraparound {
            self.active_buffer_mut().soft_wrap_cursor(cols, seqno);
        }

        let screen_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols) as usize
        };
        if screen_col + width > cols as usize {
            self.active_buffer_mut().soft_wrap_cursor(cols, seqno);
        }

        let (cursor, attrs) = {
            let buffer = self.active_buffer();
            (buffer.cursor, buffer.attrs)
        };

        self.active_buffer_mut()
            .clear_overwrite(cursor.line, cursor.col, seqno);
        if width == 2 {
            self.active_buffer_mut()
                .clear_overwrite(cursor.line, cursor.col + 1, seqno);
        }
        self.active_buffer_mut().set_cell(
            cursor.line,
            cursor.col,
            TerminalCell::new(c.to_string(), attrs, width == 2),
            seqno,
        );
        if width == 2 {
            self.active_buffer_mut().set_cell(
                cursor.line,
                cursor.col + 1,
                TerminalCell::continuation(attrs),
                seqno,
            );
        }

        let next_screen_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols) as usize + width
        };
        if next_screen_col >= cols as usize {
            let row_start = {
                let buffer = self.active_buffer_mut();
                buffer.current_row_start(cols)
            };
            let max_col = cols.saturating_sub(1) as usize;
            let buffer = self.active_buffer_mut();
            buffer.pending_wrap = true;
            buffer.cursor.col = row_start + max_col;
        } else {
            self.active_buffer_mut().cursor.col += width;
            self.active_buffer_mut().pending_wrap = false;
        }

        self.refresh_projections();
    }

    pub fn linefeed(&mut self) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let current_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        let current_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols)
        };
        self.active_buffer_mut()
            .ensure_hard_break_after_cursor_row(cols, seqno);
        if current_row == self.active_buffer().scroll_bottom {
            let scrollback_limit = self.active_scrollback_limit();
            self.active_buffer_mut()
                .scroll_up_region(cols, scrollback_limit, 1, seqno);
            let bottom = self.active_buffer().scroll_bottom;
            self.active_buffer_mut()
                .set_cursor_screen_position(cols, bottom, current_col, seqno);
        } else {
            self.active_buffer_mut().set_cursor_screen_position(
                cols,
                current_row.saturating_add(1),
                current_col,
                seqno,
            );
        }
        self.active_buffer_mut().pending_wrap = false;
        self.refresh_projections();
    }

    pub fn carriage_return(&mut self) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut().move_cursor_to_col(cols, 0, seqno);
        self.refresh_projections();
    }

    pub fn backspace(&mut self) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let current_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols)
        };
        self.active_buffer_mut()
            .move_cursor_to_col(cols, current_col.saturating_sub(1), seqno);
        self.refresh_projections();
    }

    pub fn tab(&mut self) {
        let cols = self.cols;
        if cols == 0 {
            return;
        }
        let seqno = self.bump_seqno();
        let cursor_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols) as usize
        };
        let next = self
            .next_tab_stop(cursor_col)
            .unwrap_or(cols.saturating_sub(1) as usize) as u16;
        self.active_buffer_mut()
            .move_cursor_to_col(cols, next, seqno);
        self.refresh_projections();
    }

    pub fn move_forward_tabs(&mut self, count: u16) {
        for _ in 0..count.max(1) {
            self.tab();
        }
    }

    pub fn move_backward_tabs(&mut self, count: u16) {
        let cols = self.cols;
        if cols == 0 {
            return;
        }
        let seqno = self.bump_seqno();
        let mut cursor_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols) as usize
        };
        for _ in 0..count.max(1) {
            cursor_col = self.previous_tab_stop(cursor_col).unwrap_or(0);
        }
        self.active_buffer_mut()
            .move_cursor_to_col(cols, cursor_col as u16, seqno);
        self.refresh_projections();
    }

    pub fn set_horizontal_tabstop(&mut self) {
        let cursor_col = self.active_buffer().projection.cursor_live.col as usize;
        if let Some(tab_stop) = self.tab_stops.get_mut(cursor_col) {
            *tab_stop = true;
        }
    }

    pub fn clear_current_tab_stop(&mut self) {
        let cursor_col = self.active_buffer().projection.cursor_live.col as usize;
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
        self.primary.reset(self.rows, seqno);
        self.alternate.reset(self.rows, seqno);
        self.modes = TerminalModes::default();
        self.tab_stops = default_tab_stops(self.cols);
        self.refresh_projections();
    }

    pub fn reverse_index(&mut self) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let current_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        if current_row == self.active_buffer().scroll_top {
            self.active_buffer_mut().scroll_down_region(cols, 1, seqno);
            self.active_buffer_mut()
                .set_cursor_screen_position(cols, current_row, 0, seqno);
        } else {
            self.active_buffer_mut()
                .move_cursor_to_row(cols, current_row.saturating_sub(1), seqno);
        }
        self.refresh_projections();
    }

    pub fn save_cursor(&mut self) {
        self.active_buffer_mut().save_cursor();
        self.refresh_projections();
    }

    pub fn restore_cursor(&mut self) {
        self.active_buffer_mut().restore_cursor();
        self.refresh_projections();
    }

    pub fn cursor_up(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let current_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        self.active_buffer_mut().move_cursor_to_row(
            cols,
            current_row.saturating_sub(count.max(1)),
            seqno,
        );
        self.refresh_projections();
    }

    pub fn cursor_down(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let max_row = self.rows.saturating_sub(1);
        let current_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        self.active_buffer_mut().move_cursor_to_row(
            cols,
            (current_row + count.max(1)).min(max_row),
            seqno,
        );
        self.refresh_projections();
    }

    pub fn cursor_forward(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let max_col = cols.saturating_sub(1);
        let current_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols)
        };
        self.active_buffer_mut().move_cursor_to_col(
            cols,
            (current_col + count.max(1)).min(max_col),
            seqno,
        );
        self.refresh_projections();
    }

    pub fn cursor_back(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let current_col = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_col(cols)
        };
        self.active_buffer_mut().move_cursor_to_col(
            cols,
            current_col.saturating_sub(count.max(1)),
            seqno,
        );
        self.refresh_projections();
    }

    pub fn cursor_next_line(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let max_row = self.rows.saturating_sub(1);
        let current_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        self.active_buffer_mut().set_cursor_screen_position(
            cols,
            (current_row + count.max(1)).min(max_row),
            0,
            seqno,
        );
        self.refresh_projections();
    }

    pub fn cursor_prev_line(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let current_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        self.active_buffer_mut().set_cursor_screen_position(
            cols,
            current_row.saturating_sub(count.max(1)),
            0,
            seqno,
        );
        self.refresh_projections();
    }

    pub fn set_cursor_col(&mut self, col: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut()
            .move_cursor_to_col(cols, col, seqno);
        self.refresh_projections();
    }

    pub fn set_cursor_row(&mut self, row: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut()
            .move_cursor_to_row(cols, row, seqno);
        self.refresh_projections();
    }

    pub fn set_cursor_position(&mut self, row: u16, col: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut()
            .set_cursor_screen_position(cols, row, col, seqno);
        self.refresh_projections();
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        match mode {
            3 => self.active_buffer_mut().clear_scrollback(cols),
            1 => {
                let cursor_row = {
                    let buffer = self.active_buffer_mut();
                    buffer.current_screen_row(cols)
                };
                for row in 0..cursor_row {
                    let absolute = self.active_buffer().projection.live_start + row as usize;
                    let line = self
                        .active_buffer_mut()
                        .materialize_absolute_row(cols, absolute, seqno);
                    self.active_buffer_mut()
                        .clear_range(line, 0, usize::MAX, seqno);
                }
                let cursor_col = {
                    let buffer = self.active_buffer_mut();
                    buffer.current_screen_col(cols) as usize + 1
                };
                let absolute = self.active_buffer().projection.live_start + cursor_row as usize;
                let line = self
                    .active_buffer_mut()
                    .materialize_absolute_row(cols, absolute, seqno);
                self.active_buffer_mut()
                    .clear_range(line, 0, cursor_col, seqno);
            }
            2 => {
                for row in 0..self.rows {
                    let absolute = self.active_buffer().projection.live_start + row as usize;
                    let line = self
                        .active_buffer_mut()
                        .materialize_absolute_row(cols, absolute, seqno);
                    self.active_buffer_mut()
                        .clear_range(line, 0, usize::MAX, seqno);
                }
            }
            _ => {
                let cursor_row = {
                    let buffer = self.active_buffer_mut();
                    buffer.current_screen_row(cols)
                };
                let cursor_col = {
                    let buffer = self.active_buffer_mut();
                    buffer.current_screen_col(cols) as usize
                };
                let absolute = self.active_buffer().projection.live_start + cursor_row as usize;
                let line = self
                    .active_buffer_mut()
                    .materialize_absolute_row(cols, absolute, seqno);
                self.active_buffer_mut()
                    .clear_range(line, cursor_col, usize::MAX, seqno);
                for row in cursor_row + 1..self.rows {
                    let absolute = self.active_buffer().projection.live_start + row as usize;
                    let line = self
                        .active_buffer_mut()
                        .materialize_absolute_row(cols, absolute, seqno);
                    self.active_buffer_mut()
                        .clear_range(line, 0, usize::MAX, seqno);
                }
            }
        }
        self.refresh_projections();
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let absolute = self.active_buffer().projection.live_start
            + self.active_buffer().projection.cursor_live.row as usize;
        let line = self
            .active_buffer_mut()
            .materialize_absolute_row(cols, absolute, seqno);
        let cursor_col = self.active_buffer().projection.cursor_live.col as usize;
        match mode {
            1 => self
                .active_buffer_mut()
                .clear_range(line, 0, cursor_col + 1, seqno),
            2 => self
                .active_buffer_mut()
                .clear_range(line, 0, usize::MAX, seqno),
            _ => self
                .active_buffer_mut()
                .clear_range(line, cursor_col, usize::MAX, seqno),
        }
        self.refresh_projections();
    }

    pub fn insert_blank_chars(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let absolute = self.active_buffer().projection.live_start
            + self.active_buffer().projection.cursor_live.row as usize;
        let line = self
            .active_buffer_mut()
            .materialize_absolute_row(cols, absolute, seqno);
        let cursor_col = self.active_buffer().projection.cursor_live.col as usize;
        self.active_buffer_mut().shift_right(
            line,
            cursor_col,
            count.max(1) as usize,
            cols as usize,
            seqno,
        );
        self.refresh_projections();
    }

    pub fn delete_chars(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let absolute = self.active_buffer().projection.live_start
            + self.active_buffer().projection.cursor_live.row as usize;
        let line = self
            .active_buffer_mut()
            .materialize_absolute_row(cols, absolute, seqno);
        let cursor_col = self.active_buffer().projection.cursor_live.col as usize;
        self.active_buffer_mut().shift_left(
            line,
            cursor_col,
            count.max(1) as usize,
            cols as usize,
            seqno,
        );
        self.refresh_projections();
    }

    pub fn erase_chars(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let absolute = self.active_buffer().projection.live_start
            + self.active_buffer().projection.cursor_live.row as usize;
        let line = self
            .active_buffer_mut()
            .materialize_absolute_row(cols, absolute, seqno);
        let cursor_col = self.active_buffer().projection.cursor_live.col as usize;
        self.active_buffer_mut().clear_range(
            line,
            cursor_col,
            cursor_col + count.max(1) as usize,
            seqno,
        );
        self.refresh_projections();
    }

    pub fn insert_lines(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let cursor_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        let scroll_top = self.active_buffer().scroll_top;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        if cursor_row < scroll_top || cursor_row > scroll_bottom {
            return;
        }

        self.active_buffer_mut()
            .canonicalize_live_region(cols, cursor_row, scroll_bottom, seqno);
        for _ in 0..count.max(1) {
            self.active_buffer_mut().rebuild_projection(cols);
            let absolute = self.active_buffer().projection.live_start + cursor_row as usize;
            let bottom_absolute =
                self.active_buffer().projection.live_start + scroll_bottom as usize;
            let insert_at = self.active_buffer().projection.rows[absolute]
                .line_index
                .expect("canonicalized insert line");
            let remove_at = self.active_buffer().projection.rows[bottom_absolute]
                .line_index
                .expect("canonicalized bottom line");
            let id = self.active_buffer_mut().alloc_line_id();
            self.active_buffer_mut()
                .lines
                .insert(insert_at, LogicalLine::blank(id, seqno));
            self.active_buffer_mut().lines.remove(remove_at + 1);
        }
        self.refresh_projections();
    }

    pub fn delete_lines(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let cursor_row = {
            let buffer = self.active_buffer_mut();
            buffer.current_screen_row(cols)
        };
        let scroll_top = self.active_buffer().scroll_top;
        let scroll_bottom = self.active_buffer().scroll_bottom;
        if cursor_row < scroll_top || cursor_row > scroll_bottom {
            return;
        }

        self.active_buffer_mut()
            .canonicalize_live_region(cols, cursor_row, scroll_bottom, seqno);
        for _ in 0..count.max(1) {
            self.active_buffer_mut().rebuild_projection(cols);
            let absolute = self.active_buffer().projection.live_start + cursor_row as usize;
            let bottom_absolute =
                self.active_buffer().projection.live_start + scroll_bottom as usize;
            let remove_at = self.active_buffer().projection.rows[absolute]
                .line_index
                .expect("canonicalized delete line");
            let insert_at = self.active_buffer().projection.rows[bottom_absolute]
                .line_index
                .expect("canonicalized bottom line");
            self.active_buffer_mut().lines.remove(remove_at);
            let id = self.active_buffer_mut().alloc_line_id();
            self.active_buffer_mut()
                .lines
                .insert(insert_at, LogicalLine::blank(id, seqno));
        }
        self.refresh_projections();
    }

    pub fn scroll_up(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        let scrollback_limit = self.active_scrollback_limit();
        self.active_buffer_mut()
            .scroll_up_region(cols, scrollback_limit, count.max(1), seqno);
        self.refresh_projections();
    }

    pub fn scroll_down(&mut self, count: u16) {
        let cols = self.cols;
        let seqno = self.bump_seqno();
        self.active_buffer_mut()
            .scroll_down_region(cols, count.max(1), seqno);
        self.refresh_projections();
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
        buffer.cursor = CanonicalCursor::default();
        buffer.pending_wrap = false;
        self.refresh_projections();
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

    fn refresh_projections(&mut self) {
        self.primary.rebuild_projection(self.cols);
        self.alternate.rebuild_projection(self.cols);
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
            self.alternate.reset(self.rows, seqno);
            self.modes.alternate_screen = true;
        } else {
            if !self.modes.alternate_screen {
                return;
            }

            self.modes.alternate_screen = false;
            if save_cursor {
                self.primary.restore_cursor();
            }
        }
        self.refresh_projections();
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

fn trim_trailing_blank_cells(cells: &mut Vec<TerminalCell>) {
    while cells.last().is_some_and(TerminalCell::is_blank) {
        cells.pop();
    }
}

fn projected_row_count_for_line(line: &LogicalLine, cols: u16) -> usize {
    let width = cols.max(1) as usize;
    if line.cells.is_empty() {
        return 1;
    }

    let mut count = 0;
    let mut start = 0;
    while start < line.cells.len() {
        start = reflow_break(&line.cells, start, width);
        count += 1;
    }
    count.max(1)
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

        terminal.resize(4, 6);
        terminal.resize(2, 12);
        terminal.resize(5, 5);
        terminal.resize(3, 10);

        assert!(
            terminal
                .contents_between_stable(0, 0, 6, 10)
                .contains("alpha")
        );
        assert!(
            terminal
                .contents_between_stable(0, 0, 6, 10)
                .contains("golf")
        );
    }

    #[test]
    fn prompt_redraw_during_resize_does_not_duplicate_history() {
        let mut terminal = TerminalState::new(3, 12, 64);
        write_line(&mut terminal, "old output");
        for ch in "$ ls".chars() {
            terminal.print(ch);
        }

        terminal.resize(3, 8);
        terminal.carriage_return();
        for ch in "$ pwd".chars() {
            terminal.print(ch);
        }
        terminal.resize(3, 12);

        assert_eq!(
            terminal.visible_row(0).expect("row").text_range(0, 10),
            "old output"
        );
        assert_eq!(
            terminal.visible_row(1).expect("row").text_range(0, 5),
            "$ pwd"
        );
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
    fn wide_characters_project_cleanly_near_wrap_boundaries() {
        let mut terminal = TerminalState::new(3, 4, 8);
        for ch in ['a', 'b', '界', 'c'] {
            terminal.print(ch);
        }

        assert_eq!(
            terminal.visible_row(0).expect("row").text_range(0, 4),
            "ab界"
        );
        assert_eq!(terminal.visible_row(1).expect("row").text_range(0, 1), "c");
    }

    #[test]
    fn alternate_screen_resize_keeps_primary_scrollback() {
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

        assert!(terminal.contents_between_stable(0, 0, 3, 8).contains("one"));
        assert!(
            terminal
                .contents_between_stable(0, 0, 3, 8)
                .contains("three")
        );
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
