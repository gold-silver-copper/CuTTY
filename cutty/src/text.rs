use parley::{
    Alignment, AlignmentOptions, FontContext, GenericFamily, Layout, LayoutContext, LineHeight,
    StyleProperty,
};
use vello::peniko::{Brush, Color};
use winit::dpi::PhysicalSize;

use crate::terminal::{DEFAULT_FG, TerminalState, cell_colors};

pub const PADDING_X: f32 = 10.0;
pub const PADDING_Y: f32 = 10.0;
pub const FONT_SIZE: f32 = 18.0;

#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone)]
pub struct ShapedCell {
    pub col: u16,
    pub layout: Layout<Brush>,
}

#[derive(Clone, Default)]
pub struct ShapedRow {
    pub cells: Vec<ShapedCell>,
}

pub struct TextSystem {
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    metrics: CellMetrics,
    rows: Vec<Option<ShapedRow>>,
}

impl TextSystem {
    pub fn new(visible_rows: u16) -> Self {
        let mut font_cx = FontContext::default();
        let mut layout_cx = LayoutContext::default();
        let metrics = measure_cell_metrics(&mut font_cx, &mut layout_cx);

        Self {
            font_cx,
            layout_cx,
            metrics,
            rows: vec![None; visible_rows as usize],
        }
    }

    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    pub fn visible_grid(&self, size: PhysicalSize<u32>) -> (u16, u16) {
        let width = (size.width as f32 - PADDING_X * 2.0).max(self.metrics.width);
        let height = (size.height as f32 - PADDING_Y * 2.0).max(self.metrics.height);
        let cols = (width / self.metrics.width).floor().max(1.0) as u16;
        let rows = (height / self.metrics.height).floor().max(1.0) as u16;
        (cols, rows)
    }

    pub fn resize_cache(&mut self, rows: u16) {
        self.rows.resize(rows as usize, None);
    }

    pub fn sync_terminal_rows(&mut self, terminal: &TerminalState, dirty_rows: &[usize]) {
        let (rows, _) = terminal.size();
        self.resize_cache(rows);

        for &row_index in dirty_rows {
            if row_index < rows as usize {
                self.rows[row_index] = Some(self.shape_row(terminal, row_index as u16));
            }
        }

        for row_index in 0..rows as usize {
            if self.rows[row_index].is_none() {
                self.rows[row_index] = Some(self.shape_row(terminal, row_index as u16));
            }
        }
    }

    pub fn row(&self, row: usize) -> Option<&ShapedRow> {
        self.rows.get(row).and_then(Option::as_ref)
    }

    fn shape_row(&mut self, terminal: &TerminalState, row: u16) -> ShapedRow {
        let (_, cols) = terminal.size();
        let mut shaped_cells = Vec::new();

        for col in 0..cols {
            let Some(cell) = terminal.cell(row, col) else {
                continue;
            };
            if cell.is_wide_continuation() {
                continue;
            }

            let contents = if cell.has_contents() {
                cell.contents()
            } else {
                ""
            };

            if !contents.is_empty() && !contents.chars().all(char::is_whitespace) {
                let colors = cell_colors(Some(cell));
                let layout = self.shape_cell(contents, colors.fg);
                shaped_cells.push(ShapedCell { col, layout });
            }
        }

        ShapedRow {
            cells: shaped_cells,
        }
    }

    fn shape_cell(&mut self, text: &str, fg: crate::terminal::Rgb) -> Layout<Brush> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(GenericFamily::Monospace);
        builder.push_default(StyleProperty::FontSize(FONT_SIZE));
        builder.push_default(LineHeight::Absolute(self.metrics.height));
        builder.push_default(StyleProperty::Brush(Brush::Solid(color_from_rgb(
            DEFAULT_FG,
        ))));
        builder.push_default(StyleProperty::Brush(Brush::Solid(color_from_rgb(fg))));

        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());
        layout
    }
}

fn measure_cell_metrics(
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<Brush>,
) -> CellMetrics {
    let sample = "M";
    let mut builder = layout_cx.ranged_builder(font_cx, sample, 1.0, true);
    builder.push_default(GenericFamily::Monospace);
    builder.push_default(StyleProperty::FontSize(FONT_SIZE));
    builder.push_default(LineHeight::FontSizeRelative(1.25));
    builder.push_default(StyleProperty::Brush(Brush::Solid(Color::WHITE)));

    let mut layout = builder.build(sample);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());

    let line = layout.lines().next().expect("sample line");
    CellMetrics {
        width: layout.full_width().max(1.0).ceil(),
        height: line.metrics().line_height.max(FONT_SIZE).ceil(),
    }
}

fn color_from_rgb(color: crate::terminal::Rgb) -> Color {
    Color::from_rgb8(color.r, color.g, color.b)
}
