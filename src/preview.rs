//! Hand-rolled wgpu PBR material preview. Renders the selected mesh (sphere / plane / cube)
//! lit by the derived maps into an offscreen color+depth target in a `CallbackTrait::prepare`
//! pass, then the UI shows that target via `register_native_texture` as an `egui::Image`.

use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu::{self, wgpu};

use crate::preview_shader::SHADER;
use crate::state::PreviewMesh;

// ── Math (minimal, no external linalg dep) ────────────────────────────────

type Vec3 = [f32; 3];
type Mat4 = [[f32; 4]; 4];

fn vsub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn vcross(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn vdot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn vnorm(a: Vec3) -> Vec3 {
    let l = vdot(a, a).sqrt();
    if l < 1e-6 { [0.0, 0.0, 1.0] } else { [a[0] / l, a[1] / l, a[2] / l] }
}

/// `a · b` for column-major matrices (`m[col][row]`, matching WGSL `mat4x4`). The storage
/// convention matters: a row-major multiply over this layout silently computes `b · a`,
/// reversing the MVP chain (pinned by tests/preview_render.rs).
fn mat_mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, col) in out.iter_mut().enumerate() {
        for (r, cell) in col.iter_mut().enumerate() {
            *cell = a[0][r] * b[c][0] + a[1][r] * b[c][1] + a[2][r] * b[c][2] + a[3][r] * b[c][3];
        }
    }
    out
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y / 2.0).tan();
    let mut m = [[0.0; 4]; 4];
    m[0][0] = f / aspect;
    m[1][1] = f;
    m[2][2] = (far + near) / (near - far);
    m[2][3] = -1.0;
    m[3][2] = (2.0 * far * near) / (near - far);
    m
}

fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = vnorm(vsub(target, eye));
    let s = vnorm(vcross(f, up));
    let u = vcross(s, f);
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-vdot(s, eye), -vdot(u, eye), vdot(f, eye), 1.0],
    ]
}

fn rotation_y(angle: f32) -> Mat4 {
    let (s, c) = angle.sin_cos();
    [[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

// ── Mesh ──────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    tangent: [f32; 4],
}

struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

fn sphere_mesh(segments: u32, rings: u32) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for r in 0..=rings {
        let v = r as f32 / rings as f32;
        let phi = v * std::f32::consts::PI;
        for s in 0..=segments {
            let u = s as f32 / segments as f32;
            let theta = u * std::f32::consts::TAU;
            let (sp, cp) = phi.sin_cos();
            let (st, ct) = theta.sin_cos();
            let normal = [sp * ct, cp, sp * st];
            let tangent = vnorm([-st, 0.0, ct]);
            // Handedness -1: our UVs have v growing DOWN the image (top-left origin), so the
            // bitangent must flip to keep OpenGL (green-up) normal maps shading bumps as bumps.
            vertices.push(Vertex {
                pos: normal,
                normal,
                uv: [u, v],
                tangent: [tangent[0], tangent[1], tangent[2], -1.0],
            });
        }
    }
    let stride = segments + 1;
    for r in 0..rings {
        for s in 0..segments {
            let a = r * stride + s;
            let b = a + stride;
            // Wound so the right-hand-rule face normal points OUTWARD — the reverse order
            // renders the sphere inside-out (front faces culled, far inner surface showing).
            indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    Mesh { vertices, indices }
}

fn plane_mesh() -> Mesh {
    // Two-sided: an orbiting camera can end up behind the sheet, and a single-sided quad
    // simply vanishes there (backface culling). The back face gets its own normal/tangent
    // so lighting is correct from both sides.
    let quad = |n: Vec3, t: [f32; 4]| {
        move |x: f32, y: f32, u: f32, vv: f32| Vertex {
            pos: [x, y, 0.0],
            normal: n,
            uv: [u, vv],
            tangent: t,
        }
    };
    // Handedness -1 — see sphere_mesh.
    let f = quad([0.0, 0.0, 1.0], [1.0, 0.0, 0.0, -1.0]);
    let b = quad([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0, -1.0]);
    Mesh {
        vertices: vec![
            f(-1.0, -1.0, 0.0, 1.0),
            f(1.0, -1.0, 1.0, 1.0),
            f(1.0, 1.0, 1.0, 0.0),
            f(-1.0, 1.0, 0.0, 0.0),
            b(-1.0, -1.0, 0.0, 1.0),
            b(1.0, -1.0, 1.0, 1.0),
            b(1.0, 1.0, 1.0, 0.0),
            b(-1.0, 1.0, 0.0, 0.0),
        ],
        indices: vec![0, 1, 2, 0, 2, 3, 4, 6, 5, 4, 7, 6],
    }
}

/// (normal, tangent, 4 CCW corners) for one cube face.
type CubeFace = ([f32; 3], [f32; 3], [[f32; 3]; 4]);

fn cube_mesh() -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let faces: [CubeFace; 6] = [
        (
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            [[-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]],
        ),
        (
            [0.0, 0.0, -1.0],
            [-1.0, 0.0, 0.0],
            [[1.0, -1.0, -1.0], [-1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [1.0, 1.0, -1.0]],
        ),
        (
            [1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [[1.0, -1.0, 1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0]],
        ),
        (
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [[-1.0, -1.0, -1.0], [-1.0, -1.0, 1.0], [-1.0, 1.0, 1.0], [-1.0, 1.0, -1.0]],
        ),
        (
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [[-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]],
        ),
        (
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, -1.0, 1.0], [-1.0, -1.0, 1.0]],
        ),
    ];
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    for (normal, tangent, corners) in faces {
        let base = vertices.len() as u32;
        for (i, c) in corners.iter().enumerate() {
            // Handedness -1 — see sphere_mesh.
            vertices.push(Vertex {
                pos: *c,
                normal,
                uv: uvs[i],
                tangent: [tangent[0], tangent[1], tangent[2], -1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh { vertices, indices }
}

// ── Uniforms ──────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: Mat4,
    model: Mat4,
    normal_mat: Mat4,
    cam_pos: [f32; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    ambient_top: [f32; 4],
    ambient_bottom: [f32; 4],
    params: [f32; 4],
    /// x = normal-map strength, y = emission intensity, z/w unused.
    knobs: [f32; 4],
}

/// A selectable lighting environment (the HDRI dropdown). Analytic — a key light + hemisphere
/// ambient tuned per environment — so switching the HDRI genuinely changes the shading without a
/// full image-based lighting pipeline.
#[derive(Clone, Copy)]
pub struct EnvPreset {
    /// Key light color.
    pub key_color: [f32; 3],
    /// Key light intensity.
    pub key_intensity: f32,
    /// Hemisphere sky (top) ambient color.
    pub ambient_top: [f32; 3],
    /// Hemisphere ground (bottom) ambient color.
    pub ambient_bottom: [f32; 3],
    /// Tonemap exposure.
    pub exposure: f32,
}

impl EnvPreset {
    /// Resolve the environment from the HDRI dropdown label.
    pub fn from_hdri(name: &str) -> Self {
        match name {
            "Warehouse" => EnvPreset {
                key_color: [1.0, 0.95, 0.85],
                key_intensity: 2.6,
                ambient_top: [0.09, 0.09, 0.10],
                ambient_bottom: [0.04, 0.04, 0.045],
                exposure: 1.0,
            },
            "Outdoor Noon" => EnvPreset {
                key_color: [1.0, 0.98, 0.92],
                key_intensity: 4.6,
                ambient_top: [0.20, 0.26, 0.34],
                ambient_bottom: [0.11, 0.10, 0.08],
                exposure: 1.1,
            },
            "Night City" => EnvPreset {
                key_color: [0.55, 0.62, 0.95],
                key_intensity: 1.6,
                ambient_top: [0.04, 0.05, 0.09],
                ambient_bottom: [0.03, 0.02, 0.04],
                exposure: 1.35,
            },
            // "Studio Soft" (default).
            _ => EnvPreset {
                key_color: [1.0, 1.0, 1.0],
                key_intensity: 3.0,
                ambient_top: [0.10, 0.11, 0.13],
                ambient_bottom: [0.03, 0.035, 0.045],
                exposure: 1.0,
            },
        }
    }
}

// ── GPU resources ─────────────────────────────────────────────────────────

/// Persistent GPU state for the preview: pipeline, shared sampler/white texture, meshes, and the
/// offscreen color+depth target. Created lazily and stored in egui's callback resources.
pub struct PreviewResources {
    pipeline: wgpu::RenderPipeline,
    bind_layout: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    sampler: wgpu::Sampler,
    white_view: Arc<wgpu::TextureView>,
    meshes: [Option<GpuMesh>; 3],
    /// Per-map GPU texture cache. The CPU buffers are `Arc<Vec<u8>>` replaced wholesale
    /// (never mutated in place), so Arc pointer identity + side is a sound invalidation
    /// key — a map re-uploads only when its pixels actually changed, not every frame.
    map_cache: [Option<MapSlot>; 6],
    /// The material bind group, rebuilt only when one of the six texture views changes.
    bind_cache: Option<([usize; 6], wgpu::BindGroup)>,
}

/// One cached GPU texture: the source buffer's identity and the uploaded view.
struct MapSlot {
    key: (usize, u32),
    view: Arc<wgpu::TextureView>,
}

struct GpuMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
}

// The offscreen target is shared between the UI thread (which registers it as an egui texture)
// and the callback's `prepare` (which renders into it). egui's callback resources aren't visible
// to the UI thread, so the view is kept in a process-wide slot keyed by physical size. Both
// sides run on the render/UI thread within the same frame, so a plain `Mutex` never contends.
type OffscreenSlot = (Arc<wgpu::TextureView>, wgpu::TextureView, [u32; 2]);
static OFFSCREEN: std::sync::Mutex<Option<OffscreenSlot>> = std::sync::Mutex::new(None);

/// A single-mip 2D texture with the given format/usage.
fn texture_2d(device: &wgpu::Device, label: &'static str, size: [u32; 2], format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

/// A color (render attachment + binding) and depth target pair at `size`.
fn create_targets(device: &wgpu::Device, size: [u32; 2], color_label: &'static str, depth_label: &'static str) -> (Arc<wgpu::TextureView>, wgpu::TextureView) {
    let color = texture_2d(
        device,
        color_label,
        size,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let depth = texture_2d(
        device,
        depth_label,
        size,
        wgpu::TextureFormat::Depth24Plus,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    (
        Arc::new(color.create_view(&wgpu::TextureViewDescriptor::default())),
        depth.create_view(&wgpu::TextureViewDescriptor::default()),
    )
}

/// Upload square RGBA8 `pixels` (`side` × `side`) into mip 0 of `tex`.
fn upload_rgba(queue: &wgpu::Queue, tex: &wgpu::Texture, pixels: &[u8], side: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * side),
            rows_per_image: Some(side),
        },
        wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
    );
}

fn offscreen_view(device: &wgpu::Device, size: [u32; 2]) -> (Arc<wgpu::TextureView>, wgpu::TextureView) {
    // A poisoned mutex can't be recovered without panicking; fall back to a fresh unsaved target
    // so the preview degrades (renders but doesn't display) instead of crashing the UI.
    let mut slot = match OFFSCREEN.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if slot.as_ref().is_none_or(|(_, _, s)| *s != size) {
        let (view, depth) = create_targets(device, size, "pbr_offscreen", "pbr_depth");
        *slot = Some((view, depth, size));
    }
    match slot.as_ref() {
        Some((v, d, _)) => (Arc::clone(v), d.clone()),
        // Unreachable: we just populated the slot above. Guard without panicking anyway —
        // a 1×1 target renders nothing visible but keeps the pass valid.
        None => create_targets(device, [1, 1], "pbr_color_fallback", "pbr_depth_fallback"),
    }
}

/// Register the shared offscreen target as an egui texture. The registration is kept across
/// frames and only re-pointed when the target is recreated (viewport resize), so we don't leak a
/// new egui texture every frame. Called from the UI before the callback's prepare renders.
pub fn register_offscreen(device: &wgpu::Device, renderer: &mut egui_wgpu::Renderer, size: [u32; 2]) -> egui::TextureId {
    let (view, _) = offscreen_view(device, size);
    let mut reg = OFFSCREEN_REG.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((id, stored)) = reg.as_ref() {
        let id = *id;
        if Arc::ptr_eq(stored, &view) {
            return id;
        }
        // Target was recreated (viewport resize): re-point the existing registration at the
        // new view IN PLACE. Never free first — free_texture removes the renderer's entry
        // and the update then panics ("Tried to update a texture that has not been
        // allocated yet").
        renderer.update_egui_texture_from_wgpu_texture(device, &view, wgpu::FilterMode::Linear, id);
        *reg = Some((id, Arc::clone(&view)));
        return id;
    }
    let id = renderer.register_native_texture(device, &view, wgpu::FilterMode::Linear);
    *reg = Some((id, Arc::clone(&view)));
    id
}

static OFFSCREEN_REG: std::sync::Mutex<Option<(egui::TextureId, Arc<wgpu::TextureView>)>> = std::sync::Mutex::new(None);

impl PreviewResources {
    /// Public so the headless render test (tests/preview_render.rs) can drive the exact
    /// production pipeline without an eframe window.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pbr_preview"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pbr_preview_bgl"),
            entries: &[
                uniform_entry(0),
                texture_entry(1),
                texture_entry(2),
                texture_entry(3),
                texture_entry(4),
                texture_entry(5),
                sampler_entry(6),
                texture_entry(7),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pbr_preview_pl"),
            bind_group_layouts: &[Some(&bind_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pbr_preview"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                front_face: wgpu::FrontFace::Ccw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pbr_preview_ubo"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pbr_preview_samp"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let white = solid_texture(device, queue, [255, 255, 255, 255]);
        // A wgpu validation error must degrade the preview, not abort the process — the
        // default uncaptured-error behavior is a panic in the callback. Log loudly instead;
        // the tests' fail-on-validation hook installs its own handler on its own device.
        device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
            tracing::error!("wgpu uncaptured error in the preview path: {e}");
        }));
        Self {
            pipeline,
            bind_layout,
            uniform_buf,
            sampler,
            white_view: Arc::new(white),
            meshes: [None, None, None],
            map_cache: [const { None }; 6],
            bind_cache: None,
        }
    }
}

fn solid_texture(device: &wgpu::Device, queue: &wgpu::Queue, rgba: [u8; 4]) -> wgpu::TextureView {
    let tex = texture_2d(
        device,
        "pbr_white",
        [1, 1],
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    // A created texture is zeroed (transparent black) — without this upload every "white
    // fallback" bind samples black and shaded fallback renders show nothing.
    upload_rgba(queue, &tex, &rgba, 1);
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn mesh_index(which: PreviewMesh) -> usize {
    match which {
        PreviewMesh::Sphere => 0,
        PreviewMesh::Plane => 1,
        PreviewMesh::Cube => 2,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
fn texture_binding(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

// ── The paint callback ────────────────────────────────────────────────────

/// Renders the mesh into the offscreen target during `prepare`. The UI then shows that target via
/// the `TextureId` from [`PreviewResources::egui_texture`]. `paint` is a no-op (the image is drawn
/// by egui itself as a regular `Image` widget).
pub struct PreviewCallback {
    pub mesh: PreviewMesh,
    /// Light rotation (the "Rotate light" action).
    pub angle: f32,
    /// Camera orbit `[yaw, pitch]` in radians (drag to orbit).
    pub orbit: [f32; 2],
    /// Camera distance (scroll to dolly).
    pub radius: f32,
    /// UV repeat factor (tiling inspection in 3D; sampler address mode is Repeat).
    pub tiling: f32,
    /// Normal-map strength multiplier (scales the tangent-space XY before renormalizing).
    pub normal_strength: f32,
    /// Emission intensity multiplier.
    pub emission_boost: f32,
    pub has_maps: bool,
    pub albedo: Option<Arc<Vec<u8>>>,
    pub normal: Option<Arc<Vec<u8>>>,
    pub roughness: Option<Arc<Vec<u8>>>,
    pub metallic: Option<Arc<Vec<u8>>>,
    pub ao: Option<Arc<Vec<u8>>>,
    pub emission: Option<Arc<Vec<u8>>>,
    /// Per-slot pixel side (albedo, normal, roughness, metallic, ao, emission) — decoded
    /// masters shade the mesh at native resolution, working copies at `TEX_SIZE`.
    pub map_sides: [u32; 6],
    /// The selected lighting environment (from the HDRI dropdown).
    pub env: EnvPreset,
    /// Physical-pixel size of the offscreen target (viewport rect × pixels_per_point).
    pub phys_size: [u32; 2],
}

impl PreviewCallback {
    /// Render the preview into the given color+depth views — the shared offscreen target in
    /// the app, an explicit readback target in the headless render test. This IS the
    /// production render path; `prepare` only resolves the target and delegates here.
    pub fn render_to(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        res: &mut PreviewResources,
        offscreen_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) {
        upload_mesh(device, queue, res, self.mesh);

        let albedo = upload_map(device, queue, self.albedo.as_ref(), self.map_sides[0], res, 0);
        let normal = upload_map(device, queue, self.normal.as_ref(), self.map_sides[1], res, 1);
        let rough = upload_map(device, queue, self.roughness.as_ref(), self.map_sides[2], res, 2);
        let metal = upload_map(device, queue, self.metallic.as_ref(), self.map_sides[3], res, 3);
        let ao = upload_map(device, queue, self.ao.as_ref(), self.map_sides[4], res, 4);
        let emis = upload_map(device, queue, self.emission.as_ref(), self.map_sides[5], res, 5);

        let [w, h] = self.phys_size;
        let aspect = w as f32 / h.max(1) as f32;
        let proj = perspective(45f32.to_radians(), aspect, 0.1, 100.0);
        // Orbit camera: the mesh stays put; the camera moves on a sphere around it.
        let [yaw, pitch] = self.orbit;
        let pitch = pitch.clamp(-1.45, 1.45);
        let r = self.radius.clamp(1.4, 12.0);
        let cam_pos = [r * pitch.cos() * yaw.sin(), r * pitch.sin(), r * pitch.cos() * yaw.cos()];
        let view = look_at(cam_pos, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        let model = rotation_y(0.0);
        let mvp = mat_mul(proj, mat_mul(view, model));
        let env = self.env;
        let uniforms = Uniforms {
            mvp,
            model,
            normal_mat: model,
            cam_pos: [cam_pos[0], cam_pos[1], cam_pos[2], 1.0],
            light_dir: light_from_angle(self.angle),
            light_color: [env.key_color[0], env.key_color[1], env.key_color[2], env.key_intensity],
            ambient_top: [env.ambient_top[0], env.ambient_top[1], env.ambient_top[2], 1.0],
            ambient_bottom: [env.ambient_bottom[0], env.ambient_bottom[1], env.ambient_bottom[2], 1.0],
            params: [if self.has_maps { 1.0 } else { 0.0 }, env.exposure, self.tiling.clamp(0.25, 32.0), 0.0],
            knobs: [self.normal_strength.clamp(0.0, 4.0), self.emission_boost.clamp(0.0, 16.0), 0.0, 0.0],
        };
        queue.write_buffer(&res.uniform_buf, 0, bytemuck::bytes_of(&uniforms));

        // Rebuild the bind group only when a texture view actually changed (uniform buffer
        // and sampler are stable for the resource lifetime).
        let view_keys = [&albedo, &normal, &rough, &metal, &ao, &emis].map(|v| Arc::as_ptr(v) as usize);
        if res.bind_cache.as_ref().is_none_or(|(keys, _)| *keys != view_keys) {
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pbr_preview_bg"),
                layout: &res.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: res.uniform_buf.as_entire_binding(),
                    },
                    texture_binding(1, &albedo),
                    texture_binding(2, &normal),
                    texture_binding(3, &rough),
                    texture_binding(4, &ao),
                    texture_binding(5, &emis),
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&res.sampler),
                    },
                    texture_binding(7, &metal),
                ],
            });
            res.bind_cache = Some((view_keys, bind));
        }

        // upload_mesh ran above, so the mesh is present; skip the draw rather than panic if not.
        let Some(mesh) = res.meshes[mesh_index(self.mesh)].as_ref() else {
            return;
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pbr_preview_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: offscreen_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.06,
                            g: 0.07,
                            b: 0.09,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&res.pipeline);
            // The cache was just (re)filled above; a missing entry means bind-group
            // creation failed on the device — skip the draw rather than panic.
            let Some((_, bind)) = res.bind_cache.as_ref() else { return };
            pass.set_bind_group(0, bind, &[]);
            pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
            pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, 0..1);
        }
    }
}

impl egui_wgpu::CallbackTrait for PreviewCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let res: &mut PreviewResources = resources.entry::<PreviewResources>().or_insert_with(|| PreviewResources::new(device, queue));
        let (offscreen_view, depth_view) = offscreen_view(device, self.phys_size);
        self.render_to(device, queue, encoder, res, &offscreen_view, &depth_view);
        Vec::new()
    }

    fn paint(&self, _info: egui::epaint::PaintCallbackInfo, _render_pass: &mut wgpu::RenderPass<'static>, _resources: &egui_wgpu::CallbackResources) {
        // The offscreen target is shown by egui as a regular Image widget; nothing to paint here.
    }
}

fn light_from_angle(angle: f32) -> [f32; 4] {
    let a = angle * 0.5 + 0.8;
    let dir = vnorm([a.cos(), 0.6, a.sin()]);
    [dir[0], dir[1], dir[2], 0.0]
}

fn upload_mesh(device: &wgpu::Device, queue: &wgpu::Queue, res: &mut PreviewResources, which: PreviewMesh) {
    let idx = mesh_index(which);
    if res.meshes[idx].is_some() {
        return;
    }
    let mesh = match which {
        PreviewMesh::Sphere => sphere_mesh(48, 32),
        PreviewMesh::Plane => plane_mesh(),
        PreviewMesh::Cube => cube_mesh(),
    };
    let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pbr_vbuf"),
        size: (mesh.vertices.len() * std::mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&mesh.vertices));
    let ibuf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pbr_ibuf"),
        size: (mesh.indices.len() * 4) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&ibuf, 0, bytemuck::cast_slice(&mesh.indices));
    res.meshes[idx] = Some(GpuMesh {
        vbuf,
        ibuf,
        index_count: mesh.indices.len() as u32,
    });
}

/// Upload a map's RGBA8 pixels to a GPU texture view, falling back to the shared white texture
/// when the map isn't present or is the wrong size. Cached per `slot`: the upload runs only
/// when the source buffer changed, not on every repainted frame.
fn upload_map(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixels: Option<&Arc<Vec<u8>>>,
    size: u32,
    res: &mut PreviewResources,
    slot: usize,
) -> Arc<wgpu::TextureView> {
    let Some(px) = pixels.filter(|px| px.len() == (size * size * 4) as usize) else {
        res.map_cache[slot] = None;
        return Arc::clone(&res.white_view);
    };
    // The buffers are replaced wholesale on change (never mutated through the Arc), so
    // pointer identity + side length identifies the exact pixel content.
    let key = (Arc::as_ptr(px) as usize, size);
    if let Some(cached) = res.map_cache[slot].as_ref()
        && cached.key == key
    {
        return Arc::clone(&cached.view);
    }
    let tex = texture_2d(
        device,
        "pbr_map",
        [size, size],
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    );
    upload_rgba(queue, &tex, px, size);
    let view = Arc::new(tex.create_view(&wgpu::TextureViewDescriptor::default()));
    res.map_cache[slot] = Some(MapSlot { key, view: Arc::clone(&view) });
    view
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Right-hand-rule face normal of each triangle must agree with its vertices' shading
    /// normal — a wrong winding renders the mesh inside-out (front faces culled, inner far
    /// surface showing).
    fn assert_outward(mesh: &Mesh, name: &str) {
        let mut checked = 0;
        for tri in mesh.indices.chunks_exact(3) {
            let [a, b, c] = [
                mesh.vertices[tri[0] as usize].pos,
                mesh.vertices[tri[1] as usize].pos,
                mesh.vertices[tri[2] as usize].pos,
            ];
            let e1 = vsub(b, a);
            let e2 = vsub(c, a);
            let n = vcross(e1, e2);
            if vdot(n, n) < 1e-10 {
                continue; // degenerate (sphere pole rows)
            }
            let out = mesh.vertices[tri[0] as usize].normal;
            assert!(
                vdot(n, out) > 0.0,
                "{name}: triangle {tri:?} wound against its shading normal (face {n:?} vs vertex {out:?})"
            );
            checked += 1;
        }
        assert!(checked > 0, "{name}: no non-degenerate triangles checked");
    }

    #[test]
    fn meshes_wind_outward() {
        assert_outward(&sphere_mesh(24, 16), "sphere");
        assert_outward(&cube_mesh(), "cube");
        assert_outward(&plane_mesh(), "plane");
    }

    /// Every mesh uses handedness -1 (top-left-origin UVs) so OpenGL normal maps shade
    /// bumps as bumps.
    #[test]
    fn tangent_handedness_is_negative() {
        for mesh in [sphere_mesh(8, 6), plane_mesh(), cube_mesh()] {
            for v in &mesh.vertices {
                assert!((v.tangent[3] + 1.0).abs() < f32::EPSILON);
            }
        }
    }
}
