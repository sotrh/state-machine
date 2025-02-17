use std::num::NonZero;

use anyhow::Context;
use wgpu::{FragmentState, VertexState};

pub struct RenderPipelineBuilder<'a> {
    label: Option<&'a str>,
    layout: Option<&'a wgpu::PipelineLayout>,
    vertex: Option<wgpu::VertexState<'a>>,
    primitive: wgpu::PrimitiveState,
    depth_stencil: Option<wgpu::DepthStencilState>,
    multisample: wgpu::MultisampleState,
    fragment: Option<wgpu::FragmentState<'a>>,
    multiview: Option<NonZero<u32>>,
    cache: Option<&'a wgpu::PipelineCache>,
}

impl<'a> RenderPipelineBuilder<'a> {
    pub fn new() -> Self {
        Self {
            label: None,
            layout: None,
            vertex: None,
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: None,
            multiview: None,
            cache: None,
        }
    }

    #[allow(unused)]
    pub fn label(mut self, value: &'a str) -> Self {
        self.label = Some(value);
        self
    }

    #[allow(unused)]
    pub fn layout(mut self, layout: &'a wgpu::PipelineLayout) -> Self {
        self.layout = Some(layout);
        self
    }

    #[allow(unused)]
    pub fn vertex(mut self, state: VertexState<'a>) -> Self {
        self.vertex = Some(state);
        self
    }

    #[allow(unused)]
    pub fn fragment(mut self, state: FragmentState<'a>) -> Self {
        self.fragment = Some(state);
        self
    }

    #[allow(unused)]
    pub fn depth(
        mut self,
        format: wgpu::TextureFormat,
        depth_compare: wgpu::CompareFunction,
    ) -> Self {
        if let Some(state) = &mut self.depth_stencil {
            state.format = format;
        } else {
            self.depth_stencil = Some(wgpu::DepthStencilState {
                format,
                depth_write_enabled: true,
                depth_compare,
                stencil: Default::default(),
                bias: Default::default(),
            })
        }
        self
    }

    #[allow(unused)]
    pub fn topology(mut self, value: wgpu::PrimitiveTopology) -> Self {
        self.primitive.topology = value;
        self
    }

    pub fn build(self, device: &wgpu::Device) -> anyhow::Result<wgpu::RenderPipeline> {
        Ok(
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: self.label,
                layout: self.layout,
                vertex: self.vertex.with_context(|| "Must specify vertex state")?,
                primitive: self.primitive,
                depth_stencil: self.depth_stencil,
                multisample: self.multisample,
                fragment: self.fragment,
                multiview: self.multiview,
                cache: self.cache,
            }),
        )
    }
}
