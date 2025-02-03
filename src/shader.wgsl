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

fn dot2(x: vec2<f32>) -> f32 {
    return dot(x, x);
}

@fragment
fn canvas(vs: VsOut) -> @location(0) vec4<f32> {
    let col = vec3(vs.uv, 0.0);
    return vec4(col, 1.0);
}