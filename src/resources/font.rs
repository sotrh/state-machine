use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::Path,
};

use anyhow::Context;
use glam::{vec2, Vec2};
use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::utils::RenderPipelineBuilder;

use super::{
    camera::{CameraBinder, CameraBinding},
    Resources,
};

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

#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct FontUniforms {
    unit_range: Vec2,
    in_bias: f32,
    out_bias: f32,
    smoothness: f32,
    super_sample: f32,
    inv_gamma: f32,
    _padding: u32,
}

pub struct TextPipeline {
    font_uniforms: FontUniforms,
    font_uniform_buffer: wgpu::Buffer,
    text_pipeline: wgpu::RenderPipeline,
    font_uniform_bg: wgpu::BindGroup,
    font_atlas: wgpu::BindGroup,
}

impl TextPipeline {
    pub fn new(
        font: &Font,
        camera_binder: &CameraBinder,
        surface_format: wgpu::TextureFormat,
        texture_bindgroup_layout: &wgpu::BindGroupLayout,
        shader: &wgpu::ShaderModule,
        device: &wgpu::Device,
    ) -> anyhow::Result<Self> {
        let font_uniforms = FontUniforms {
            unit_range: vec2(
                font.info.distance_field.distance_range as f32 / font.info.common.scale_w as f32,
                font.info.distance_field.distance_range as f32 / font.info.common.scale_h as f32,
            ),
            in_bias: 0.0,
            out_bias: 0.0,
            smoothness: 0.0,
            super_sample: 0.0,
            inv_gamma: 1.0 / 1.0,
            _padding: 0,
        };

        let font_uniform_buffer = device.create_buffer_init(&BufferInitDescriptor {
            label: Some("font_uniform_buffer"),
            contents: bytemuck::bytes_of(&font_uniforms),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
        });

        let font_uniform_bg_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("font_uniform_bg_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let font_uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("font_uniform_bg"),
            layout: &font_uniform_bg_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: font_uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[
                texture_bindgroup_layout,
                camera_binder.layout(),
                &font_uniform_bg_layout,
            ],
            push_constant_ranges: &[],
        });

        let text_pipeline = RenderPipelineBuilder::new()
            .layout(&pipeline_layout)
            .vertex(wgpu::VertexState {
                module: &shader,
                entry_point: Some("textured"),
                compilation_options: Default::default(),
                buffers: &[TexturedVertex::VB_DESC],
            })
            .fragment(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("msdf_text"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            })
            .build(&device)?;

        let font_atlas = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("font_atlas"),
            layout: &text_pipeline.get_bind_group_layout(0),
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
            font_uniforms,
            font_uniform_buffer,
            font_uniform_bg,
            text_pipeline,
            font_atlas,
        })
    }

    pub fn buffer_text(
        &self,
        font: &Font,
        device: &wgpu::Device,
        text: &str,
    ) -> anyhow::Result<TextBuffer> {
        let (verts, indices) = generate_text_data(font, text, font.unknown_char);

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

        Ok(TextBuffer {
            num_indices: indices.len() as _,
            indices: ib,
            vertices: vb,
        })
    }

    pub fn update_text(
        &self,
        font: &Font,
        text: &str,
        buffer: &mut TextBuffer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<()> {
        let (verts, indices) = generate_text_data(font, text, font.unknown_char);

        if verts.len() * size_of::<TexturedVertex>() > buffer.vertices.size() as usize {
            buffer.vertices = device.create_buffer_init(&BufferInitDescriptor {
                label: Some(text),
                contents: bytemuck::cast_slice(&verts),
                usage: buffer.vertices.usage(),
            });
        } else {
            queue.write_buffer(&buffer.vertices, 0, bytemuck::cast_slice(&verts));
        }

        if indices.len() * size_of::<TexturedVertex>() > buffer.indices.size() as usize {
            buffer.indices = device.create_buffer_init(&BufferInitDescriptor {
                label: Some(text),
                contents: bytemuck::cast_slice(&indices),
                usage: buffer.indices.usage(),
            });
        } else {
            queue.write_buffer(&buffer.indices, 0, bytemuck::cast_slice(&indices));
        }

        buffer.num_indices = indices.len() as _;

        Ok(())
    }

    pub fn draw_text(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        text: &TextBuffer,
        camera_binding: &CameraBinding,
    ) {
        pass.set_bind_group(0, &self.font_atlas, &[]);
        pass.set_bind_group(1, camera_binding.bind_group(), &[]);
        pass.set_bind_group(2, &self.font_uniform_bg, &[]);
        pass.set_vertex_buffer(0, text.vertices.slice(..));
        pass.set_index_buffer(text.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_pipeline(&self.text_pipeline);
        pass.draw_indexed(0..text.num_indices as u32, 0, 0..1);
    }
}

fn generate_text_data(font: &Font, text: &str, unknown_char: char) -> (Vec<TexturedVertex>, Vec<u32>) {
    let tex_width = font.texture.width() as f32;
    let tex_height = font.texture.height() as f32;

    let mut cursor = 0.0;
    let mut i = 0u32;

    let mut verts = Vec::new();
    let mut indices = Vec::new();
    for c in text.chars() {
        let glyph = font
            .glyph(c)
            .unwrap_or_else(|| font.unknown_glyph());

        if glyph.width == 0 || glyph.height == 0 {
            cursor += glyph.xadvance as f32;
            continue;
        }

        let min_uv = glam::vec2(glyph.x as f32 / tex_width, glyph.y as f32 / tex_height);
        let max_uv = min_uv
            + glam::vec2(
                glyph.width as f32 / tex_width,
                glyph.height as f32 / tex_height,
            );

        let p1 = glam::vec2(
            cursor + glyph.xoffset as f32 + 20.0,
            glyph.yoffset as f32 + 20.0,
        );
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
    (verts, indices)
}

pub struct TextBuffer {
    // todo: font: FontId,
    num_indices: u32,
    indices: wgpu::Buffer,
    vertices: wgpu::Buffer,
}

pub struct Font {
    unknown_char: char,
    pub info: FontData,
    pub texture: wgpu::Texture,
    pub glyph_map: HashMap<char, usize>,
}

impl Font {
    pub fn load(
        resources: &Resources,
        path: impl AsRef<Path>,
        unknown_char: char,
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

        if !glyph_map.contains_key(&unknown_char) {
            anyhow::bail!("'{unknown_char}' not supported by font");
        }

        Ok(Self {
            unknown_char,
            texture,
            info,
            glyph_map,
        })
    }

    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyph_map.get(&c).map(|&i| &self.info.glyphs[i])
    }
    
    pub fn unknown_glyph(&self) -> &Glyph {
        self.glyph(self.unknown_char).unwrap()
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
