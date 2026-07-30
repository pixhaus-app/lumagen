//! Embedded brand assets, included straight from `reference/lumagen-brand-pack` (the
//! binary is self-contained; the pack directory is the single source of truth). The
//! identity is the four-quadrant material mark: albedo, normal, roughness, and height
//! quadrants in one circular preview.

use eframe::egui;

/// The circular four-quadrant mark in its container tile (256², used for the window icon
/// and in-app logo).
pub const MARK_PNG: &[u8] = include_bytes!("../reference/lumagen-brand-pack/windows/png/lumagen-256x256.png");
/// Horizontal mark + wordmark lockup for dark backgrounds.
pub const WORDMARK_PNG: &[u8] = include_bytes!("../reference/lumagen-brand-pack/brand/lumagen-horizontal-for-dark-1024.png");
/// Dark splash screen (1280×800) — the first-run hero.
pub const SPLASH_PNG: &[u8] = include_bytes!("../reference/lumagen-brand-pack/splash/dark/lumagen-splash-1280x800.png");

/// The mark at `size`² points (egui's bytes loader caches by URI, so this is cheap per frame).
pub fn mark(size: f32) -> egui::Image<'static> {
    egui::Image::from_bytes("bytes://brand/lumagen-mark", MARK_PNG).fit_to_exact_size(egui::Vec2::splat(size))
}

/// The horizontal lockup constrained to `height` points (aspect preserved).
pub fn wordmark(height: f32) -> egui::Image<'static> {
    egui::Image::from_bytes("bytes://brand/lumagen-wordmark", WORDMARK_PNG).max_height(height)
}

/// The dark splash artwork (caller sizes it).
pub fn splash() -> egui::Image<'static> {
    egui::Image::from_bytes("bytes://brand/lumagen-splash", SPLASH_PNG)
}

/// Decode the mark into the window/taskbar icon. `None` if the embedded PNG fails to
/// decode (the window then keeps the default icon instead of failing startup).
pub fn window_icon() -> Option<egui::IconData> {
    let rgba = image::load_from_memory(MARK_PNG).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    })
}
