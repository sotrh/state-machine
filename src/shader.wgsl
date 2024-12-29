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

fn sdf_line(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

fn sdf_bezier(P: vec2<f32>, A: vec2<f32>, B: vec2<f32>, C: vec2<f32>) -> f32 {
    let a = B - A;
    let b = A - 2.0 * B + C;
    let c = a * 2.0;
    let d = A - P;
    let kk = 1.0 / dot(b, b);
    let kx = kk * dot(a, b);
    let ky = kk * (2.0 * dot(a, a) +  dot(d, b)) / 3.0;
    let kz = kk * dot(d, a);
    let p = ky - kx * kx;
    let p3 = p * p * p;
    let q = kx * (2.0 * kx * kx - 3.0 * ky) + kz;

    var h = q * q + 4.0 * p3;
    var res = 0.0;

    if h >= 0.0 {
        h = sqrt(h);
        let x = (vec2(h, -h) - q) / 2.0;
        let uv =  sign(x) * pow(abs(x), vec2(1.0/3.0));
        let t = clamp(uv.x + uv.y - kx, 0.0, 1.0);
        res = dot2(d + (c + b * t) * t);
    } else {
        let z = sqrt(-p);
        let v = acos(q/(p * z * 2.0)) / 3.0;
        let m = cos(v);
        let n = sin(v) * 1.732050808;
        let t = clamp(vec3(m+m, -n-m, n-m) * z - kx, vec3(0.0), vec3(1.0));
        res = min(
            dot2(d + (c + b * t.x) * t.x),
            dot2(d + (c + b * t.y) * t.y),
        );
        // The third root cannot be the closest
        // res = min(res,dot2(d+(c+b*t.z)*t.z));
    }

    return sqrt(res);
}

struct Line {
    a: vec2<f32>,
    b: vec2<f32>,
}

struct GeometryInfo {
    preview_line: Line,
    num_lines: u32,
    mode: u32,
    aspect_ratio: f32,
}

@group(0)
@binding(0)
var<uniform> geo_info: GeometryInfo;

@group(0)
@binding(1)
var<storage, read> lines: array<Line>;

@fragment
fn canvas(vs: VsOut) -> @location(0) vec4<f32> {
    let p = vec2(vs.uv.x * geo_info.aspect_ratio, vs.uv.y);
    var d = sdf_line(p, geo_info.preview_line.a, geo_info.preview_line.b);

    for (var i = 0u; i < arrayLength(&lines) && i < geo_info.num_lines; i += 1u) {
        var line = lines[i];
        d = min(d, sdf_line(p, line.a, line.b));
    }

    var col = mix(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), smoothstep(0.015, 0.01, d));
    if (geo_info.mode == 1) {
        col = mix(vec3(0.0, 0.0, 0.0), vec3(0.0, 0.0, 1.0), fract(d * 20.0));
    }

    return vec4(col, 1.0);
}