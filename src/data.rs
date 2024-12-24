#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Line {
    pub a: glam::Vec2,
    pub b: glam::Vec2,
}