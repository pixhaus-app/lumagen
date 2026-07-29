//! Lumagen — a PBR material studio. Turns one seamless albedo into a full, pixel-aligned
//! PBR map set (roughness, metallic, normal, displacement, AO, emission, transparency).
//! Native `eframe`/`egui` front end over a `wgpu` canvas; map rasterization and OS-vault
//! access run on a single background worker so the UI thread never blocks.
//!
//! Library target so the UI can be exercised headlessly (egui_kittest renders the screens
//! off-screen in tests); the `lumagen` binary is a thin `main` over [`app::LumagenApp`].

pub mod app;
pub mod brand;
pub mod data;
pub mod diagnostics;
pub mod icon;
pub mod library;
pub mod maps;
pub mod modals;
pub mod preview;
pub mod preview_shader;
pub mod render;
pub mod secrets;
pub mod settings;
pub mod state;
pub mod theme;
pub mod widgets;
pub mod workspace;
