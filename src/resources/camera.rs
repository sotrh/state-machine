use wgpu::util::{BufferInitDescriptor, DeviceExt};

pub trait Camera {
    fn view_proj(&self) -> glam::Mat4;
}

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct CameraUniform {
    pub view_proj: glam::Mat4,
}

pub struct CameraBinder {
    layout: wgpu::BindGroupLayout,
}

impl CameraBinder {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("CameraBinder"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        Self { layout }
    }

    pub fn bind(&self, device: &wgpu::Device, camera: &impl Camera) -> CameraBinding {
        let buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("CameraBinding::buffer"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.view_proj(),
            }),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("CameraBinding::bind_group"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        CameraBinding { bind_group, buffer }
    }

    pub(crate) fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }
}

pub struct CameraBinding {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl CameraBinding {
    pub fn update(&mut self, camera: &impl Camera, queue: &wgpu::Queue) {
        queue.write_buffer(
            &self.buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.view_proj(),
            }),
        );
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

#[derive(Debug)]
pub struct OrthoCamera {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
}

impl OrthoCamera {
    pub fn new(left: f32, right: f32, bottom: f32, top: f32) -> Self {
        Self {
            left,
            right,
            bottom,
            top,
        }
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.right = width as f32;
        self.bottom = height as f32;
    }
}

impl Camera for OrthoCamera {
    fn view_proj(&self) -> glam::Mat4 {
        glam::Mat4::orthographic_rh(self.left, self.right, self.bottom, self.top, 0.0, 1.0)
    }
}
