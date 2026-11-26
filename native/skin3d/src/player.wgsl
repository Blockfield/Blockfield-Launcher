// Ported skinview3d/three.js material math; notices in THIRD_PARTY_LICENSES.
struct Uniforms {
    view_projection: mat4x4<f32>,
    camera: vec4<f32>,
}
@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var skin: texture_2d<f32>;
@group(0) @binding(2) var nearest: sampler;

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

@vertex
fn vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip = uniforms.view_projection * vec4(position, 1.0);
    out.world = position;
    out.normal = normal;
    out.uv = uv;
    return out;
}

fn lighting(rgb: vec3<f32>, world: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    // MeshStandardMaterial defaults: roughness=1, metalness=0, dielectric F0=.04.
    // The only point light is at the camera, so light/view/half directions coincide.
    let delta = uniforms.camera.xyz - world;
    let n_dot_l = clamp(dot(normalize(normal), normalize(delta)), 0.0, 1.0);
    let irradiance = n_dot_l * 0.6 / max(dot(delta, delta), 0.01);
    let reciprocal_pi = 0.3183098861837907;
    let visibility = 0.5 / max(2.0 * n_dot_l, 1e-6);
    let fresnel = 0.04 + 0.96 * exp2(-12.53789);
    let specular = fresnel * visibility * reciprocal_pi;
    return rgb * reciprocal_pi * (3.0 + irradiance) + vec3(irradiance * specular);
}

@fragment
fn inner(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(skin, nearest, in.uv);
    return vec4(lighting(texel.rgb, in.world, in.normal), 1.0);
}

@fragment
fn outer(in: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let texel = textureSample(skin, nearest, in.uv);
    if texel.a < 1e-5 { discard; }
    let normal = select(-in.normal, in.normal, front);
    return vec4(lighting(texel.rgb, in.world, normal), texel.a);
}
