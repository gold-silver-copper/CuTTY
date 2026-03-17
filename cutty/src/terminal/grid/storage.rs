use std::ops::{Index, IndexMut};

use super::Line;

const MAX_CACHE_SIZE: usize = 1_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowStorage<T> {
    inner: Vec<T>,
    zero: usize,
    visible_lines: usize,
    len: usize,
}

impl<T> RowStorage<T> {
    pub fn with_capacity<F>(visible_lines: usize, mut make_row: F) -> Self
    where
        F: FnMut() -> T,
    {
        let mut inner = Vec::with_capacity(visible_lines);
        inner.resize_with(visible_lines, &mut make_row);

        Self {
            inner,
            zero: 0,
            visible_lines,
            len: visible_lines,
        }
    }

    pub fn grow_visible_lines<F>(&mut self, next: usize, make_row: F)
    where
        F: FnMut() -> T,
    {
        let additional_lines = next - self.visible_lines;
        self.initialize(additional_lines, make_row);
        self.visible_lines = next;
    }

    pub fn shrink_visible_lines(&mut self, next: usize) {
        let shrinkage = self.visible_lines - next;
        self.shrink_lines(shrinkage);
        self.visible_lines = next;
    }

    pub fn shrink_lines(&mut self, shrinkage: usize) {
        self.len -= shrinkage;
        if self.inner.len() > self.len + MAX_CACHE_SIZE {
            self.truncate();
        }
    }

    pub fn truncate(&mut self) {
        self.rezero();
        self.inner.truncate(self.len);
    }

    pub fn initialize<F>(&mut self, additional_rows: usize, mut make_row: F)
    where
        F: FnMut() -> T,
    {
        if self.len + additional_rows > self.inner.len() {
            self.rezero();
            let realloc_size = self.inner.len() + additional_rows.max(MAX_CACHE_SIZE);
            self.inner.resize_with(realloc_size, &mut make_row);
        }
        self.len += additional_rows;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        (0..self.len).map(move |index| {
            let mapped = self.compute_top_index(index);
            &self.inner[mapped]
        })
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut T> {
        self.rezero();
        self.inner[..self.len].iter_mut().rev()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        (index < self.len).then(|| {
            let mapped = self.compute_top_index(index);
            &self.inner[mapped]
        })
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.len {
            return None;
        }

        let mapped = self.compute_top_index(index);
        self.inner.get_mut(mapped)
    }

    pub fn swap(&mut self, a: Line, b: Line) {
        let a = self.compute_index(a);
        let b = self.compute_index(b);
        self.inner.swap(a, b);
    }

    pub fn rotate(&mut self, count: isize) {
        debug_assert!(count.unsigned_abs() <= self.inner.len());
        let len = self.inner.len();
        self.zero = (self.zero as isize + count + len as isize) as usize % len;
    }

    pub fn rotate_down(&mut self, count: usize) {
        self.zero = (self.zero + count) % self.inner.len();
    }

    pub fn replace_inner(&mut self, vec: Vec<T>) {
        self.len = vec.len();
        self.inner = vec.into_iter().rev().collect();
        self.zero = 0;
    }

    pub fn take_all(&mut self) -> Vec<T> {
        self.truncate();
        let mut buffer = Vec::new();
        std::mem::swap(&mut buffer, &mut self.inner);
        self.len = 0;
        buffer.reverse();
        buffer
    }

    fn compute_index(&self, requested: Line) -> usize {
        debug_assert!(requested.0 < self.visible_lines as i32);
        let positive = -(requested - self.visible_lines).0 as usize - 1;
        debug_assert!(positive < self.len);
        self.compute_absolute_index(positive)
    }

    fn compute_top_index(&self, logical_top: usize) -> usize {
        let positive = self.len - logical_top - 1;
        self.compute_absolute_index(positive)
    }

    fn compute_absolute_index(&self, logical: usize) -> usize {
        let zeroed = self.zero + logical;
        if zeroed >= self.inner.len() {
            zeroed - self.inner.len()
        } else {
            zeroed
        }
    }

    fn rezero(&mut self) {
        if self.zero == 0 {
            return;
        }

        self.inner.rotate_left(self.zero);
        self.zero = 0;
    }
}

impl<T> Index<Line> for RowStorage<T> {
    type Output = T;

    fn index(&self, index: Line) -> &Self::Output {
        let index = self.compute_index(index);
        &self.inner[index]
    }
}

impl<T> IndexMut<Line> for RowStorage<T> {
    fn index_mut(&mut self, index: Line) -> &mut Self::Output {
        let index = self.compute_index(index);
        &mut self.inner[index]
    }
}
