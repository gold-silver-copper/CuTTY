use std::cmp::{Ordering, max, min};
use std::fmt;
use std::ops::{Add, AddAssign, Deref, Sub, SubAssign};

use super::Dimensions;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Boundary {
    Cursor,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct Point<L = Line, C = Column> {
    pub line: L,
    pub column: C,
}

impl<L, C> Point<L, C> {
    pub fn new(line: L, column: C) -> Self {
        Self { line, column }
    }
}

impl Point {
    pub fn sub<D>(mut self, dimensions: &D, boundary: Boundary, rhs: usize) -> Self
    where
        D: Dimensions,
    {
        let cols = dimensions.columns();
        let line_changes = (rhs + cols - 1).saturating_sub(self.column.0) / cols;
        self.line -= line_changes;
        self.column = Column((cols + self.column.0 - rhs % cols) % cols);
        self.grid_clamp(dimensions, boundary)
    }

    pub fn add<D>(mut self, dimensions: &D, boundary: Boundary, rhs: usize) -> Self
    where
        D: Dimensions,
    {
        let cols = dimensions.columns();
        self.line += (rhs + self.column.0) / cols;
        self.column = Column((self.column.0 + rhs) % cols);
        self.grid_clamp(dimensions, boundary)
    }

    pub fn grid_clamp<D>(mut self, dimensions: &D, boundary: Boundary) -> Self
    where
        D: Dimensions,
    {
        let last_column = dimensions.last_column();
        self.column = min(self.column, last_column);

        let topmost_line = dimensions.topmost_line();
        let bottommost_line = dimensions.bottommost_line();

        match boundary {
            Boundary::Cursor if self.line < 0 => Point::new(Line(0), Column(0)),
            Boundary::Grid if self.line < topmost_line => Point::new(topmost_line, Column(0)),
            Boundary::Cursor | Boundary::Grid if self.line > bottommost_line => {
                Point::new(bottommost_line, last_column)
            }
            Boundary::None => {
                self.line = self.line.grid_clamp(dimensions, boundary);
                self
            }
            _ => self,
        }
    }
}

impl<L: Ord, C: Ord> PartialOrd for Point<L, C> {
    fn partial_cmp(&self, other: &Point<L, C>) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L: Ord, C: Ord> Ord for Point<L, C> {
    fn cmp(&self, other: &Point<L, C>) -> Ordering {
        match (self.line.cmp(&other.line), self.column.cmp(&other.column)) {
            (Ordering::Equal, ord) | (ord, _) => ord,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
pub struct Line(pub i32);

impl Line {
    pub fn grid_clamp<D: Dimensions>(self, dimensions: &D, boundary: Boundary) -> Self {
        match boundary {
            Boundary::Cursor => max(Line(0), min(dimensions.bottommost_line(), self)),
            Boundary::Grid => {
                let bottommost_line = dimensions.bottommost_line();
                let topmost_line = dimensions.topmost_line();
                max(topmost_line, min(bottommost_line, self))
            }
            Boundary::None => {
                let screen_lines = dimensions.screen_lines() as i32;
                let total_lines = dimensions.total_lines() as i32;

                if self.0 >= screen_lines {
                    let topmost_line = dimensions.topmost_line();
                    let extra = (self.0 - screen_lines) % total_lines;
                    topmost_line + extra
                } else {
                    let bottommost_line = dimensions.bottommost_line();
                    let extra = (self.0 - screen_lines + 1) % total_lines;
                    bottommost_line + extra
                }
            }
        }
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for Line {
    fn from(source: usize) -> Self {
        Self(source as i32)
    }
}

impl Add<usize> for Line {
    type Output = Line;

    fn add(self, rhs: usize) -> Line {
        self + rhs as i32
    }
}

impl AddAssign<usize> for Line {
    fn add_assign(&mut self, rhs: usize) {
        *self += rhs as i32;
    }
}

impl Sub<usize> for Line {
    type Output = Line;

    fn sub(self, rhs: usize) -> Line {
        self - rhs as i32
    }
}

impl SubAssign<usize> for Line {
    fn sub_assign(&mut self, rhs: usize) {
        *self -= rhs as i32;
    }
}

impl PartialOrd<usize> for Line {
    fn partial_cmp(&self, other: &usize) -> Option<Ordering> {
        self.0.partial_cmp(&(*other as i32))
    }
}

impl PartialEq<usize> for Line {
    fn eq(&self, other: &usize) -> bool {
        self.0.eq(&(*other as i32))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default, Ord, PartialOrd)]
pub struct Column(pub usize);

impl fmt::Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

macro_rules! ops {
    ($ty:ty, $construct:expr, $primitive:ty) => {
        impl Deref for $ty {
            type Target = $primitive;

            fn deref(&self) -> &$primitive {
                &self.0
            }
        }

        impl From<$primitive> for $ty {
            fn from(val: $primitive) -> $ty {
                $construct(val)
            }
        }

        impl Add<$ty> for $ty {
            type Output = $ty;

            fn add(self, rhs: $ty) -> $ty {
                $construct(self.0 + rhs.0)
            }
        }

        impl AddAssign<$ty> for $ty {
            fn add_assign(&mut self, rhs: $ty) {
                self.0 += rhs.0;
            }
        }

        impl Add<$primitive> for $ty {
            type Output = $ty;

            fn add(self, rhs: $primitive) -> $ty {
                $construct(self.0 + rhs)
            }
        }

        impl AddAssign<$primitive> for $ty {
            fn add_assign(&mut self, rhs: $primitive) {
                self.0 += rhs;
            }
        }

        impl Sub<$ty> for $ty {
            type Output = $ty;

            fn sub(self, rhs: $ty) -> $ty {
                $construct(self.0 - rhs.0)
            }
        }

        impl SubAssign<$ty> for $ty {
            fn sub_assign(&mut self, rhs: $ty) {
                self.0 -= rhs.0;
            }
        }

        impl Sub<$primitive> for $ty {
            type Output = $ty;

            fn sub(self, rhs: $primitive) -> $ty {
                $construct(self.0 - rhs)
            }
        }

        impl SubAssign<$primitive> for $ty {
            fn sub_assign(&mut self, rhs: $primitive) {
                self.0 -= rhs;
            }
        }
    };
}

ops!(Line, Line, i32);
ops!(Column, Column, usize);
