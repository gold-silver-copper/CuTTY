use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use parley::layout::PositionedLayoutItem;
use vello::{
    AaConfig, Renderer, RendererOptions, Scene,
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
    util::{RenderContext, RenderSurface},
    wgpu,
};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::Window,
};

use crate::{
    selection::SelectionState,
    terminal::{CURSOR_COLOR, DEFAULT_BG, TerminalState, cell_colors},
    text::{PADDING_X, PADDING_Y, TextSystem},
};

pub struct GpuWindow {
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    surface: Box<RenderSurface<'static>>,
    scene: Scene,
    valid_surface: bool,
    max_surface_dimension: u32,
    pub window: Arc<Window>,
}

impl GpuWindow {
    pub fn new(event_loop: &ActiveEventLoop, title: &str, size: (u32, u32)) -> Result<Self> {
        let default_limit = wgpu::Limits::default().max_texture_dimension_2d;
        let requested_size = PhysicalSize::new(size.0, size.1);
        let clamped_size = clamp_render_size(requested_size, default_limit);
        let window = create_window(event_loop, title, (clamped_size.width, clamped_size.height));
        let mut context = RenderContext::new();
        let surface = pollster::block_on(context.create_surface(
            window.clone(),
            clamped_size.width,
            clamped_size.height,
            wgpu::PresentMode::AutoVsync,
        ))
        .context("failed to create Vello surface")?;
        let max_surface_dimension = context.devices[surface.dev_id]
            .device
            .limits()
            .max_texture_dimension_2d;

        let mut renderers = Vec::new();
        renderers.resize_with(context.devices.len(), || None);
        renderers[surface.dev_id] = Some(
            Renderer::new(
                &context.devices[surface.dev_id].device,
                RendererOptions::default(),
            )
            .map_err(|error| anyhow!("failed to create renderer: {error}"))?,
        );

        Ok(Self {
            context,
            renderers,
            surface: Box::new(surface),
            scene: Scene::new(),
            valid_surface: true,
            max_surface_dimension,
            window,
        })
    }

    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.clamp_render_size(self.window.inner_size())
    }

    pub fn max_render_size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.max_surface_dimension, self.max_surface_dimension)
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let size = self.clamp_render_size(size);
        if size.width == 0 || size.height == 0 {
            self.valid_surface = false;
            return;
        }

        if self.window.inner_size() != size {
            let _ = self.window.request_inner_size(size);
        }

        self.context
            .resize_surface(&mut self.surface, size.width, size.height);
        self.valid_surface = true;
    }

    pub fn request_inner_size(&self, size: PhysicalSize<u32>) -> PhysicalSize<u32> {
        let size = self.clamp_render_size(size);
        let _ = self.window.request_inner_size(size);
        size
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn render_terminal(
        &mut self,
        terminal: &TerminalState,
        text_system: &TextSystem,
        selection: &SelectionState,
    ) -> Result<()> {
        if !self.valid_surface {
            return Ok(());
        }

        self.scene.reset();
        paint_terminal(&mut self.scene, terminal, text_system, selection);

        let width = self.surface.config.width;
        let height = self.surface.config.height;
        let device_handle = &self.context.devices[self.surface.dev_id];
        let renderer = self.renderers[self.surface.dev_id]
            .as_mut()
            .context("renderer was not initialized")?;

        renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                &self.scene,
                &self.surface.target_view,
                &vello::RenderParams {
                    base_color: color_from_rgb(DEFAULT_BG),
                    width,
                    height,
                    antialiasing_method: AaConfig::Msaa16,
                },
            )
            .map_err(|error| anyhow!("failed to render scene: {error}"))?;

        let surface_texture = match self.surface.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(error) => {
                self.context
                    .resize_surface(&mut self.surface, width, height);
                return Err(anyhow!("failed to acquire swapchain texture: {error}"));
            }
        };

        let mut encoder =
            device_handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cutty.surface.blit"),
                });
        self.surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &self.surface.target_view,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device_handle.queue.submit([encoder.finish()]);
        surface_texture.present();
        device_handle.device.poll(wgpu::PollType::Poll).ok();

        Ok(())
    }
}

impl GpuWindow {
    fn clamp_render_size(&self, size: PhysicalSize<u32>) -> PhysicalSize<u32> {
        clamp_render_size(size, self.max_surface_dimension)
    }
}

fn create_window(event_loop: &ActiveEventLoop, title: &str, size: (u32, u32)) -> Arc<Window> {
    let attrs = Window::default_attributes()
        .with_inner_size(LogicalSize::new(size.0, size.1))
        .with_title(title)
        .with_resizable(true);
    Arc::new(
        event_loop
            .create_window(attrs)
            .expect("window creation failed"),
    )
}

fn clamp_render_size(size: PhysicalSize<u32>, max_dimension: u32) -> PhysicalSize<u32> {
    PhysicalSize::new(
        size.width.min(max_dimension),
        size.height.min(max_dimension),
    )
}

fn paint_terminal(
    scene: &mut Scene,
    terminal: &TerminalState,
    text_system: &TextSystem,
    selection: &SelectionState,
) {
    let metrics = text_system.metrics();
    let (rows, cols) = terminal.size();

    for row_index in 0..rows as usize {
        paint_backgrounds(
            scene,
            terminal,
            row_index as u16,
            cols,
            metrics.width,
            metrics.height,
        );
    }
    paint_selection(scene, terminal, selection, metrics.width, metrics.height);

    for row_index in 0..rows as usize {
        if let Some(shaped_row) = text_system.row(row_index) {
            for shaped_cell in &shaped_row.cells {
                let transform = Affine::translate((
                    (PADDING_X + shaped_cell.col as f32 * metrics.width) as f64,
                    (PADDING_Y + row_index as f32 * metrics.height) as f64,
                ));
                for line in shaped_cell.layout.lines() {
                    for item in line.items() {
                        let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                            continue;
                        };
                        let style = glyph_run.style();
                        let run = glyph_run.run();
                        let font = run.font();
                        let font_size = run.font_size();
                        let mut x = glyph_run.offset();
                        let y = glyph_run.baseline();

                        scene
                            .draw_glyphs(font)
                            .brush(&style.brush)
                            .hint(true)
                            .transform(transform)
                            .font_size(font_size)
                            .normalized_coords(run.normalized_coords())
                            .draw(
                                Fill::NonZero,
                                glyph_run.glyphs().map(|glyph| {
                                    let gx = x + glyph.x;
                                    let gy = y + glyph.y;
                                    x += glyph.advance;
                                    vello::Glyph {
                                        id: glyph.id,
                                        x: gx,
                                        y: gy,
                                    }
                                }),
                            );
                    }
                }
            }
        }
    }

    if !terminal.hide_cursor() {
        let (row, col) = terminal.cursor_position();
        let x = PADDING_X + col as f32 * metrics.width;
        let y = PADDING_Y + row as f32 * metrics.height;
        let rect = Rect::new(
            x as f64,
            y as f64,
            (x + metrics.width) as f64,
            (y + metrics.height) as f64,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color_with_alpha(CURSOR_COLOR, 0.35),
            None,
            &rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::clamp_render_size;
    use winit::dpi::PhysicalSize;

    #[test]
    fn render_size_is_clamped_to_gpu_limits() {
        assert_eq!(
            clamp_render_size(PhysicalSize::new(9000, 7000), 8192),
            PhysicalSize::new(8192, 7000)
        );
    }
}

fn paint_selection(
    scene: &mut Scene,
    terminal: &TerminalState,
    selection: &SelectionState,
    cell_width: f32,
    cell_height: f32,
) {
    let (rows, _) = terminal.size();
    for row in 0..rows {
        let Some((start_col, end_col)) = selection.cols_for_visible_row(terminal, row) else {
            continue;
        };

        if end_col <= start_col {
            continue;
        }

        let x = PADDING_X + start_col as f32 * cell_width;
        let y = PADDING_Y + row as f32 * cell_height;
        let rect = Rect::new(
            x as f64,
            y as f64,
            (x + (end_col - start_col) as f32 * cell_width) as f64,
            (y + cell_height) as f64,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0x88, 0xc0, 0xd0, 0x6d),
            None,
            &rect,
        );
    }
}

fn paint_backgrounds(
    scene: &mut Scene,
    terminal: &TerminalState,
    row: u16,
    cols: u16,
    cell_width: f32,
    cell_height: f32,
) {
    let mut start = 0_u16;
    while start < cols {
        let bg = cell_colors(terminal.cell(row, start)).bg;
        let mut end = start + 1;
        while end < cols {
            let next_bg = cell_colors(terminal.cell(row, end)).bg;
            if next_bg != bg {
                break;
            }
            end += 1;
        }

        if bg != DEFAULT_BG {
            let x = PADDING_X + start as f32 * cell_width;
            let y = PADDING_Y + row as f32 * cell_height;
            let rect = Rect::new(
                x as f64,
                y as f64,
                (x + (end - start) as f32 * cell_width) as f64,
                (y + cell_height) as f64,
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                color_from_rgb(bg),
                None,
                &rect,
            );
        }

        start = end;
    }
}

fn color_from_rgb(color: crate::terminal::Rgb) -> Color {
    Color::from_rgb8(color.r, color.g, color.b)
}

fn color_with_alpha(color: crate::terminal::Rgb, alpha: f32) -> Color {
    let mut rgba = color_from_rgb(color).to_rgba8();
    rgba.a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_rgba8(rgba.r, rgba.g, rgba.b, rgba.a)
}
