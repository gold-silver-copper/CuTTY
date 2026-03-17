use super::{Column, GridCell, GridRow, Line, Point, RowStorage};

impl GridCell for usize {
    fn is_empty(&self) -> bool {
        *self == 0
    }

    fn reset(&mut self, template: &Self) {
        *self = *template;
    }
}

#[test]
fn storage_rotate_preserves_visible_indexing() {
    let mut storage = RowStorage::<GridRow<usize>>::with_capacity(4, || GridRow::new(1));
    for i in 0..4 {
        storage[Line(i)][Column(0)] = i as usize + 1;
    }

    storage.rotate_down(1);

    assert_eq!(storage[Line(0)][Column(0)], 4);
    assert_eq!(storage[Line(1)][Column(0)], 1);
    assert_eq!(storage[Line(2)][Column(0)], 2);
    assert_eq!(storage[Line(3)][Column(0)], 3);
}

#[test]
fn row_occ_tracks_mutations() {
    let mut row = GridRow::<usize>::new(4);
    row[Column(2)] = 7;
    assert_eq!(row.occ, 3);

    row[Column(3)] = 8;
    assert_eq!(row.occ, 4);
}

#[test]
fn row_shrink_returns_non_empty_tail() {
    let mut row = GridRow::<usize>::new(5);
    row[Column(0)] = 1;
    row[Column(1)] = 2;
    row[Column(3)] = 3;

    let tail = row.shrink(2).expect("tail");
    assert_eq!(row.len(), 2);
    assert_eq!(tail, vec![0, 3]);
}

#[test]
fn point_add_and_sub_wrap_columns() {
    struct Dims;

    impl super::Dimensions for Dims {
        fn total_lines(&self) -> usize {
            4
        }

        fn screen_lines(&self) -> usize {
            4
        }

        fn columns(&self) -> usize {
            3
        }
    }

    let point = Point::new(Line(1), Column(2));
    assert_eq!(
        point.add(&Dims, super::Boundary::Cursor, 2),
        Point::new(Line(2), Column(1))
    );
    assert_eq!(
        point.sub(&Dims, super::Boundary::Cursor, 2),
        Point::new(Line(1), Column(0))
    );
}
