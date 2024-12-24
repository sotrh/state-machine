mod data;
mod resources;
mod utils;

use std::sync::Arc;

use anyhow::Context;
use data::Line;
use resources::buffer::BackedBuffer;
use utils::RenderPipelineBuilder;
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{ModifiersKeyState, ModifiersState},
    window::Window,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub const CANVAS_ID: &str = "canvas";

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<Canvas>>,
    canvas: Option<Canvas>,
    current_line: Option<Line>,
    shift_pressed: bool,
    cursor: glam::Vec2,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<Canvas>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            cursor: glam::vec2(0.0, 0.0),
            canvas: None,
            current_line: None,
            shift_pressed: false,
            #[cfg(target_arch = "wasm32")]
            proxy,
        }
    }
}

impl ApplicationHandler<Canvas> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_element = canvas.unchecked_into();
            window_attributes = window_attributes.with_canvas(Some(html_canvas_element));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.canvas = Some(pollster::block_on(Canvas::new(window)).unwrap());
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    assert!(proxy
                        .send_event(
                            Canvas::new(window)
                                .await
                                .expect("Unable to create canvas!!!")
                        )
                        .is_ok())
                });
            }
        }
    }

    #[allow(unused_mut)]
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut event: Canvas) {
        #[cfg(target_arch = "wasm32")]
        if event.is_webgpu_supported {
            event.window.request_redraw();
            event.resize(
                event.window.inner_size().width,
                event.window.inner_size().height,
            );
        }
        self.canvas = Some(event);
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
            WindowEvent::ModifiersChanged(mods) => {
                self.shift_pressed = mods.lshift_state() == ModifiersKeyState::Pressed
                    || mods.rshift_state() == ModifiersKeyState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = canvas.project_point(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state.is_pressed()) {
                (MouseButton::Left, true) => {
                    if let Some(line) = &mut self.current_line {
                        line.b = self.cursor;
                    } else {
                        self.current_line = Some(Line {
                            a: self.cursor,
                            b: self.cursor,
                        });
                    }
                }
                (MouseButton::Left, false) => {
                    if let Some(mut line) = self.current_line.take() {
                        line.b = self.cursor;
                        canvas.finish_line(line);
                    }
                }
                // _ => {}
                state => {
                    println!("{state:?}");
                }
            },
            _ => {}
        }
    }
}

pub struct Canvas {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    fullscreen_quad: wgpu::RenderPipeline,
    #[cfg(target_arch = "wasm32")]
    is_webgpu_supported: bool,
    #[allow(unused)]
    window: Arc<Window>,
    lines: BackedBuffer<Line>,
    geometry_bind_group: wgpu::BindGroup,
}

impl Canvas {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        #[allow(unused_mut)]
        let mut backends = wgpu::Backends::all();
        #[cfg(target_arch = "wasm32")]
        let is_webgpu_supported = wgpu::util::is_browser_webgpu_supported().await;
        #[cfg(target_arch = "wasm32")]
        if !is_webgpu_supported {
            backends = wgpu::Backends::GL;
        }
        log::info!("Backends: {backends:?}");
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        log::info!("Creating surface");
        let surface = instance.create_surface(window.clone())?;
        log::info!("Requesting adapter");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .with_context(|| "No compatible adapter")?;
        let device_request = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    #[cfg(target_arch = "wasm32")]
                    required_limits: if is_webgpu_supported {
                        log::info!("Using webgpu");
                        wgpu::Limits::downlevel_defaults()
                    } else {
                        log::info!("Using webgl");
                        wgpu::Limits::downlevel_webgl2_defaults()
                    },
                    ..Default::default()
                },
                None,
            )
            .await;
        log::info!("Requesting device");
        #[cfg(not(target_arch = "wasm32"))]
        let (device, queue) = device_request?;
        #[cfg(target_arch = "wasm32")]
        let (device, queue) = device_request.unwrap_throw();

        let mut config = surface
            .get_default_config(
                &adapter,
                window.inner_size().width,
                window.inner_size().height,
            )
            .with_context(|| "Surface is invalid")?;
        config.view_formats.push(config.format.add_srgb_suffix());

        #[cfg(not(target_arch = "wasm32"))]
        surface.configure(&device, &config);

        let lines: BackedBuffer<Line> = BackedBuffer::with_capacity(&device, 16, wgpu::BufferUsages::STORAGE);
        
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let fullscreen_quad = RenderPipelineBuilder::new()
            .vertex(wgpu::VertexState {
                module: &shader,
                entry_point: Some("fullscreen_quad"),
                compilation_options: Default::default(),
                buffers: &[],
            })
            .fragment(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("textured"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            })
            .build(&device)?;

        let geometry_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("geometry"),
            layout: &fullscreen_quad.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: lines.buffer().as_entire_binding(),
                }
            ]
        });

        Ok(Self {
            config,
            surface,
            device,
            queue,
            lines,
            geometry_bind_group,
            fullscreen_quad,
            #[cfg(target_arch = "wasm32")]
            is_webgpu_supported,
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
            Err(wgpu::SurfaceError::Outdated) => {
                return;
            }
            Err(e) => {
                log::error!("{e}");
                event_loop.exit();
                return;
            }
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: self.config.view_formats.get(0).copied(),
            ..Default::default()
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

            pass.set_pipeline(&self.fullscreen_quad);
            pass.set_bind_group(0, &self.geometry_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        self.queue.submit([encoder.finish()]);
        frame.present();
    }

    pub fn finish_line(&mut self, line: Line) {
        self.lines.batch(&self.device, &self.queue).push(line);
        self.window.request_redraw();
    }
    
    pub fn project_point(&self, x: f32, y: f32) -> glam::Vec2 {
        glam::vec2(
            x / self.config.width.max(1) as f32,
            1.0 - y / self.config.height.max(1) as f32,
        )
    }
}

pub fn run() -> anyhow::Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_log::init_with_level(log::Level::Info).unwrap_throw();
    }

    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App::new(
        #[cfg(target_arch = "wasm32")]
        &event_loop,
    );
    event_loop.run_app(&mut app)?;

    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn run_web() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    run().unwrap_throw();

    Ok(())
}
