use std::cmp::{max, min};
use std::ops::{Index, IndexMut, Range, RangeFrom, RangeFull, RangeTo, RangeToInclusive};
use std::{ptr, slice};

use super::Column;

pub trait GridCell: Sized {
    fn is_empty(&self) -> bool;
    fn reset(&mut self, template: &Self);
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct GridRow<T> {
    inner: Vec<T>,
    pub occ: usize,
}

impl<T: Default> GridRow<T> {
    pub fn new(columns: usize) -> Self {
        debug_assert!(columns >= 1);

        let mut inner = Vec::with_capacity(columns);
        unsafe {
            let mut ptr = inner.as_mut_ptr();
            for _ in 1..columns {
                ptr::write(ptr, T::default());
                ptr = ptr.offset(1);
            }
            ptr::write(ptr, T::default());
            inner.set_len(columns);
        }

        Self { inner, occ: 0 }
    }

    pub fn grow(&mut self, columns: usize) {
        if self.inner.len() < columns {
            self.inner.resize_with(columns, T::default);
        }
    }

    pub fn shrink(&mut self, columns: usize) -> Option<Vec<T>>
    where
        T: GridCell,
    {
        if self.inner.len() <= columns {
            return None;
        }

        let mut new_row = self.inner.split_off(columns);
        let index = new_row
            .iter()
            .rposition(|cell| !cell.is_empty())
            .map_or(0, |i| i + 1);
        new_row.truncate(index);
        self.occ = min(self.occ, columns);

        if new_row.is_empty() {
            None
        } else {
            Some(new_row)
        }
    }

    pub fn reset(&mut self, template: &T)
    where
        T: GridCell,
    {
        for item in &mut self.inner[0..self.occ] {
            item.reset(template);
        }
        self.occ = 0;
    }
}

impl<T> GridRow<T> {
    pub fn from_vec(vec: Vec<T>, occ: usize) -> Self {
        Self { inner: vec, occ }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.inner
    }

    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.inner.clone()
    }

    pub fn last(&self) -> Option<&T> {
        self.inner.last()
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.occ = self.inner.len();
        self.inner.last_mut()
    }

    pub fn append(&mut self, vec: &mut Vec<T>) {
        self.occ += vec.len();
        self.inner.append(vec);
    }

    pub fn append_front(&mut self, mut vec: Vec<T>) {
        self.occ += vec.len();
        vec.append(&mut self.inner);
        self.inner = vec;
    }

    pub fn is_clear(&self) -> bool
    where
        T: GridCell,
    {
        self.inner.iter().all(GridCell::is_empty)
    }

    pub fn front_split_off(&mut self, at: usize) -> Vec<T> {
        self.occ = self.occ.saturating_sub(at);
        let mut split = self.inner.split_off(at);
        std::mem::swap(&mut split, &mut self.inner);
        split
    }
}

impl<'a, T> IntoIterator for &'a GridRow<T> {
    type IntoIter = slice::Iter<'a, T>;
    type Item = &'a T;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut GridRow<T> {
    type IntoIter = slice::IterMut<'a, T>;
    type Item = &'a mut T;

    fn into_iter(self) -> Self::IntoIter {
        self.occ = self.len();
        self.inner.iter_mut()
    }
}

impl<T> Index<Column> for GridRow<T> {
    type Output = T;

    fn index(&self, index: Column) -> &Self::Output {
        &self.inner[index.0]
    }
}

impl<T> IndexMut<Column> for GridRow<T> {
    fn index_mut(&mut self, index: Column) -> &mut Self::Output {
        self.occ = max(self.occ, *index + 1);
        &mut self.inner[index.0]
    }
}

impl<T> Index<Range<Column>> for GridRow<T> {
    type Output = [T];

    fn index(&self, index: Range<Column>) -> &Self::Output {
        &self.inner[index.start.0..index.end.0]
    }
}

impl<T> IndexMut<Range<Column>> for GridRow<T> {
    fn index_mut(&mut self, index: Range<Column>) -> &mut Self::Output {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[index.start.0..index.end.0]
    }
}

impl<T> Index<RangeTo<Column>> for GridRow<T> {
    type Output = [T];

    fn index(&self, index: RangeTo<Column>) -> &Self::Output {
        &self.inner[..index.end.0]
    }
}

impl<T> IndexMut<RangeTo<Column>> for GridRow<T> {
    fn index_mut(&mut self, index: RangeTo<Column>) -> &mut Self::Output {
        self.occ = max(self.occ, *index.end);
        &mut self.inner[..index.end.0]
    }
}

impl<T> Index<RangeFrom<Column>> for GridRow<T> {
    type Output = [T];

    fn index(&self, index: RangeFrom<Column>) -> &Self::Output {
        &self.inner[index.start.0..]
    }
}

impl<T> IndexMut<RangeFrom<Column>> for GridRow<T> {
    fn index_mut(&mut self, index: RangeFrom<Column>) -> &mut Self::Output {
        self.occ = self.len();
        &mut self.inner[index.start.0..]
    }
}

impl<T> Index<RangeFull> for GridRow<T> {
    type Output = [T];

    fn index(&self, _: RangeFull) -> &Self::Output {
        &self.inner[..]
    }
}

impl<T> IndexMut<RangeFull> for GridRow<T> {
    fn index_mut(&mut self, _: RangeFull) -> &mut Self::Output {
        self.occ = self.len();
        &mut self.inner[..]
    }
}

impl<T> Index<RangeToInclusive<Column>> for GridRow<T> {
    type Output = [T];

    fn index(&self, index: RangeToInclusive<Column>) -> &Self::Output {
        &self.inner[..=index.end.0]
    }
}

impl<T> IndexMut<RangeToInclusive<Column>> for GridRow<T> {
    fn index_mut(&mut self, index: RangeToInclusive<Column>) -> &mut Self::Output {
        self.occ = max(self.occ, *index.end + 1);
        &mut self.inner[..=index.end.0]
    }
}
