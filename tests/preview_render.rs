//! Headless render test for the wgpu PBR preview: drives the exact production render path
//! (PreviewResources + PreviewCallback::render_to) on a real adapter, with validation error
//! scopes armed, and reads the pixels back. Catches "the 3D view is silently black" bugs
//! that the egui-side tests can't see (they run with render_state = None).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr, clippy::disallowed_methods)]

use eframe::egui_wgpu::wgpu;
use futures::executor::block_on;
use lumagen::preview::{EnvPreset, PreviewCallback, PreviewResources};
use lumagen::state::PreviewMesh;

const SIZE: u32 = 256;

fn device_or_skip() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("SKIP preview_render: no wgpu adapter available ({e})");
            return None;
        }
    };
    match block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) {
        Ok(pair) => Some(pair),
        Err(e) => {
            eprintln!("SKIP preview_render: no wgpu device ({e})");
            None
        }
    }
}

fn target(device: &wgpu::Device, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test_target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
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

/// Render one frame through the production path and return the RGBA8 pixels.
fn render_frame(device: &wgpu::Device, queue: &wgpu::Queue, callback: &PreviewCallback) -> Vec<u8> {
    let color = target(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
    );
    let depth = target(device, wgpu::TextureFormat::Depth24Plus, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    // Arm a validation scope over the whole frame: any validation error in the production
    // path fails the test with the real message instead of rendering silent black.
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let mut res = PreviewResources::new(device, queue);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("test_enc") });
    callback.render_to(device, queue, &mut encoder, &mut res, &color_view, &depth_view);

    let bytes_per_row = SIZE * 4; // 1024 — already a multiple of COPY_BYTES_PER_ROW_ALIGNMENT
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test_readback"),
        size: u64::from(bytes_per_row * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    if let Some(err) = block_on(scope.pop()) {
        panic!("wgpu validation error in the preview render path: {err}");
    }

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll");
    rx.recv().expect("map callback").expect("buffer map");
    slice.get_mapped_range().to_vec()
}

/// Count the pixels that differ from the pass clear color (the visible mesh footprint).
fn lit_pixels(pixels: &[u8]) -> usize {
    // Clear is (0.06, 0.07, 0.09) → ~(15, 18, 23).
    pixels
        .chunks_exact(4)
        .filter(|p| (i16::from(p[0]) - 15).abs() > 6 || (i16::from(p[1]) - 18).abs() > 6 || (i16::from(p[2]) - 23).abs() > 6)
        .count()
}

fn base_callback() -> PreviewCallback {
    PreviewCallback {
        mesh: PreviewMesh::Sphere,
        angle: 0.7,
        orbit: [0.4, 0.3],
        radius: 3.2,
        tiling: 1.0,
        normal_strength: 1.0,
        emission_boost: 1.0,
        has_maps: false,
        albedo: None,
        normal: None,
        roughness: None,
        metallic: None,
        ao: None,
        emission: None,
        map_sides: [lumagen::maps::TEX_SIZE as u32; 6],
        env: EnvPreset::from_hdri("Studio Soft"),
        phys_size: [SIZE, SIZE],
    }
}

/// A map-less render must show the lit placeholder sphere — a black viewport here is the
/// "white fallback texture was never uploaded" regression.
#[test]
fn preview_renders_visible_sphere_without_maps() {
    let Some((device, queue)) = device_or_skip() else { return };
    let pixels = render_frame(&device, &queue, &base_callback());
    let lit = lit_pixels(&pixels);
    let total = (SIZE * SIZE) as usize;
    assert!(lit > total / 10, "sphere invisible: only {lit}/{total} pixels differ from the clear color");
}

/// Every selectable mesh must be visible from the default orbit camera.
#[test]
fn all_meshes_visible_from_default_orbit() {
    let Some((device, queue)) = device_or_skip() else { return };
    let total = (SIZE * SIZE) as usize;
    for mesh in [PreviewMesh::Sphere, PreviewMesh::Plane, PreviewMesh::Cube] {
        let callback = PreviewCallback { mesh, ..base_callback() };
        let pixels = render_frame(&device, &queue, &callback);
        let lit = lit_pixels(&pixels);
        let name = mesh.name();
        assert!(lit > total / 20, "{name} invisible from the default camera: {lit}/{total} pixels lit");
    }
    // The plane must also survive orbiting BEHIND it (two-sided) — a single-sided quad
    // vanishes there.
    let callback = PreviewCallback {
        mesh: PreviewMesh::Plane,
        orbit: [std::f32::consts::PI + 0.4, 0.3],
        ..base_callback()
    };
    let pixels = render_frame(&device, &queue, &callback);
    let lit = lit_pixels(&pixels);
    assert!(lit > total / 20, "plane invisible from behind: {lit}/{total} pixels lit");
}

/// With real (synthetic) maps bound, the render must still be visible and must not trip
/// validation.
#[test]
fn preview_renders_with_maps() {
    let Some((device, queue)) = device_or_skip() else { return };
    let side = lumagen::maps::TEX_SIZE;
    let fill = |px: [u8; 4]| std::sync::Arc::new(px.repeat(side * side));
    let gray = |v: u8| fill([v, v, v, 255]);
    let callback = PreviewCallback {
        has_maps: true,
        albedo: Some(fill([170, 120, 80, 255])),
        normal: Some(fill([128, 128, 255, 255])),
        roughness: Some(gray(140)),
        metallic: Some(gray(0)),
        ao: Some(gray(255)),
        emission: Some(gray(0)),
        ..base_callback()
    };
    let pixels = render_frame(&device, &queue, &callback);
    let lit = lit_pixels(&pixels);
    let total = (SIZE * SIZE) as usize;
    assert!(lit > total / 10, "textured sphere invisible: only {lit}/{total} pixels lit");
}
