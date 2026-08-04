//! Modal interaction regressions: clicks INSIDE a modal must not dismiss it, the backdrop
//! must dismiss without activating widgets behind it, Escape must dismiss, and the export
//! modal's ✕ must not start an export.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use eframe::egui;
use egui_kittest::kittest::Queryable as _;
use lumagen::state::{AppState, Screen};

const W: f32 = 1200.0;
const H: f32 = 800.0;

/// A harness over the full screen router + modals, mirroring `LumagenApp::ui`.
fn harness(state: AppState) -> egui_kittest::Harness<'static, AppState> {
    let mut themed = false;
    let mut harness = egui_kittest::Harness::builder().with_size(egui::Vec2::new(W, H)).build_ui_state(
        move |ui, state: &mut AppState| {
            let ctx = ui.ctx().clone();
            if !themed {
                themed = true;
                lumagen::theme::apply(&ctx);
            }
            state.ensure_textures(&ctx);
            state.process_worker_results(&ctx);
            match state.screen {
                Screen::Library => lumagen::library::show(ui, state),
                Screen::Workspace => lumagen::workspace::show(ui, state, None),
                Screen::Settings => lumagen::settings::show(ui, state),
            }
            lumagen::modals::show_all(&ctx, state);
        },
        state,
    );
    harness.run();
    harness
}

/// Base state isolated from the developer's real library: the location points at a
/// nonexistent per-process temp path so the library screen's scan is a no-op and these
/// tests interact with the seeded mock entries only.
fn base() -> AppState {
    let mut s = AppState::new();
    s.show_intro = false;
    s.settings.library_location = std::env::temp_dir()
        .join(format!("lumagen_modal_isolated_{}", std::process::id()))
        .display()
        .to_string();
    s
}

#[test]
fn click_inside_new_project_modal_keeps_it_open() {
    let mut s = base();
    s.show_new_project = true;
    let mut h = harness(s);

    // Click a non-interactive spot INSIDE the dialog (its explainer label).
    h.get_by_label("Projects group materials in your library. The new project becomes the current one.")
        .click();
    h.run();
    assert!(h.state().show_new_project, "clicking inside the modal must not close it");

    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.rect().center().x > 300.0) // the modal's field, not the sidebar search box
        .expect("project name field");
    field.click();
    h.run();
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.rect().center().x > 300.0)
        .expect("project name field");
    field.type_text("Deep Space Nine");
    h.run();
    assert!(h.state().show_new_project, "typing must not close the modal");

    h.get_by_label("Create project").click();
    h.run();
    assert!(!h.state().show_new_project, "Create closes the modal");
    assert!(
        h.state().projects.iter().any(|p| p.name == "Deep Space Nine"),
        "the project was created: {:?}",
        h.state().projects.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
    );
    assert_eq!(h.state().project_name, "Deep Space Nine", "new project becomes current");
}

#[test]
fn backdrop_click_dismisses_without_activating_background() {
    let mut s = base();
    s.show_new_project = true;
    let mut h = harness(s);

    // "New material" sits in the header BEHIND the backdrop (its label starts with an icon
    // glyph, hence the contains-match; the empty-library CTA duplicates the label, so take
    // the first — the header button). The click must dismiss the modal and NOT reach it.
    let btn = h.get_all_by_label_contains("New material").next().expect("header button");
    btn.click();
    h.run();
    h.run();
    assert!(!h.state().show_new_project, "backdrop click dismisses the modal");
    assert!(!h.state().show_create, "the button behind the backdrop must not fire");
}

#[test]
fn escape_dismisses_modal() {
    let mut s = base();
    s.show_new_project = true;
    let mut h = harness(s);
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(!h.state().show_new_project, "Escape dismisses the modal");
}

#[test]
fn export_modal_close_button_does_not_export() {
    let mut s = base();
    s.screen = Screen::Workspace;
    // Give every derived map pixels so the export modal is reachable/valid.
    let albedo = s.albedo_rgba();
    for id in lumagen::data::MapId::DERIVED {
        let params = s.map(id).derive_params();
        let rgba = lumagen_core::derive::derive_map(id, &albedo, None, lumagen::maps::TEX_SIZE, &params);
        s.map_pixels.insert(id, std::sync::Arc::new(rgba));
        s.map_mut(id).status = lumagen::state::MapStatus::Ready;
    }
    s.show_export = true;
    let mut h = harness(s);

    // The header ✕ (phosphor X glyph) must only close — never launch the export (and its
    // folder picker).
    h.get_by_label(lumagen::icon::CLOSE).click();
    h.run();
    assert!(!h.state().show_export, "✕ closes the export modal");
    assert!(h.state().last_export_dir.is_none(), "✕ must not start an export");
}
