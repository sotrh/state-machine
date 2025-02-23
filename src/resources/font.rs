use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::Path,
};

use wgpu::util::{BufferInitDescriptor, DeviceExt};

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

        Ok(Self {
            texture,
            info,
            glyph_map,
        })
    }

    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyph_map.get(&c).map(|&i| &self.info.glyphs[i])
    }

    pub fn buffer_text(
        &self,
        device: &wgpu::Device,
        text: &str,
    ) -> (wgpu::Buffer, wgpu::Buffer, usize) {
        let mut cursor = 0.0;
        let mut i = 0u32;

        let mut verts = Vec::new();
        let mut indices = Vec::new();
        for c in text.chars() {
            let glyph = self.glyph(c).unwrap();
            
            if glyph.width == 0 || glyph.height == 0 {
                cursor += glyph.xadvance as f32;
                continue;
            }

            let tex_width = self.texture.width() as f32;
            let tex_height = self.texture.height() as f32;
            let min_uv = glam::vec2(glyph.x as f32 / tex_width, glyph.y as f32 / tex_height);
            let max_uv = min_uv
                + glam::vec2(
                    glyph.width as f32 / tex_width,
                    glyph.height as f32 / tex_height,
                );

            let p1 = glam::vec2(cursor + glyph.xoffset as f32 + 20.0, glyph.yoffset as f32 + 20.0);
            let p2 = p1 + glam::vec2(glyph.width as f32, glyph.height as f32);

            verts.extend_from_slice(&[
                TexturedVertex {
                    position: glam::vec2(p1.x, p1.y),
                    uv: glam::vec2(min_uv.x, min_uv.y),
                },
                TexturedVertex {
                    position: glam::vec2(p2.x, p1.y),
                    uv: glam::vec2(max_uv.x, min_uv.y),
                },
                TexturedVertex {
                    position: glam::vec2(p2.x, p2.y),
                    uv: glam::vec2(max_uv.x, max_uv.y),
                },
                TexturedVertex {
                    position: glam::vec2(p1.x, p2.y),
                    uv: glam::vec2(min_uv.x, max_uv.y),
                },
            ]);

            indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);

            cursor += glyph.xadvance as f32;
            i += 4;
        }

        let vb = device.create_buffer_init(&BufferInitDescriptor {
            label: Some(text),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&BufferInitDescriptor {
            label: Some(text),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDEX,
        });

        (vb, ib, indices.len())
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FontData {
    pub pages: Vec<String>,
    #[serde(rename = "chars")]
    pub glyphs: Vec<Glyph>,
    pub info: FontInfo,
    pub common: FontCommonInfo,
    #[serde(rename = "distanceField")]
    pub distance_field: DistanceFieldInfo,
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
    pub face: String,
    pub size: u32,
    pub bold: u32,
    pub italic: u32,
    pub charset: Vec<char>,
    pub unicode: u32,
    #[serde(rename = "stretchH")]
    pub stretch_h: u32,
    pub smooth: u32,
    pub aa: u32,
    pub padding: [u32; 4],
    pub spacing: [u32; 2],
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FontCommonInfo {
    #[serde(rename = "lineHeight")]
    pub line_height: u32,
    pub base: u32,
    #[serde(rename = "scaleW")]
    pub scale_w: u32,
    #[serde(rename = "scaleH")]
    pub scale_h: u32,
    pub pages: u32,
    pub packed: u32,
    #[serde(rename = "alphaChnl")]
    pub alpha_channel: u32,
    #[serde(rename = "redChnl")]
    pub red_channel: u32,
    #[serde(rename = "greenChnl")]
    pub green_channel: u32,
    #[serde(rename = "blueChnl")]
    pub blue_channel: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DistanceFieldInfo {
    #[serde(rename = "fieldType")]
    pub field_type: String,
    #[serde(rename = "distanceRange")]
    pub distance_range: u32,
}
