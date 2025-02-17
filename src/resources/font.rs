use std::{
    collections::HashMap, io::{Cursor, Read}, path::Path
};

use super::Resources;

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct TexturedVertex {
    pub position: glam::Vec2,
    pub uv: glam::Vec2,
}

impl TexturedVertex {
    pub const VB_DESC: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TexturedVertex>() as _,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x2,
            1 => Float32x2,
        ],
    };
}

pub struct Font {
    pub info: FontData,
    pub texture: wgpu::Texture,
    pub glyph_map: HashMap<char, usize>,
}

impl Font {
    pub fn load(
        resources: &Resources,
        path: impl AsRef<Path>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<Self> {
        let bin = resources.load_binary(path)?;

        let mut zip = zip::ZipArchive::new(Cursor::new(bin))?;

        let mut buffer = Vec::new();

        let texture = {
            let mut zipped_img = zip.by_index(1)?;
            let name = zipped_img.mangled_name();
            zipped_img.read_to_end(&mut buffer)?;
            let img = image::load_from_memory(&buffer)?.to_rgba8();

            let dimensions = img.dimensions();
            let texture_size = wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some(&format!("{}", name.display())),
                view_formats: &[],
            });

            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &img,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * dimensions.0),
                    rows_per_image: Some(dimensions.1),
                },
                texture_size,
            );

            texture
        };

        buffer.clear();

        zip.by_index(0)?.read_to_end(&mut buffer)?;

        let json = String::from_utf8(buffer)?;
        let info: FontData = serde_json::from_str(&json)?;

        let mut glyph_map = HashMap::new();
        for (i, glyph) in info.glyphs.iter().enumerate() {
            glyph_map.insert(glyph.char, i);
        }

        Ok(Self { texture, info, glyph_map })
    }

    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyph_map.get(&c).map(|&i| &self.info.glyphs[i])
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FontData {
    pages: Vec<String>,
    #[serde(rename = "chars")]
    glyphs: Vec<Glyph>,
    info: FontInfo,
    common: FontCommonInfo,
    #[serde(rename = "distanceField")]
    distance_field: DistanceFieldInfo,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Glyph {
    pub id: u32,
    pub index: u32,
    pub page: u32,
    pub char: char,
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
    pub xoffset: i32,
    pub yoffset: i32,
    pub xadvance: u32,
    pub chnl: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FontInfo {
    face: String,
    size: u32,
    bold: u32,
    italic: u32,
    charset: Vec<char>,
    unicode: u32,
    #[serde(rename = "stretchH")]
    stretch_h: u32,
    smooth: u32,
    aa: u32,
    padding: [u32; 4],
    spacing: [u32; 2],
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FontCommonInfo {
    #[serde(rename = "lineHeight")]
    line_height: u32,
    base: u32,
    #[serde(rename = "scaleW")]
    scale_w: u32,
    #[serde(rename = "scaleH")]
    scale_h: u32,
    pages: u32,
    packed: u32,
    #[serde(rename = "alphaChnl")]
    alpha_channel: u32,
    #[serde(rename = "redChnl")]
    red_channel: u32,
    #[serde(rename = "greenChnl")]
    green_channel: u32,
    #[serde(rename = "blueChnl")]
    blue_channel: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DistanceFieldInfo {
    #[serde(rename = "fieldType")]
    field_type: String,
    #[serde(rename = "distanceRange")]
    distance_range: u32,
}
