mod data;
mod resources;
mod utils;

use std::sync::Arc;

use anyhow::Context;
use data::{DrawMode, GeometryInfo, Line};
use resources::buffer::BackedBuffer;
use utils::RenderPipelineBuilder;
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, ModifiersKeyState, PhysicalKey},
    window::Window,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub const CANVAS_ID: &str = "canvas";

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<Canvas>>,
    canvas: Option<Canvas>,
    current_line: Line,
    drawing: bool,
    shift_pressed: bool,
    cursor: glam::Vec2,
    mode: DrawMode,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<Canvas>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            cursor: glam::vec2(0.0, 0.0),
            canvas: None,
            current_line: Line {
                a: glam::vec2(0.0, 0.0),
                b: glam::vec2(0.0, 0.0),
            },
            mode: DrawMode::Color,
            drawing: false,
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
        {
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
                self.current_line.b = self.cursor;
                if !self.drawing {
                    self.current_line.a = self.cursor;
                }
                canvas.preview_line(self.current_line);
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state.is_pressed()) {
                (MouseButton::Left, true) => {
                    self.drawing = true;
                }
                (MouseButton::Left, false) => {
                    self.drawing = false;
                    canvas.finish_line(self.current_line);
                }
                _ => {}
            },
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        ..
                    },
                ..
            } => match (code, state.is_pressed()) {
                (KeyCode::Escape, true) => event_loop.exit(),
                (KeyCode::Space, true) => {
                    canvas.set_mode(self.mode.increment());
                }
                _ => {}
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
    #[allow(unused)]
    window: Arc<Window>,
    lines: BackedBuffer<Line>,
    current_lines_version: u32,
    geometry_bind_group: wgpu::BindGroup,
    geo_info: BackedBuffer<GeometryInfo>,
}

impl Canvas {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        #[allow(unused_mut)]
        let mut backends = wgpu::Backends::all();
        #[cfg(target_arch = "wasm32")]
        let is_webgpu_supported = wgpu::util::is_browser_webgpu_supported().await;
        #[cfg(target_arch = "wasm32")]
        if !is_webgpu_supported {
            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let h1 = document.get_element_by_id("error").unwrap_throw()
                .dyn_into::<wgpu::web_sys::HtmlElement>()
                .unwrap_throw();

            h1.set_class_name("revealed");

            anyhow::bail!("This example requires WebGPU");
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
                    required_limits: wgpu::Limits::downlevel_defaults(),
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

        let lines: BackedBuffer<Line> =
            BackedBuffer::with_capacity(&device, 16, wgpu::BufferUsages::STORAGE);
        let geo_info = BackedBuffer::with_data(
            &device,
            vec![GeometryInfo::new(lines.len(), data::DrawMode::Color, config.width, config.height)],
            wgpu::BufferUsages::UNIFORM,
        );

        log::info!("Creating canvas pipeline");
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
                entry_point: Some("canvas"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.view_formats[0],
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            })
            .build(&device)?;

        log::info!("Creating geometry bind group");
        let geometry_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("geometry"),
            layout: &fullscreen_quad.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: geo_info.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lines.buffer().as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            config,
            surface,
            device,
            queue,
            current_lines_version: lines.version(),
            geo_info,
            lines,
            geometry_bind_group,
            fullscreen_quad,
            window,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.geo_info.update(&self.queue, |data| data[0].resize(width, height));
    }

    pub fn render(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("Render");
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

    pub fn project_point(&self, x: f32, y: f32) -> glam::Vec2 {
        let aspect_ratio = self.config.width as f32 / self.config.height as f32;
        glam::vec2(
            x / self.config.width.max(1) as f32 * aspect_ratio,
            1.0 - y / self.config.height.max(1) as f32,
        )
    }

    pub fn finish_line(&mut self, line: Line) {
        {
            self.lines.batch(&self.device, &self.queue).push(line);
            self.geo_info.update(&self.queue, |data| {
                data[0].num_lines += 1;
            });
        }

        if self.lines.version() != self.current_lines_version {
            self.geometry_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("lines"),
                layout: &self.fullscreen_quad.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.geo_info.buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.lines.buffer().as_entire_binding(),
                    },
                ],
            });
        }

        self.window.request_redraw();
    }

    pub fn preview_line(&mut self, preview_line: Line) {
        self.geo_info.update(&self.queue, |data| {
            data[0].preview_line = preview_line;
        });
        self.window.request_redraw();
    }

    pub fn set_mode(&mut self, mode: DrawMode) {
        self.geo_info
            .update(&self.queue, |data| data[0].mode = mode);
        self.window.request_redraw();
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
