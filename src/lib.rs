use std::sync::Arc;

use anyhow::Context;
use winit::{
    application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop, window::{Window, WindowAttributes}
};

pub struct App {
    canvas: Option<Canvas>,
}

impl App {
    pub fn new() -> Self {
        Self { canvas: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = WindowAttributes::default();
        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());
        self.canvas = Some(pollster::block_on(Canvas::new(window)).unwrap());
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let canvas = match &mut self.canvas {
            Some(canvas) => canvas,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => canvas.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                canvas.render(event_loop);
            }
            _ => {}
        }
    }
}

pub struct Canvas {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    #[allow(unused)]
    window: Arc<Window>,
}

impl Canvas {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(Default::default());
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .with_context(|| "No compatible adapter")?;
        let (device, queue) = adapter.request_device(&Default::default(), None).await?;

        let mut config = surface
            .get_default_config(
                &adapter,
                window.inner_size().width,
                window.inner_size().height,
            )
            .with_context(|| "Surface is invalid")?;
        config.view_formats.push(config.format.add_srgb_suffix());

        surface.configure(&device, &config);

        Ok(Self {
            config,
            surface,
            device,
            queue,
            window,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, event_loop: &ActiveEventLoop) {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated) => return,
            Err(e) => {
                eprintln!("{e}");
                event_loop.exit();
                return;
            }
        };

        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }

        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}
