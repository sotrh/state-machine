mod resources;
mod utils;

use std::sync::Arc;

use anyhow::Context;
use resources::{
    camera::{CameraBinder, OrthoCamera},
    font::{Font, TexturedVertex},
    Resources,
};
use utils::RenderPipelineBuilder;
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use winit::{
    application::ApplicationHandler,
    event::{KeyEvent, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub const CANVAS_ID: &str = "canvas";

pub struct App {
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<Canvas>>,
    canvas: Option<Canvas>,
}

impl App {
    pub fn new(#[cfg(target_arch = "wasm32")] event_loop: &EventLoop<Canvas>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let proxy = Some(event_loop.create_proxy());
        Self {
            canvas: None,
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
            WindowEvent::ModifiersChanged(mods) => {}
            WindowEvent::CursorMoved { position, .. } => {}
            WindowEvent::MouseInput { state, button, .. } => match (button, state.is_pressed()) {
                (MouseButton::Left, true) => {}
                (MouseButton::Left, false) => {}
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
                (KeyCode::Space, true) => {}
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
    font: Font,
    #[allow(unused)]
    window: Arc<Window>,
    font_atlas: wgpu::BindGroup,
    text_vb: wgpu::Buffer,
    text_ib: wgpu::Buffer,
    textured: wgpu::RenderPipeline,
    camera: OrthoCamera,
    camera_binding: resources::camera::CameraBinding,
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
            let h1 = document
                .get_element_by_id("error")
                .unwrap_throw()
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

        let camera = OrthoCamera::new(
            0.0,
            window.inner_size().width as f32,
            window.inner_size().height as f32,
            0.0,
        );
        let camera_binder = CameraBinder::new(&device);
        let camera_binding = camera_binder.bind(&device, &camera);

        let texture_bindgroup_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bindgroup_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[&texture_bindgroup_layout, camera_binder.layout()],
            push_constant_ranges: &[],
        });

        let textured = RenderPipelineBuilder::new()
            .layout(&pipeline_layout)
            .vertex(wgpu::VertexState {
                module: &shader,
                entry_point: Some("textured"),
                compilation_options: Default::default(),
                buffers: &[TexturedVertex::VB_DESC],
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

        let res = Resources::new("res");

        let font = Font::load(&res, "OpenSans MSDF.zip", &device, &queue)?;

        let glyph = font.glyph('M').unwrap();
        let tex_width = font.texture.width() as f32;
        let tex_height = font.texture.height() as f32;
        let min_uv = glam::vec2(glyph.x as f32 / tex_width, glyph.y as f32 / tex_height);
        let max_uv = min_uv
            + glam::vec2(
                glyph.width as f32 / tex_width,
                glyph.height as f32 / tex_height,
            );
        // let p =

        let text_vb = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("text_vb"),
            contents: bytemuck::cast_slice(&[
                TexturedVertex {
                    position: glam::vec2(0.0, 0.0),
                    uv: glam::vec2(min_uv.x, min_uv.y),
                },
                TexturedVertex {
                    position: glam::vec2(100.0, 0.0),
                    uv: glam::vec2(max_uv.x, min_uv.y),
                },
                TexturedVertex {
                    position: glam::vec2(100.0, 100.0),
                    uv: glam::vec2(max_uv.x, max_uv.y),
                },
                TexturedVertex {
                    position: glam::vec2(0.0, 100.0),
                    uv: glam::vec2(min_uv.x, max_uv.y),
                },
            ]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        });

        let text_ib = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("text_ib"),
            contents: bytemuck::cast_slice(&[0u32, 1, 2, 0, 2, 3]),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
        });

        let font_atlas = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("font_atlas"),
            layout: &textured.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &font.texture.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&device.create_sampler(
                        &wgpu::SamplerDescriptor {
                            min_filter: wgpu::FilterMode::Linear,
                            mag_filter: wgpu::FilterMode::Linear,
                            ..Default::default()
                        },
                    )),
                },
            ],
        });

        Ok(Self {
            config,
            surface,
            device,
            queue,
            window,
            fullscreen_quad,
            textured,
            font_atlas,
            text_vb,
            text_ib,
            font,
            camera,
            camera_binding,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.camera.resize(self.config.width, self.config.height);
        self.camera_binding.update(&self.camera, &self.queue);
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

            pass.set_bind_group(0, &self.font_atlas, &[]);
            pass.set_bind_group(1, self.camera_binding.bind_group(), &[]);
            pass.set_vertex_buffer(0, self.text_vb.slice(..));
            pass.set_index_buffer(self.text_ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&self.textured);
            pass.draw_indexed(0..6, 0, 0..1);
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
