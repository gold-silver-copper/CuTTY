mod index;
mod row;
mod storage;

pub use index::{Boundary, Column, Line, Point};
pub use row::{GridCell, GridRow};
pub use storage::RowStorage;

pub trait Dimensions {
    fn total_lines(&self) -> usize;
    fn screen_lines(&self) -> usize;
    fn columns(&self) -> usize;

    fn last_column(&self) -> Column {
        Column(self.columns().saturating_sub(1))
    }

    fn topmost_line(&self) -> Line {
        Line(-(self.history_size() as i32))
    }

    fn bottommost_line(&self) -> Line {
        Line(self.screen_lines() as i32 - 1)
    }

    fn history_size(&self) -> usize {
        self.total_lines().saturating_sub(self.screen_lines())
    }
}

#[cfg(test)]
mod tests;
