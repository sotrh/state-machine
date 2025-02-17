struct VsOut {
    @builtin(position)
    frag_position: vec4<f32>,
    @location(0)
    uv: vec2<f32>,
}

@vertex
fn fullscreen_quad(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2(
        f32(i % 2u) * 2.0,
        f32(i > 1u) * 2.0,
    );

    return VsOut(vec4(uv * 2.0 - 1.0, 0.0, 1.0), uv);
}

struct TexturedVertex {
    @location(0)
    position: vec2<f32>,
    @location(1)
    uv: vec2<f32>,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(1)
@binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn textured(in: TexturedVertex) -> VsOut {
    return VsOut(camera.view_proj * vec4(in.position, 0.0, 1.0), in.uv);
}

@group(0)
@binding(0)
var font_texture: texture_2d<f32>;
@group(0)
@binding(1)
var font_sampler: sampler;

@fragment
fn canvas(vs: VsOut) -> @location(0) vec4<f32> {
    let col = textureSample(font_texture, font_sampler, vs.uv);
    return col;
    // return vec4(vs.uv, 0.0, 1.0);
}