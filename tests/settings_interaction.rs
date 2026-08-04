//! Settings interaction regressions: the Test-connection button must give immediate,
//! persistent feedback, and the model fields must accept both a suggestion pick and
//! free-typed custom ids.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use eframe::egui;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use lumagen::state::{AppState, Screen, TestStatus};

fn harness(state: AppState) -> egui_kittest::Harness<'static, AppState> {
    let mut themed = false;
    let mut harness = egui_kittest::Harness::builder().with_size(egui::Vec2::new(1500.0, 940.0)).build_ui_state(
        move |ui, state: &mut AppState| {
            let ctx = ui.ctx().clone();
            if !themed {
                themed = true;
                lumagen::theme::apply(&ctx);
            }
            state.ensure_textures(&ctx);
            state.process_worker_results(&ctx);
            lumagen::settings::show(ui, state);
        },
        state,
    );
    harness.run();
    harness
}

fn settings_state() -> AppState {
    let mut s = AppState::new();
    s.show_intro = false;
    s.screen = Screen::Settings;
    s
}

#[test]
fn test_connection_click_shows_pending_status_and_queues_request() {
    let mut h = harness(settings_state());
    // Two "Test connection" buttons (fal first, OpenRouter second) — click the fal one.
    let buttons: Vec<_> = h.get_all_by_label_contains("Test connection").collect();
    assert!(buttons.len() >= 2, "expected fal + OpenRouter test buttons, got {}", buttons.len());
    buttons[0].click();
    drop(buttons);
    h.run();
    assert_eq!(
        h.state().settings.test_status.get(&lumagen::secrets::Provider::Fal),
        Some(&TestStatus::Pending),
        "clicking Test connection must show the inline pending status immediately"
    );
    assert_eq!(
        h.state().pending_test,
        Some(lumagen::secrets::Provider::Fal),
        "the request must be queued for the app to launch"
    );
    // The inline "testing…" label is visible.
    let _ = h.get_by_label_contains("testing…");
}

#[test]
fn model_suggestion_pick_fills_the_field() {
    let mut h = harness(settings_state());
    // Open the fal ALBEDO (t2i) suggestions dropdown — the providers page's first combo box.
    let combo = h.get_all_by_role(egui::accesskit::Role::ComboBox).next().expect("fal t2i suggestions combo");
    combo.click();
    h.step();
    h.step();
    h.step();
    h.get_by_label_contains("fal-ai/nano-banana-2  ").click();
    h.run();
    assert_eq!(
        h.state().settings.fal_model_t2i,
        "fal-ai/nano-banana-2",
        "picking a suggestion fills the albedo model field"
    );
}

#[test]
fn model_field_accepts_free_typed_text() {
    let mut h = harness(settings_state());
    // Find the fal EDIT model editor by its current value and append custom text.
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.accesskit_node().value().is_some_and(|v| v.contains("openai/gpt-image-2/edit")))
        .expect("fal edit model field");
    field.click();
    h.run();
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .find(|n| n.accesskit_node().value().is_some_and(|v| v.contains("openai/gpt-image-2/edit")))
        .expect("fal edit model field");
    field.type_text("-custom");
    h.run();
    assert!(
        h.state().settings.fal_model_edit.contains("-custom"),
        "typed text must land in the edit model id, got {:?}",
        h.state().settings.fal_model_edit
    );
}
