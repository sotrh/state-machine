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

struct FontUniforms {
    unit_range: vec2<f32>,
    in_bias: f32,
    out_bias: f32,
    smoothness: f32,
    super_sample: f32,
    inv_gamma: f32,
}

@group(2)
@binding(0)
var<uniform> uniforms: FontUniforms;

fn median(msd: vec3<f32>) -> f32 {
    return max(min(msd.r, msd.g), min(max(msd.r, msd.g), msd.b));
}

fn screen_px_range(uv: vec2<f32>) -> f32 {
    let screen_tex_size = vec2(1.0) / fwidth(uv);
    return max(0.5 * dot(uniforms.unit_range, screen_tex_size), 1.0);
}

fn contour(d: f32, width: f32) -> f32 {
    let e = width * (d - 0.5 + uniforms.in_bias) + 0.5 + uniforms.out_bias;
    return mix(
        clamp(e, 0.0, 1.0),
        smoothstep(0.0, 1.0, e),
        uniforms.smoothness
    );
}

fn sample(uv: vec2<f32>, width: f32) -> f32 {
    let msd = textureSample(font_texture, font_sampler, uv);
    let sd = median(msd.rgb);
    let opacity = contour(sd, width);
    return opacity;
}

@fragment
fn msdf_text(vs: VsOut) -> @location(0) vec4<f32> {
    let width = screen_px_range(vs.uv);
    var opacity = sample(vs.uv, width);

    let dscale = 0.345;
    let duv = dscale * (dpdx(vs.uv) + dpdy(vs.uv));
    let box = vec4(vs.uv - duv, vs.uv + duv);
    let asum = sample(box.xy, width)
        + sample(box.zw, width)
        + sample(box.xw, width)
        + sample(box.zy, width);
    opacity = mix(opacity, (opacity + 0.5 * asum) / 3.0, uniforms.super_sample);
    opacity = pow(opacity, uniforms.inv_gamma);

    let col = vec3(1.0);

    return vec4(col, opacity);
}