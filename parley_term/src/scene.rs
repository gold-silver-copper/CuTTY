use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};
use vello::{Glyph, Scene};

use crate::color::{Rgb, color_from_rgb};
use crate::grid::{CursorShape, SceneCursor, SizeInfo, TerminalCell, TerminalGrid};
use crate::rects::{RenderRect, RenderLines, paint_rect, paint_rects};
use crate::text::{TextMetrics, TextSystem};

#[derive(Debug, Clone)]
pub struct SceneFrame {
    pub size: SizeInfo,
    pub background: Rgb,
    pub grid: TerminalGrid,
    pub overlays: Vec<RenderRect>,
}

pub struct SceneBuilder {
    text_system: TextSystem,
}

impl SceneBuilder {
    pub fn new(text_system: TextSystem) -> Self {
        Self { text_system }
    }

    pub fn text_system(&self) -> &TextSystem {
        &self.text_system
    }

    pub fn text_system_mut(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    pub fn build_scene(&mut self, frame: &SceneFrame) -> Scene {
        let mut scene = Scene::new();
        self.paint_background(&mut scene, frame.size, frame.background);

        let mut lines = RenderLines::new();
        for cell in &frame.grid.cells {
            self.paint_cell_background(&mut scene, cell, frame.size);
            self.paint_cell_text(&mut scene, frame.size, cell);
            lines.update(cell);
        }

        paint_rects(&mut scene, lines.rects(&self.text_system.metrics(), &frame.size));

        if let Some(cursor) = frame.grid.cursor {
            for rect in cursor_rects(cursor, frame.size, self.text_system.metrics()) {
                paint_rect(&mut scene, &rect);
            }
        }

        paint_rects(&mut scene, frame.overlays.iter().copied());
        scene
    }

    fn paint_background(&self, scene: &mut Scene, size: SizeInfo, background: Rgb) {
        let rect = RenderRect::new(0.0, 0.0, size.width(), size.height(), background, 1.0);
        paint_rect(scene, &rect);
    }

    fn paint_cell_background(&self, scene: &mut Scene, cell: &TerminalCell, size: SizeInfo) {
        if cell.bg_alpha <= 0.0 {
            return;
        }

        let rect = RenderRect::new(
            size.padding_x() + cell.column as f32 * size.cell_width(),
            size.padding_y() + cell.row as f32 * size.cell_height(),
            size.cell_width() * cell.width.get() as f32,
            size.cell_height(),
            cell.bg,
            cell.bg_alpha,
        );
        paint_rect(scene, &rect);
    }

    fn paint_cell_text(&mut self, scene: &mut Scene, size: SizeInfo, cell: &TerminalCell) {
        let Some(layout) = self.text_system.shape_cell(cell) else {
            return;
        };

        paint_layout(
            scene,
            &layout,
            self.text_system.metrics(),
            size,
            cell.row,
            cell.column,
            cell.fg,
        );
    }
}

fn paint_layout(
    scene: &mut Scene,
    layout: &parley::Layout<()>,
    metrics: TextMetrics,
    size: SizeInfo,
    row: usize,
    column: usize,
    fg: Rgb,
) {
    let transform = Affine::translate((
        (size.padding_x() + column as f32 * size.cell_width() + metrics.glyph_offset_x) as f64,
        (size.padding_y() + row as f32 * size.cell_height() + metrics.glyph_offset_y) as f64,
    ));
    let brush = Brush::Solid(color_from_rgb(fg));

    for line in layout.lines() {
        for item in line.items() {
            let parley::layout::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };

            let run = glyph_run.run();
            let font = run.font();
            let font_size = run.font_size();
            let mut x = glyph_run.offset();
            let y = glyph_run.baseline();

            scene
                .draw_glyphs(font)
                .brush(&brush)
                .hint(false)
                .transform(transform)
                .font_size(font_size)
                .normalized_coords(run.normalized_coords())
                .draw(
                    Fill::NonZero,
                    glyph_run.glyphs().map(|glyph| scene_glyph_from_layout(&mut x, y, glyph)),
                );
        }
    }
}

fn cursor_rects(cursor: SceneCursor, size: SizeInfo, metrics: TextMetrics) -> Vec<RenderRect> {
    let x = size.padding_x() + cursor.column as f32 * size.cell_width();
    let y = size.padding_y() + cursor.row as f32 * size.cell_height();
    let width = size.cell_width() * cursor.width.get() as f32;
    let height = size.cell_height();

    match cursor.shape {
        CursorShape::Hidden => Vec::new(),
        CursorShape::Block => vec![RenderRect::new(x, y, width, height, cursor.color, 1.0)],
        CursorShape::HollowBlock => vec![
            RenderRect::new(x, y, width, 1.0, cursor.color, 1.0),
            RenderRect::new(x, y + height - 1.0, width, 1.0, cursor.color, 1.0),
            RenderRect::new(x, y, 1.0, height, cursor.color, 1.0),
            RenderRect::new(x + width - 1.0, y, 1.0, height, cursor.color, 1.0),
        ],
        CursorShape::Beam => vec![RenderRect::new(x, y, 1.0, height, cursor.color, 1.0)],
        CursorShape::Underline => vec![RenderRect::new(
            x,
            y + height - metrics.underline_thickness.max(1.0),
            width,
            metrics.underline_thickness.max(1.0),
            cursor.color,
            1.0,
        )],
    }
}

fn scene_glyph_from_layout(
    cursor_x: &mut f32,
    baseline: f32,
    glyph: parley::layout::Glyph,
) -> Glyph {
    let positioned = Glyph { id: glyph.id, x: *cursor_x + glyph.x, y: baseline - glyph.y };
    *cursor_x += glyph.advance;
    positioned
}

#[cfg(test)]
mod tests {
    use super::scene_glyph_from_layout;

    #[test]
    fn scene_glyphs_use_baseline_relative_y_coordinates() {
        let mut cursor_x = 10.0;
        let glyph = parley::layout::Glyph { id: 42, style_index: 0, x: 1.5, y: 2.0, advance: 8.0 };

        let positioned = scene_glyph_from_layout(&mut cursor_x, 20.0, glyph);

        assert_eq!(positioned.id, 42);
        assert_eq!(positioned.x, 11.5);
        assert_eq!(positioned.y, 18.0);
        assert_eq!(cursor_x, 18.0);
    }
}
