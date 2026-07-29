//! The `lumagen` binary: tracing + credential-store setup, then the eframe app.
// Release builds use the Windows GUI subsystem so launching the exe doesn't open a
// console window; debug keeps the console for live stderr logs. File logging is
// unaffected either way.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use lumagen::{app, diagnostics, secrets};

fn main() -> eframe::Result<()> {
    // The binary owns the single tracing subscriber; keep the guard alive to the end so
    // buffered file logs flush on shutdown.
    let _log_guard = diagnostics::init_tracing();
    tracing::info!("lumagen starting");
    // Install the OS credential store once, before any API-key Entry is created.
    secrets::install_credential_store();
    // Install the rustls crypto provider before any HTTP client exists — without it every
    // network call panics inside its task and silently dies.
    lumagen_core::providers::init_crypto();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Lumagen — PBR material studio")
        .with_inner_size([1500.0, 940.0])
        .with_position([80.0, 60.0]);
    // The brand mark as the window/taskbar icon (the .exe icon itself is embedded by
    // build.rs via winresource).
    if let Some(icon) = lumagen::brand::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        // wgpu backend — required for the custom canvas; set explicitly so a feature edit
        // can't silently flip it.
        renderer: eframe::Renderer::Wgpu,
        viewport,
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("lumagen", options, Box::new(|cc| Ok(Box::new(app::LumagenApp::new(cc)?))))
}
