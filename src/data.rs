#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Line {
    pub a: glam::Vec2,
    pub b: glam::Vec2,
}

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct GeometryInfo {
    pub num_lines: u32,
    _padding0: u32,   
}

impl GeometryInfo {
    pub fn new(num_lines: u32) -> Self {
        Self { num_lines, _padding0: 0 }
    }
}