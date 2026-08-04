//! Headless visual review: renders every screen (and the main modals) off-screen through
//! egui_kittest + wgpu and writes PNGs to `target/ui_shots/`. The assertions are minimal —
//! the point is a fast, desktop-independent way to SEE the UI after a change.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods, clippy::print_stdout)]

use eframe::egui;
use lumagen::data::MapId;
use lumagen::state::{AppState, MapStatus, Screen, ViewMode};

const W: f32 = 1500.0;
const H: f32 = 940.0;

fn shot_dir() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("ui_shots");
    std::fs::create_dir_all(&dir).expect("create ui_shots dir");
    dir
}

/// Render `state` for `frames` steps (letting the background render worker deliver textures
/// between steps) and save the final frame as `<name>.png`.
fn snap(name: &str, state: AppState, frames: usize) {
    let mut state = state;
    let mut themed = false;
    let mut harness = egui_kittest::Harness::builder().with_size(egui::Vec2::new(W, H)).wgpu().build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        if !themed {
            themed = true;
            lumagen::theme::apply(&ctx);
        }
        // Mirror LumagenApp::logic's per-frame state work (minus the tokio launches).
        state.ensure_textures(&ctx);
        state.process_worker_results(&ctx);
        match state.screen {
            Screen::Library => lumagen::library::show(ui, &mut state),
            Screen::Workspace => lumagen::workspace::show(ui, &mut state, None),
            Screen::Settings => lumagen::settings::show(ui, &mut state),
        }
        lumagen::modals::show_all(&ctx, &mut state);
    });
    for _ in 0..frames {
        harness.step();
        // Give the CPU render worker time to rasterize previews/thumbnails.
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    harness.step();
    let image = harness.render().expect("render frame");
    assert!(image.width() > 0 && image.height() > 0);
    let path = shot_dir().join(format!("{name}.png"));
    image.save(&path).expect("save png");
    println!("wrote {}", path.display());
}

/// A base state with the intro dismissed (most shots don't want the splash). The library
/// location points at a nonexistent per-process temp path so a triggered scan is a no-op —
/// shots must show the seeded fixtures, never the developer's real ~/Lumagen/Library.
fn base() -> AppState {
    let mut s = AppState::new();
    s.show_intro = false;
    s.settings.library_location = std::env::temp_dir()
        .join(format!("lumagen_ui_shots_isolated_{}", std::process::id()))
        .display()
        .to_string();
    s
}

/// Seed a few library entries (doc-less, procedural previews) so the grid shots have content
/// — the real app starts with an empty library.
fn with_demo_library(mut s: AppState) -> AppState {
    for (slug, class, coverage, stale, seed) in [
        ("painted_hull", "Painted", 8u8, None, 4821u32),
        ("bare_metal", "Metal", 8, None, 1207),
        ("observation_glass", "Glass", 5, Some(4), 9333),
        ("light_strip", "Painted", 8, None, 5540),
        ("pipes_cables", "Metal", 6, None, 8111),
        ("conduits_vents", "Metal", 0, None, 377),
        ("solar_panel", "Glass", 3, None, 6644),
    ] {
        s.library.push(lumagen::state::LibMaterial {
            slug: slug.into(),
            class: class.into(),
            project: "Orbital Station".into(),
            res: "4K".into(),
            coverage,
            stale_index: stale,
            seed,
            doc_path: None,
        });
    }
    s.projects.push(lumagen::state::LibProject {
        name: "Orbital Station".into(),
    });
    s.library_scanned = true; // skip the disk scan; these shots want the seeded set
    s
}

/// Install real derived pixels for every map so the workspace looks like a finished material.
fn with_generated_maps(mut s: AppState) -> AppState {
    let albedo = s.albedo_rgba();
    let size = lumagen::maps::TEX_SIZE;
    for id in MapId::DERIVED {
        let params = s.map(id).derive_params();
        let rgba = lumagen_core::derive::derive_map(id, &albedo, None, size, &params);
        s.map_pixels.insert(id, std::sync::Arc::new(rgba));
        s.map_mut(id).status = MapStatus::Ready;
    }
    s
}

#[test]
fn ui_screens_render_to_pngs() {
    snap("library", with_demo_library(base()), 14);
    snap("library_empty", base(), 6);
    {
        let mut s = AppState::new();
        s.show_intro = true;
        snap("intro", with_demo_library(s), 14);
    }
    {
        let mut s = with_demo_library(base());
        s.lib_search = "zzz_no_match".into();
        snap("library_empty_search", s, 6);
    }

    {
        let mut s = base();
        s.screen = Screen::Workspace;
        snap("workspace_2d", s, 14);
    }
    {
        let mut s = base();
        s.create_project("Test Project");
        s.new_material_from_create("hull_test".into(), 777);
        s.pending_albedo_gen = false; // freeze in the generating state for the shot
        snap("workspace_albedo_generating", s, 8);
    }
    {
        let mut s = base();
        s.create_project("Test Project");
        s.new_material_from_create("hull_test".into(), 777);
        s.pending_albedo_gen = false;
        s.map_mut(MapId::Albedo).status = MapStatus::Error;
        s.albedo_error = Some("api 404: model 'openai/gpt-image-2' was not found".into());
        snap("workspace_albedo_failed", s, 8);
    }
    {
        let mut s = base();
        s.screen = Screen::Workspace;
        s.select(MapId::Roughness);
        snap("workspace_empty_map", s, 10);
    }
    {
        let mut s = with_generated_maps(base());
        s.screen = Screen::Workspace;
        s.view = ViewMode::Sheet;
        snap("workspace_sheet", s, 14);
    }
    // Selecting a derived map shows the adjust panel.
    {
        let mut s = with_generated_maps(base());
        s.screen = Screen::Workspace;
        s.select(MapId::Normal);
        s.view = ViewMode::Tiled;
        snap("workspace_tiled_normal", s, 14);
    }

    // Settings: the three most content-heavy categories.
    {
        let mut s = base();
        s.screen = Screen::Settings;
        snap("settings_providers", s, 8);
    }
    {
        let mut s = base();
        s.screen = Screen::Settings;
        s.settings.category = lumagen::state::SettingsCategory::Prompts;
        snap("settings_prompts", s, 8);
    }
    {
        let mut s = base();
        s.screen = Screen::Settings;
        s.settings.category = lumagen::state::SettingsCategory::Interface;
        snap("settings_interface", s, 8);
    }

    {
        let mut s = base();
        s.show_create = true;
        snap("modal_create_choice", s, 10);
    }
    {
        let mut s = base();
        s.create.stage = lumagen::state::CreateStage::Generate;
        s.show_create = true;
        snap("modal_create_generate", s, 10);
    }
    {
        let mut s = base();
        s.show_new_project = true;
        snap("modal_new_project", s, 8);
    }
    {
        let mut s = base();
        s.screen = Screen::Workspace;
        s.show_assist = true;
        snap("modal_assist", s, 12);
    }
    {
        let mut s = with_generated_maps(base());
        s.screen = Screen::Workspace;
        s.show_export = true;
        snap("modal_export", s, 12);
    }
    {
        let mut s = with_generated_maps(base());
        s.screen = Screen::Workspace;
        s.select(MapId::Normal);
        s.show_compare = true;
        snap("modal_compare", s, 12);
    }
}
