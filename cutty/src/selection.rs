use winit::dpi::PhysicalPosition;

use crate::terminal::TerminalState;
use crate::text::{CellMetrics, PADDING_X, PADDING_Y};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellPos {
    pub row: u16,
    pub col: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: CellPos,
    pub end: CellPos,
}

#[derive(Clone, Debug, Default)]
pub struct SelectionState {
    anchor: Option<CellPos>,
    focus: Option<CellPos>,
    selecting: bool,
}

impl SelectionState {
    pub fn begin(&mut self, pos: CellPos) {
        self.anchor = Some(pos);
        self.focus = Some(pos);
        self.selecting = true;
    }

    pub fn update(&mut self, pos: CellPos) -> bool {
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

    pub fn range(&self) -> Option<SelectionRange> {
        let anchor = self.anchor?;
        let focus = self.focus?;
        if anchor == focus {
            return None;
        }

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
        Some(terminal.contents_between(range.start.row, range.start.col, range.end.row, end_col))
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
