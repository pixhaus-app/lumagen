//! WGSL for the hand-rolled PBR preview. Cook-Torrance GGX
//! with the derived maps: albedo (sRGB), normal (tangent-space), roughness, metallic, AO,
//! emission. A single key light plus a hemisphere ambient term whose colors/intensity come from
//! the selected environment preset (Studio / Warehouse / Outdoor / Night) — real, selectable
//! lighting without shipping a full image-based HDRI pipeline.

pub const SHADER: &str = r"
struct Uniforms {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    normal_mat: mat4x4<f32>,
    cam_pos: vec4<f32>,
    light_dir: vec4<f32>,       // toward the light, normalized, w unused
    light_color: vec4<f32>,     // rgb, w = intensity
    ambient_top: vec4<f32>,     // hemisphere sky color
    ambient_bottom: vec4<f32>,  // hemisphere ground color
    params: vec4<f32>,          // x = has_maps (0/1), y = exposure, z = uv tiling, w unused
    knobs: vec4<f32>,           // x = normal strength, y = emission intensity, z/w unused
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var albedo_tex: texture_2d<f32>;
@group(0) @binding(2) var normal_tex: texture_2d<f32>;
@group(0) @binding(3) var rough_tex: texture_2d<f32>;
@group(0) @binding(4) var ao_tex: texture_2d<f32>;
@group(0) @binding(5) var emis_tex: texture_2d<f32>;
@group(0) @binding(6) var samp: sampler;
@group(0) @binding(7) var metal_tex: texture_2d<f32>;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tangent: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let wp = u.model * vec4<f32>(in.pos, 1.0);
    out.world_pos = wp.xyz;
    out.clip = u.mvp * vec4<f32>(in.pos, 1.0);
    out.normal = normalize((u.normal_mat * vec4<f32>(in.normal, 0.0)).xyz);
    out.uv = in.uv;
    let t = normalize((u.normal_mat * vec4<f32>(in.tangent.xyz, 0.0)).xyz);
    out.tangent = vec4<f32>(t, in.tangent.w);
    return out;
}

const PI: f32 = 3.14159265359;

fn distribution_ggx(n: vec3<f32>, h: vec3<f32>, rough: f32) -> f32 {
    let a = rough * rough;
    let a2 = a * a;
    let ndh = max(dot(n, h), 0.0);
    let denom = (ndh * ndh * (a2 - 1.0) + 1.0);
    return a2 / (PI * denom * denom);
}

fn geometry_schlick(ndv: f32, rough: f32) -> f32 {
    let r = rough + 1.0;
    let k = (r * r) / 8.0;
    return ndv / (ndv * (1.0 - k) + k);
}

fn geometry_smith(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, rough: f32) -> f32 {
    return geometry_schlick(max(dot(n, v), 0.0), rough) * geometry_schlick(max(dot(n, l), 0.0), rough);
}

fn fresnel_schlick(cos_theta: f32, f0: vec3<f32>) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let has_maps = u.params.x > 0.5;
    // UV repeat for tiling inspection (sampler address mode is Repeat).
    let uv = in.uv * max(u.params.z, 0.01);

    // Albedo (authored sRGB → linear).
    let albedo_srgb = textureSample(albedo_tex, samp, uv).rgb;
    let albedo = pow(albedo_srgb, vec3<f32>(2.2));

    var roughness = 0.6;
    var metal = 0.0;
    var ao = 1.0;
    var emission = vec3<f32>(0.0);
    var n = normalize(in.normal);

    if has_maps {
        roughness = textureSample(rough_tex, samp, uv).r;
        metal = textureSample(metal_tex, samp, uv).r;
        ao = textureSample(ao_tex, samp, uv).r;
        emission = pow(textureSample(emis_tex, samp, uv).rgb, vec3<f32>(2.2)) * u.knobs.y;

        // Tangent-space normal map → world, with an adjustable strength (scales the XY
        // deviation; 0 = flat geometric normal, 1 = as authored).
        let raw_n = textureSample(normal_tex, samp, uv).xyz * 2.0 - vec3<f32>(1.0);
        let map_n = normalize(vec3<f32>(raw_n.xy * u.knobs.x, max(raw_n.z, 0.05)));
        let t = normalize(in.tangent.xyz);
        let b = cross(n, t) * in.tangent.w;
        let tbn = mat3x3<f32>(t, b, n);
        n = normalize(tbn * map_n);
    }
    roughness = clamp(roughness, 0.045, 1.0);

    let v = normalize(u.cam_pos.xyz - in.world_pos);
    let l = normalize(u.light_dir.xyz);
    let h = normalize(v + l);
    let radiance = u.light_color.rgb * u.light_color.w;

    // Metals reflect their albedo (F0 = albedo); dielectrics use a fixed 0.04.
    let f0 = mix(vec3<f32>(0.04), albedo, metal);
    let ndf = distribution_ggx(n, h, roughness);
    let g = geometry_smith(n, v, l, roughness);
    let f = fresnel_schlick(max(dot(h, v), 0.0), f0);

    let numerator = ndf * g * f;
    let denominator = 4.0 * max(dot(n, v), 0.0) * max(dot(n, l), 0.0) + 0.0001;
    let specular = numerator / vec3<f32>(denominator);

    let ks = f;
    // Metals have no diffuse term.
    let kd = (vec3<f32>(1.0) - ks) * (1.0 - metal);
    let ndl = max(dot(n, l), 0.0);
    var lo = (kd * albedo / PI + specular) * radiance * ndl;

    // Hemisphere ambient from the selected environment preset; AO attenuates it.
    let hemi = mix(u.ambient_bottom.rgb, u.ambient_top.rgb, n.y * 0.5 + 0.5);
    let ambient = hemi * albedo * ao;
    var color = ambient + lo * ao + emission;

    // Exposure + Reinhard-ish tonemap, then back to sRGB.
    color = color * u.params.y;
    color = color / (color + vec3<f32>(1.0));
    color = pow(color, vec3<f32>(1.0 / 2.2));
    return vec4<f32>(color, 1.0);
}
";
