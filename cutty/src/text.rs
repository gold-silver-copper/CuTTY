use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, Layout, LayoutContext, LineHeight,
    StyleProperty,
};
use vello::peniko::{Brush, Color};
use winit::dpi::PhysicalSize;

use crate::{
    config::FontConfig,
    terminal::{DEFAULT_FG, TerminalState, VisibleLineInfo, cell_colors},
};

pub const PADDING_X: f32 = 10.0;
pub const PADDING_Y: f32 = 10.0;
pub const MIN_FONT_SIZE: f32 = 8.0;
pub const MAX_FONT_SIZE: f32 = 72.0;

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

#[derive(Clone)]
struct CachedRow {
    stable_row: usize,
    seqno: u64,
    shaped: ShapedRow,
}

pub struct TextSystem {
    font: FontConfig,
    font_family: FontFamily<'static>,
    font_cx: FontContext,
    layout_cx: LayoutContext<Brush>,
    metrics: CellMetrics,
    rows: Vec<Option<CachedRow>>,
}

impl TextSystem {
    pub fn new(visible_rows: u16, font: &FontConfig) -> Self {
        let font = font.clone();
        let font_family = font.family_stack();
        let mut font_cx = FontContext::default();
        let mut layout_cx = LayoutContext::default();
        let metrics = measure_cell_metrics(&mut font_cx, &mut layout_cx, &font, &font_family);

        Self {
            font,
            font_family,
            font_cx,
            layout_cx,
            metrics,
            rows: vec![None; visible_rows as usize],
        }
    }

    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    pub fn adjust_font_size(&mut self, delta: f32) -> bool {
        let new_size = (self.font.size + delta).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.set_font_size(new_size)
    }

    pub fn adjust_font_size_to_window(
        &mut self,
        delta: f32,
        cols: u16,
        rows: u16,
        max_window_size: PhysicalSize<u32>,
    ) -> bool {
        let requested = (self.font.size + delta).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let new_size = if delta > 0.0 {
            self.max_font_size_for_grid(cols, rows, max_window_size, requested)
        } else {
            requested
        };
        self.set_font_size(new_size)
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

    fn invalidate_rows(&mut self) {
        self.rows.iter_mut().for_each(|row| *row = None);
    }

    pub fn sync_terminal_rows(&mut self, terminal: &TerminalState) {
        let (rows, _) = terminal.size();
        self.resize_cache(rows);

        for row_index in 0..rows as usize {
            let visible_row = row_index as u16;
            let Some(line_info) = terminal.visible_line_info(visible_row) else {
                self.rows[row_index] = None;
                continue;
            };

            if !self.cached_row_matches(row_index, line_info) {
                self.rows[row_index] = Some(CachedRow {
                    stable_row: line_info.stable_row,
                    seqno: line_info.seqno,
                    shaped: self.shape_row(terminal, visible_row),
                });
            }
        }
    }

    pub fn row(&self, row: usize) -> Option<&ShapedRow> {
        self.rows
            .get(row)
            .and_then(Option::as_ref)
            .map(|cached| &cached.shaped)
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
        builder.push_default(StyleProperty::FontFamily(self.font_family.clone()));
        builder.push_default(StyleProperty::FontSize(self.font.size));
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

    fn cached_row_matches(&self, row: usize, line_info: VisibleLineInfo) -> bool {
        self.rows
            .get(row)
            .and_then(Option::as_ref)
            .is_some_and(|cached| {
                cached.stable_row == line_info.stable_row && cached.seqno == line_info.seqno
            })
    }

    fn set_font_size(&mut self, size: f32) -> bool {
        if (size - self.font.size).abs() < f32::EPSILON {
            return false;
        }

        self.font.size = size;
        self.metrics = self.measure_metrics_for_size(size);
        self.invalidate_rows();
        true
    }

    fn max_font_size_for_grid(
        &mut self,
        cols: u16,
        rows: u16,
        max_window_size: PhysicalSize<u32>,
        requested: f32,
    ) -> f32 {
        if self.grid_fits_window(cols, rows, max_window_size, requested) {
            return requested;
        }

        let mut low = self.font.size;
        let mut high = requested;
        for _ in 0..16 {
            let mid = ((low + high) / 2.0).floor().max(low);
            if (high - low).abs() < 0.01 || (mid - low).abs() < 0.01 {
                break;
            }

            if self.grid_fits_window(cols, rows, max_window_size, mid) {
                low = mid;
            } else {
                high = mid;
            }
        }

        low
    }

    fn grid_fits_window(
        &mut self,
        cols: u16,
        rows: u16,
        max_window_size: PhysicalSize<u32>,
        font_size: f32,
    ) -> bool {
        let metrics = self.measure_metrics_for_size(font_size);
        let required_size = grid_pixel_size(cols, rows, metrics);
        required_size.width <= max_window_size.width
            && required_size.height <= max_window_size.height
    }

    fn measure_metrics_for_size(&mut self, font_size: f32) -> CellMetrics {
        let mut font = self.font.clone();
        font.size = font_size;
        measure_cell_metrics(
            &mut self.font_cx,
            &mut self.layout_cx,
            &font,
            &self.font_family,
        )
    }
}

pub(crate) fn grid_pixel_size(cols: u16, rows: u16, metrics: CellMetrics) -> PhysicalSize<u32> {
    let width = (cols as f32 * metrics.width + PADDING_X * 2.0).ceil() as u32;
    let height = (rows as f32 * metrics.height + PADDING_Y * 2.0).ceil() as u32;
    PhysicalSize::new(width, height)
}

fn measure_cell_metrics(
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<Brush>,
    font: &FontConfig,
    font_family: &FontFamily<'static>,
) -> CellMetrics {
    let sample = "M";
    let mut builder = layout_cx.ranged_builder(font_cx, sample, 1.0, true);
    builder.push_default(StyleProperty::FontFamily(font_family.clone()));
    builder.push_default(StyleProperty::FontSize(font.size));
    builder.push_default(LineHeight::FontSizeRelative(font.line_height));
    builder.push_default(StyleProperty::Brush(Brush::Solid(Color::WHITE)));

    let mut layout = builder.build(sample);
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());

    let line = layout.lines().next().expect("sample line");
    CellMetrics {
        width: layout.full_width().max(1.0).ceil(),
        height: line.metrics().line_height.max(font.size).ceil(),
    }
}

fn color_from_rgb(color: crate::terminal::Rgb) -> Color {
    Color::from_rgb8(color.r, color.g, color.b)
}

#[cfg(test)]
mod tests {
    use super::{MAX_FONT_SIZE, MIN_FONT_SIZE, TextSystem, grid_pixel_size};
    use crate::config::FontConfig;
    use winit::dpi::PhysicalSize;

    #[test]
    fn font_size_adjustment_recomputes_metrics() {
        let font = FontConfig::default();
        let mut text = TextSystem::new(4, &font);
        let original = text.metrics();

        assert!(text.adjust_font_size(2.0));

        let updated = text.metrics();
        assert!(updated.width >= original.width);
        assert!(updated.height >= original.height);
    }

    #[test]
    fn font_size_adjustment_respects_bounds() {
        let font = FontConfig::default();
        let mut text = TextSystem::new(4, &font);

        assert!(text.adjust_font_size(MIN_FONT_SIZE - 100.0));
        assert!(!text.adjust_font_size(-1.0));

        assert!(text.adjust_font_size(MAX_FONT_SIZE));
        assert!(!text.adjust_font_size(1.0));
    }

    #[test]
    fn font_growth_stops_at_window_limit() {
        let font = FontConfig::default();
        let mut text = TextSystem::new(24, &font);
        let limit = grid_pixel_size(80, 24, text.metrics());

        assert!(!text.adjust_font_size_to_window(
            1.0,
            80,
            24,
            PhysicalSize::new(limit.width, limit.height)
        ));
    }
}
