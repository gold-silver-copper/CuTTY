use winit::dpi::PhysicalPosition;

use crate::terminal::{StableRowIndex, TerminalState};
use crate::text::{CellMetrics, PADDING_X, PADDING_Y};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellPos {
    pub row: u16,
    pub col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StableCellPos {
    pub row: StableRowIndex,
    pub col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: StableCellPos,
    pub end: StableCellPos,
}

#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    anchor: Option<StableCellPos>,
    focus: Option<StableCellPos>,
    selecting: bool,
}

impl SelectionState {
    pub fn begin(&mut self, pos: StableCellPos) {
        self.anchor = Some(pos);
        self.focus = Some(pos);
        self.selecting = true;
    }

    pub fn update(&mut self, pos: StableCellPos) -> bool {
        if self.selecting && self.focus != Some(pos) {
            self.focus = Some(pos);
            return true;
        }
        false
    }

    pub fn finish(&mut self) -> bool {
        let changed = self.selecting;
        self.selecting = false;
        if self.anchor == self.focus {
            self.clear();
        }
        changed
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.focus = None;
        self.selecting = false;
    }

    pub fn set_range(&mut self, anchor: StableCellPos, focus: StableCellPos) -> bool {
        let changed = self.anchor != Some(anchor) || self.focus != Some(focus) || self.selecting;
        self.anchor = Some(anchor);
        self.focus = Some(focus);
        self.selecting = false;
        changed
    }

    pub fn range(&self) -> Option<SelectionRange> {
        let anchor = self.anchor?;
        let focus = self.focus?;

        let (start, end) = if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        };
        Some(SelectionRange { start, end })
    }

    pub fn is_selected(&self) -> bool {
        self.range().is_some()
    }

    pub fn selection_text(&self, terminal: &TerminalState) -> Option<String> {
        let range = self.range()?;
        let (_, cols) = terminal.size();
        let end_col = range.end.col.saturating_add(1).min(cols);
        Some(terminal.contents_between_stable(
            range.start.row,
            range.start.col,
            range.end.row,
            end_col,
        ))
    }

    pub fn cols_for_visible_row(&self, terminal: &TerminalState, row: u16) -> Option<(u16, u16)> {
        let range = self.range()?;
        let stable_row = terminal.visible_row_to_stable_row(row);
        if stable_row < range.start.row || stable_row > range.end.row {
            return None;
        }

        let (_, cols) = terminal.size();
        let start_col = if stable_row == range.start.row {
            range.start.col
        } else {
            0
        };
        let end_col = if stable_row == range.end.row {
            range.end.col.saturating_add(1).min(cols)
        } else {
            cols
        };

        (end_col > start_col).then_some((start_col, end_col))
    }
}

pub fn cell_at_position(
    position: PhysicalPosition<f64>,
    metrics: CellMetrics,
    terminal: &TerminalState,
) -> Option<CellPos> {
    let x = position.x as f32 - PADDING_X;
    let y = position.y as f32 - PADDING_Y;
    if x < 0.0 || y < 0.0 {
        return None;
    }

    let (rows, cols) = terminal.size();
    if rows == 0 || cols == 0 {
        return None;
    }

    let col = (x / metrics.width).floor() as u16;
    let row = (y / metrics.height).floor() as u16;
    if row >= rows || col >= cols {
        return None;
    }

    Some(CellPos { row, col })
}

#[cfg(test)]
mod tests {
    use super::{SelectionState, StableCellPos};
    use crate::terminal::TerminalState;

    #[test]
    fn explicit_single_cell_selection_is_preserved() {
        let mut selection = SelectionState::default();
        let cell = StableCellPos { row: 1, col: 2 };

        assert!(selection.set_range(cell, cell));
        assert_eq!(selection.range().expect("range").start, cell);
        assert_eq!(selection.range().expect("range").end, cell);
    }

    #[test]
    fn single_cell_selection_text_round_trips() {
        let mut terminal = TerminalState::new(2, 4, 0);
        terminal.print('a');
        terminal.print('b');

        let mut selection = SelectionState::default();
        selection.set_range(
            StableCellPos { row: 0, col: 1 },
            StableCellPos { row: 0, col: 1 },
        );

        assert_eq!(selection.selection_text(&terminal).as_deref(), Some("b"));
    }
}
