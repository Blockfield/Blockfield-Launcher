// three.js r156 FXAAShader port; NVIDIA and three.js notices in THIRD_PARTY_LICENSES.
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var linear_sampler: sampler;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    return vec4(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

fn tap(uv: vec2<f32>) -> vec4<f32> {
    return textureSampleLevel(scene, linear_sampler, uv, 0.0);
}

fn contrast(a: vec4<f32>, b: vec4<f32>) -> f32 {
    let d = abs(a - b);
    return max(max(d.r, d.g), max(d.b, d.a));
}

fn fxaa(uv: vec2<f32>, pixel: vec2<f32>) -> vec4<f32> {
    let m = tap(uv);
    var s = tap(uv + vec2(0.0, pixel.y));
    let e = tap(uv + vec2(pixel.x, 0.0));
    var n = tap(uv - vec2(0.0, pixel.y));
    let w = tap(uv - vec2(pixel.x, 0.0));
    let cn = contrast(m, n);
    let cs = contrast(m, s);
    let ce = contrast(m, e);
    let cw = contrast(m, w);
    if max(max(cn, cs), max(ce, cw)) < 0.2 { return m; }
    var relative_v = ((cn + cs) - (ce + cw)) * 5.0;
    var horizontal = relative_v > 0.0;
    if abs(relative_v) < 0.3 {
        let direction = vec2(select(-1.0, 1.0, ce > cw), select(-1.0, 1.0, cs > cn));
        let along_h = tap(uv + vec2(direction.x, -direction.y) * pixel);
        let along_v = tap(uv + vec2(-direction.x, direction.y) * pixel);
        relative_v = (contrast(m, along_v) - contrast(m, along_h)) * 5.0;
        if abs(relative_v) < 0.3 { return mix(m, (n + s + e + w) * 0.25, 0.4); }
        horizontal = relative_v > 0.0;
    }
    if !horizontal { n = w; s = e; }
    if contrast(m, n) <= contrast(m, s) { n = s; }
    let step = select(vec2(0.0, pixel.y), vec2(pixel.x, 0.0), horizontal);
    var done_n = false;
    var done_p = false;
    var distance = 0.0;
    var used_n = 0.0;
    var used_p = 0.0;
    for (var i = 0u; i < 5u; i++) {
        distance += f32(i + 1u);
        if !done_n {
            let end = tap(uv + step * distance);
            done_n = contrast(end, m) > contrast(end, n);
            used_n = f32(i);
        }
        if !done_p {
            let end = tap(uv - step * distance);
            done_p = contrast(end, m) > contrast(end, n);
            used_p = f32(i);
        }
        if done_n || done_p { break; }
    }
    if !done_n && !done_p { return m; }
    let distance_ratio = min(select(1.0, used_n / 4.0, done_n), select(1.0, used_p / 4.0, done_p));
    return mix(m, n, (1.0 - sqrt(distance_ratio)) * 0.5);
}

@fragment
fn fs(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = 1.0 / vec2<f32>(textureDimensions(scene));
    return fxaa(position.xy * pixel, pixel);
}
