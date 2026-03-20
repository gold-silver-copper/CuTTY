use std::array;

use vello::Scene;
use vello::kurbo::{Affine, BezPath, Rect, Stroke};
use vello::peniko::Fill;

use cutty_terminal::grid::Dimensions;
use cutty_terminal::index::{Column, Point};
use cutty_terminal::term::cell::Flags;

use crate::display::SizeInfo;
use crate::display::color::Rgb;
use crate::display::content::RenderableCell;
use crate::display::text::{TextMetrics, color_from_rgb};

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
    pub start: Point<usize>,
    pub end: Point<usize>,
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
    pub fn rects(&self, metrics: &TextMetrics, size: &SizeInfo, flag: Flags) -> Vec<RenderRect> {
        let mut rects = Vec::new();

        let mut start = self.start;
        while start.line < self.end.line {
            let end = Point::new(start.line, size.last_column());
            Self::push_rects(&mut rects, metrics, size, flag, start, end, self.color);
            start = Point::new(start.line + 1, Column(0));
        }
        Self::push_rects(&mut rects, metrics, size, flag, start, self.end, self.color);

        rects
    }

    fn push_rects(
        rects: &mut Vec<RenderRect>,
        metrics: &TextMetrics,
        size: &SizeInfo,
        flag: Flags,
        start: Point<usize>,
        end: Point<usize>,
        color: Rgb,
    ) {
        let (position, thickness, kind) = match flag {
            Flags::DOUBLE_UNDERLINE => {
                let top_pos = 0.25 * metrics.descent;
                let bottom_pos = 0.75 * metrics.descent;
                rects.push(Self::create_rect(
                    size,
                    metrics.descent,
                    start,
                    end,
                    top_pos,
                    metrics.underline_thickness,
                    color,
                ));
                (bottom_pos, metrics.underline_thickness, RectKind::Normal)
            },
            Flags::UNDERCURL => (metrics.descent, metrics.descent.abs(), RectKind::Undercurl),
            Flags::UNDERLINE => {
                (metrics.underline_position, metrics.underline_thickness, RectKind::Normal)
            },
            Flags::DOTTED_UNDERLINE => {
                (metrics.descent, metrics.descent.abs(), RectKind::DottedUnderline)
            },
            Flags::DASHED_UNDERLINE => {
                (metrics.underline_position, metrics.underline_thickness, RectKind::DashedUnderline)
            },
            Flags::STRIKEOUT => {
                (metrics.strikeout_position, metrics.strikeout_thickness, RectKind::Normal)
            },
            _ => unreachable!("invalid line flag"),
        };

        let mut rect =
            Self::create_rect(size, metrics.descent, start, end, position, thickness, color);
        rect.kind = kind;
        rects.push(rect);
    }

    fn create_rect(
        size: &SizeInfo,
        descent: f32,
        start: Point<usize>,
        end: Point<usize>,
        position: f32,
        mut thickness: f32,
        color: Rgb,
    ) -> RenderRect {
        let start_x = start.column.0 as f32 * size.cell_width();
        let end_x = (end.column.0 + 1) as f32 * size.cell_width();
        let width = end_x - start_x;

        thickness = thickness.max(1.);

        let line_bottom = (start.line as f32 + 1.) * size.cell_height();
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

const LINE_FLAGS: [Flags; 6] = [
    Flags::UNDERLINE,
    Flags::DOUBLE_UNDERLINE,
    Flags::STRIKEOUT,
    Flags::UNDERCURL,
    Flags::DOTTED_UNDERLINE,
    Flags::DASHED_UNDERLINE,
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

    pub fn update(&mut self, cell: &RenderableCell) {
        self.update_flag(cell, Flags::UNDERLINE);
        self.update_flag(cell, Flags::DOUBLE_UNDERLINE);
        self.update_flag(cell, Flags::STRIKEOUT);
        self.update_flag(cell, Flags::UNDERCURL);
        self.update_flag(cell, Flags::DOTTED_UNDERLINE);
        self.update_flag(cell, Flags::DASHED_UNDERLINE);
    }

    fn update_flag(&mut self, cell: &RenderableCell, flag: Flags) {
        if !cell.flags.contains(flag) {
            return;
        }

        let color = if flag.contains(Flags::STRIKEOUT) { cell.fg } else { cell.underline };

        let mut end = cell.point;
        if cell.flags.contains(Flags::WIDE_CHAR) {
            end.column += 1;
        }

        let lines = &mut self.inner[line_flag_index(flag)];
        if let Some(line) = lines.last_mut()
            && color == line.color
            && cell.point.column == line.end.column + 1
            && cell.point.line == line.end.line
        {
            line.end = end;
            return;
        }

        let line = RenderLine { start: cell.point, end, color };
        lines.push(line);
    }
}

fn line_flag_index(flag: Flags) -> usize {
    match flag {
        Flags::UNDERLINE => 0,
        Flags::DOUBLE_UNDERLINE => 1,
        Flags::STRIKEOUT => 2,
        Flags::UNDERCURL => 3,
        Flags::DOTTED_UNDERLINE => 4,
        Flags::DASHED_UNDERLINE => 5,
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
    let step = (rect.height * 2.0).max(4.0) as f64;

    path.move_to((start_x, mid_y));
    let mut x = start_x;
    let mut up = true;
    while x < end_x {
        let next_x = (x + step).min(end_x);
        let ctrl_x = x + (next_x - x) / 2.0;
        let ctrl_y = if up { mid_y - amplitude } else { mid_y + amplitude };
        path.quad_to((ctrl_x, ctrl_y), (next_x, mid_y));
        x = next_x;
        up = !up;
    }

    scene.stroke(&Stroke::new(rect.height.max(1.0) as f64), Affine::IDENTITY, brush, None, &path);
}
