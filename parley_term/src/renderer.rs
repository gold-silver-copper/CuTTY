use std::sync::Arc;

use pollster::block_on;
use vello::peniko::Color;
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};
use winit::dpi::PhysicalSize;
use winit::window::Window as WinitWindow;

#[derive(Debug)]
pub enum Error {
    CreateSurface(vello::Error),
    CreateRenderer(vello::Error),
    Render(vello::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateSurface(err) => write!(f, "failed to create render surface: {err}"),
            Self::CreateRenderer(err) => write!(f, "failed to create renderer: {err}"),
            Self::Render(err) => write!(f, "failed to render scene: {err}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct SceneRenderer {
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    surface: Box<RenderSurface<'static>>,
    valid_surface: bool,
    max_surface_dimension: u32,
}

impl SceneRenderer {
    pub fn new(window: Arc<WinitWindow>, size: PhysicalSize<u32>) -> Result<Self, Error> {
        let mut context = RenderContext::new();
        let size = clamp_render_size(size, wgpu::Limits::default().max_texture_dimension_2d);
        let surface = block_on(context.create_surface(
            window,
            size.width.max(1),
            size.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .map_err(Error::CreateSurface)?;

        let max_surface_dimension =
            context.devices[surface.dev_id].device.limits().max_texture_dimension_2d;

        let mut renderers = Vec::new();
        renderers.resize_with(context.devices.len(), || None);
        renderers[surface.dev_id] = Some(
            Renderer::new(&context.devices[surface.dev_id].device, RendererOptions {
                antialiasing_support: [AaConfig::Msaa8].into_iter().collect::<AaSupport>(),
                ..RendererOptions::default()
            })
            .map_err(Error::CreateRenderer)?,
        );

        Ok(Self {
            context,
            renderers,
            surface: Box::new(surface),
            valid_surface: true,
            max_surface_dimension,
        })
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let size = self.clamp_render_size(size);
        if size.width == 0 || size.height == 0 {
            self.valid_surface = false;
            return;
        }

        self.context.resize_surface(&mut self.surface, size.width, size.height);
        self.valid_surface = true;
    }

    pub fn clamp_render_size(&self, size: PhysicalSize<u32>) -> PhysicalSize<u32> {
        clamp_render_size(size, self.max_surface_dimension)
    }

    pub fn render(&mut self, scene: &Scene, base_color: Color) -> Result<(), Error> {
        if !self.valid_surface {
            return Ok(());
        }

        let width = self.surface.config.width;
        let height = self.surface.config.height;
        let device_handle = &self.context.devices[self.surface.dev_id];
        let renderer = self.renderers[self.surface.dev_id].as_mut().expect("renderer initialized");

        renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                scene,
                &self.surface.target_view,
                &RenderParams { base_color, width, height, antialiasing_method: AaConfig::Msaa8 },
            )
            .map_err(Error::Render)?;

        let surface_texture = match self.surface.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(_) => {
                self.context.resize_surface(&mut self.surface, width.max(1), height.max(1));
                return Ok(());
            },
        };

        let surface_view =
            surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            device_handle.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("parley_term.vello.surface_blit"),
            });
        self.surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &self.surface.target_view,
            &surface_view,
        );
        device_handle.queue.submit([encoder.finish()]);
        surface_texture.present();
        let _ = device_handle.device.poll(wgpu::PollType::Poll);

        Ok(())
    }
}

fn clamp_render_size(size: PhysicalSize<u32>, max_dimension: u32) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.min(max_dimension), size.height.min(max_dimension))
}
