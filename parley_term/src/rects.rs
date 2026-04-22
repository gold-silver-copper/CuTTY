use std::array;

use vello::Scene;
use vello::kurbo::{Affine, BezPath, Rect, Stroke};
use vello::peniko::Fill;

use crate::color::{Rgb, color_from_rgb};
use crate::grid::{CellFlags, SizeInfo, TerminalCell};
use crate::text::TextMetrics;

#[derive(Debug, Copy, Clone)]
pub struct RenderRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Rgb,
    pub alpha: f32,
    pub kind: RectKind,
}

impl RenderRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: Rgb, alpha: f32) -> Self {
        Self { x, y, width, height, color, alpha, kind: RectKind::Normal }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RenderLine {
    pub start_row: usize,
    pub start_column: usize,
    pub end_row: usize,
    pub end_column: usize,
    pub color: Rgb,
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RectKind {
    Normal = 0,
    Undercurl = 1,
    DottedUnderline = 2,
    DashedUnderline = 3,
}

impl RenderLine {
    pub fn rects(&self, metrics: &TextMetrics, size: &SizeInfo, flag: CellFlags) -> Vec<RenderRect> {
        let mut rects = Vec::new();

        let mut start_row = self.start_row;
        let mut start_column = self.start_column;
        while start_row < self.end_row {
            Self::push_rects(
                &mut rects,
                metrics,
                size,
                flag,
                start_row,
                start_column,
                start_row,
                size.last_column(),
                self.color,
            );
            start_row += 1;
            start_column = 0;
        }

        Self::push_rects(
            &mut rects,
            metrics,
            size,
            flag,
            start_row,
            start_column,
            self.end_row,
            self.end_column,
            self.color,
        );

        rects
    }

    #[allow(clippy::too_many_arguments)]
    fn push_rects(
        rects: &mut Vec<RenderRect>,
        metrics: &TextMetrics,
        size: &SizeInfo,
        flag: CellFlags,
        start_row: usize,
        start_column: usize,
        end_row: usize,
        end_column: usize,
        color: Rgb,
    ) {
        debug_assert_eq!(start_row, end_row);

        let (position, thickness, kind) = if flag.contains(CellFlags::DOUBLE_UNDERLINE) {
            let top_pos = 0.25 * metrics.descent;
            let bottom_pos = 0.75 * metrics.descent;
            rects.push(Self::create_rect(
                size,
                metrics.descent,
                start_row,
                start_column,
                end_column,
                top_pos,
                metrics.underline_thickness,
                color,
            ));
            (bottom_pos, metrics.underline_thickness, RectKind::Normal)
        } else if flag.contains(CellFlags::UNDERCURL) {
            (metrics.descent, metrics.descent.abs(), RectKind::Undercurl)
        } else if flag.contains(CellFlags::UNDERLINE) {
            (metrics.underline_position, metrics.underline_thickness, RectKind::Normal)
        } else if flag.contains(CellFlags::DOTTED_UNDERLINE) {
            (metrics.descent, metrics.descent.abs(), RectKind::DottedUnderline)
        } else if flag.contains(CellFlags::DASHED_UNDERLINE) {
            (metrics.underline_position, metrics.underline_thickness, RectKind::DashedUnderline)
        } else if flag.contains(CellFlags::STRIKEOUT) {
            (metrics.strikeout_position, metrics.strikeout_thickness, RectKind::Normal)
        } else {
            unreachable!("invalid line flag");
        };

        let mut rect = Self::create_rect(
            size,
            metrics.descent,
            start_row,
            start_column,
            end_column,
            position,
            thickness,
            color,
        );
        rect.kind = kind;
        rects.push(rect);
    }

    fn create_rect(
        size: &SizeInfo,
        descent: f32,
        row: usize,
        start_column: usize,
        end_column: usize,
        position: f32,
        mut thickness: f32,
        color: Rgb,
    ) -> RenderRect {
        let start_x = start_column as f32 * size.cell_width();
        let end_x = (end_column + 1) as f32 * size.cell_width();
        let width = end_x - start_x;

        thickness = thickness.max(1.);

        let line_bottom = (row as f32 + 1.) * size.cell_height();
        let baseline = line_bottom + descent;

        let mut y = (baseline - position - thickness / 2.).round();
        let max_y = line_bottom - thickness;
        if y > max_y {
            y = max_y;
        }

        RenderRect::new(
            start_x + size.padding_x(),
            y + size.padding_y(),
            width,
            thickness,
            color,
            1.,
        )
    }
}

pub struct RenderLines {
    inner: [Vec<RenderLine>; LINE_FLAGS.len()],
}

const LINE_FLAGS: [CellFlags; 6] = [
    CellFlags::UNDERLINE,
    CellFlags::DOUBLE_UNDERLINE,
    CellFlags::STRIKEOUT,
    CellFlags::UNDERCURL,
    CellFlags::DOTTED_UNDERLINE,
    CellFlags::DASHED_UNDERLINE,
];

impl Default for RenderLines {
    fn default() -> Self {
        Self { inner: array::from_fn(|_| Vec::new()) }
    }
}

impl RenderLines {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rects(&self, metrics: &TextMetrics, size: &SizeInfo) -> Vec<RenderRect> {
        let mut rects = Vec::with_capacity(self.inner.iter().map(Vec::len).sum::<usize>());
        for (index, lines) in self.inner.iter().enumerate() {
            let flag = LINE_FLAGS[index];
            for line in lines {
                rects.extend(line.rects(metrics, size, flag));
            }
        }
        rects
    }

    pub fn update(&mut self, cell: &TerminalCell) {
        self.update_flag(cell, CellFlags::UNDERLINE);
        self.update_flag(cell, CellFlags::DOUBLE_UNDERLINE);
        self.update_flag(cell, CellFlags::STRIKEOUT);
        self.update_flag(cell, CellFlags::UNDERCURL);
        self.update_flag(cell, CellFlags::DOTTED_UNDERLINE);
        self.update_flag(cell, CellFlags::DASHED_UNDERLINE);
    }

    fn update_flag(&mut self, cell: &TerminalCell, flag: CellFlags) {
        if !cell.flags.contains(flag) {
            return;
        }

        let color = if flag.contains(CellFlags::STRIKEOUT) { cell.fg } else { cell.underline };
        let end_column = cell.column + cell.width.get() as usize - 1;
        let lines = &mut self.inner[line_flag_index(flag)];

        if let Some(line) = lines.last_mut()
            && color == line.color
            && cell.row == line.end_row
            && cell.column == line.end_column + 1
        {
            line.end_column = end_column;
            return;
        }

        lines.push(RenderLine {
            start_row: cell.row,
            start_column: cell.column,
            end_row: cell.row,
            end_column,
            color,
        });
    }
}

fn line_flag_index(flag: CellFlags) -> usize {
    match flag {
        CellFlags::UNDERLINE => 0,
        CellFlags::DOUBLE_UNDERLINE => 1,
        CellFlags::STRIKEOUT => 2,
        CellFlags::UNDERCURL => 3,
        CellFlags::DOTTED_UNDERLINE => 4,
        CellFlags::DASHED_UNDERLINE => 5,
        _ => unreachable!("invalid line flag"),
    }
}

pub fn paint_rect(scene: &mut Scene, rect: &RenderRect) {
    let brush = color_from_rgb(rect.color).with_alpha(rect.alpha);
    match rect.kind {
        RectKind::Undercurl => paint_undercurl(scene, rect, brush),
        _ => {
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                brush,
                None,
                &Rect::new(
                    rect.x as f64,
                    rect.y as f64,
                    (rect.x + rect.width) as f64,
                    (rect.y + rect.height) as f64,
                ),
            );
        },
    }
}

pub fn paint_rects(scene: &mut Scene, rects: impl IntoIterator<Item = RenderRect>) {
    for rect in rects {
        paint_rect(scene, &rect);
    }
}

fn paint_undercurl(scene: &mut Scene, rect: &RenderRect, brush: vello::peniko::Color) {
    let mut path = BezPath::new();
    let start_x = rect.x as f64;
    let end_x = (rect.x + rect.width) as f64;
    let mid_y = (rect.y + rect.height / 2.0) as f64;
    let amplitude = (rect.height / 2.0).max(1.0) as f64;
    let wavelength = (rect.height * 2.0).max(4.0) as f64;

    let mut x = start_x;
    path.move_to((x, mid_y));
    while x < end_x {
        let next = (x + wavelength / 2.0).min(end_x);
        path.quad_to((x + wavelength / 4.0, mid_y - amplitude), (next, mid_y));
        x = next;
        let next = (x + wavelength / 2.0).min(end_x);
        path.quad_to((x + wavelength / 4.0, mid_y + amplitude), (next, mid_y));
        x = next;
    }

    scene.stroke(&Stroke::new(rect.height.max(1.0) as f64), Affine::IDENTITY, brush, None, &path);
}
