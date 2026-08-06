//! Settings hub (spec §10): providers, generation defaults, prompts & templates,
//! material presets, export, library, interface.

use eframe::egui::{self, Color32, Frame, RichText, Sense, Stroke, Ui, Vec2};

use crate::data::{MapId, PRESETS};
use crate::state::{AppState, PromptTab, SettingsCategory, TestStatus};
use crate::theme::*;
use crate::widgets::{self, ToastKind};

/// Suggested fal.ai TEXT-TO-IMAGE endpoints for the source albedo (any id typed works).
pub const FAL_T2I_MODELS: [(&str, &str); 3] = [
    ("openai/gpt-image-2", "GPT Image 2"),
    ("fal-ai/nano-banana-2", "Nano Banana 2"),
    ("fal-ai/flux-pro/v1.1", "FLUX 1.1 [pro]"),
];

/// Suggested fal.ai IMAGE-EDIT endpoints, shared by every AI-path map (albedo-referenced).
pub const FAL_EDIT_MODELS: [(&str, &str); 5] = [
    ("openai/gpt-image-2/edit", "GPT Image 2 Edit"),
    ("fal-ai/nano-banana-2/edit", "Nano Banana 2 Edit"),
    ("fal-ai/nano-banana-pro/edit", "Nano Banana Pro Edit"),
    ("fal-ai/flux-pro/kontext", "FLUX.1 Kontext [pro]"),
    ("fal-ai/qwen-image-edit", "Qwen Image Edit"),
];

/// Suggested OpenRouter image-output models (chat-completions image path; one model serves
/// both text-to-image and edits).
pub const OR_MODELS: [(&str, &str); 5] = [
    ("google/gemini-3.1-flash-image", "Nano Banana 2"),
    ("google/gemini-3-pro-image", "Nano Banana Pro"),
    ("google/gemini-2.5-flash-image", "Nano Banana"),
    ("openai/gpt-5-image", "GPT-5 Image"),
    ("openai/gpt-5-image-mini", "GPT-5 Image Mini"),
];

/// Suggested LLM models for the Description Check (any OpenAI-compatible id works).
const LLM_MODELS: [(&str, &str); 3] = [
    ("gpt-5.4-mini", "OpenAI, current mini"),
    ("gpt-5.4-nano", "OpenAI, cheapest"),
    ("gpt-4o-mini", "OpenAI, legacy"),
];

/// Topaz upscaler models offered in Settings → Generation and the Refine modal.
pub const TOPAZ_MODELS: [&str; 7] = ["Standard V2", "High Fidelity V2", "CGI", "Text Refine", "Wonder 3.5", "Redefine", "Bloom 2"];

/// Export-size labels in `final_size` index order.
pub const FINAL_SIZE_LABELS: [&str; 5] = ["4096²", "2048²", "1024²", "8192²", "16384²"];

/// A labeled combo over `opts` that stores the selected index in `value`.
pub fn indexed_combo(ui: &mut Ui, salt: &str, label: &str, opts: &[&str], width: f32, value: &mut usize) {
    widgets::field_label(ui, label, None);
    egui::ComboBox::from_id_salt(salt)
        .selected_text(opts[(*value).min(opts.len() - 1)])
        .width(width)
        .show_ui(ui, |ui| {
            for (i, o) in opts.iter().enumerate() {
                ui.selectable_value(value, i, *o);
            }
        });
}

/// A labeled full-width monospace text field.
fn mono_line(ui: &mut Ui, label: &str, value: &mut String) {
    widgets::field_label(ui, label, None);
    ui.add(
        egui::TextEdit::singleline(value)
            .margin(egui::Margin::symmetric(8, 6))
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let ctx = ui.ctx().clone();
    egui::Panel::top("set_head")
        .exact_size(52.0)
        .frame(Frame::new().fill(BG_1()).stroke(Stroke::new(1.0_f32, BORDER)))
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if widgets::ghost_button(ui, &format!("{} Back", crate::icon::BACK)).clicked() {
                    state.screen = state.settings.return_to;
                }
                ui.label(RichText::new("Settings").size(14.0).strong());
            });
        });

    egui::Panel::left("set_nav")
        .exact_size(240.0)
        .resizable(false)
        .frame(Frame::new().fill(BG_1()).stroke(Stroke::new(1.0_f32, BORDER)))
        .show(ui, |ui| {
            ui.add_space(12.0);
            for cat in SettingsCategory::ALL {
                let on = state.settings.category == cat;
                let fill = if on { accent_dim() } else { Color32::TRANSPARENT };
                let resp = Frame::new()
                    .fill(fill)
                    .corner_radius(R_CTL)
                    .inner_margin(egui::Margin::symmetric(11, 9))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width() - 12.0);
                        ui.label(RichText::new(cat.name()).size(12.5).color(if on { accent_text() } else { TEXT_MID }));
                    })
                    .response
                    .interact(Sense::click());
                if resp.clicked() {
                    state.settings.category = cat;
                }
                ui.add_space(2.0);
            }
        });

    egui::CentralPanel::default().frame(Frame::new().fill(BG_0())).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(840.0);
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.add_space(30.0);
                ui.vertical(|ui| {
                    ui.set_width(760.0);
                    match state.settings.category {
                        SettingsCategory::Providers => providers(ui, &ctx, state),
                        SettingsCategory::Generation => generation(ui, state),
                        SettingsCategory::Prompts => prompts(ui, state),
                        SettingsCategory::Presets => presets(ui, state),
                        SettingsCategory::Export => export(ui, state),
                        SettingsCategory::Library => library(ui, state),
                        SettingsCategory::Interface => interface(ui, state),
                    }
                    ui.add_space(40.0);
                });
            });
        });
    });
}

fn heading(ui: &mut Ui, title: &str, lead: &str) {
    ui.label(RichText::new(title).size(17.0).strong());
    ui.label(RichText::new(lead).size(12.5).color(TEXT_MID));
    ui.add_space(18.0);
}

fn providers(ui: &mut Ui, ctx: &egui::Context, state: &mut AppState) {
    // Stored keys load from the OS vault once at startup, so these rows already reflect it.
    heading(
        ui,
        "Providers & API keys",
        "Bring your own key. Keys live in your OS keychain — never in plaintext or the library.",
    );

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("fal.ai · FLUX Kontext").size(13.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(91, 140, 255, 30))
                    .corner_radius(3)
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(RichText::new("primary").size(10.0).color(accent_text()));
                    });
            });
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new("Edit/control models that keep a derived map aligned to the albedo. Get a key at fal.ai.")
                .size(11.0)
                .color(TEXT_LO),
        );
        ui.add_space(8.0);
        key_row(ui, ctx, state, crate::secrets::Provider::Fal);
        ui.add_space(8.0);
        // fal splits families into separate text-to-image and edit endpoints — one slot for
        // the albedo, one shared by every AI-path map (never one model per channel).
        model_field(
            ui,
            "fal_model_t2i",
            "Albedo model (text-to-image)",
            &mut state.settings.fal_model_t2i,
            &FAL_T2I_MODELS,
        );
        ui.add_space(8.0);
        model_field(
            ui,
            "fal_model_edit",
            "Maps model (image edit, shared by all AI maps)",
            &mut state.settings.fal_model_edit,
            &FAL_EDIT_MODELS,
        );
        ui.add_space(8.0);
        test_row(ui, ctx, state, crate::secrets::Provider::Fal);
    });

    widgets::card(ui, |ui| {
        ui.label(RichText::new("OpenRouter · Gemini / FLUX edit").size(13.0).strong());
        ui.add_space(4.0);
        ui.label(
            RichText::new("One key across many models; single call, reports the real cost per image.")
                .size(11.0)
                .color(TEXT_LO),
        );
        ui.add_space(8.0);
        key_row(ui, ctx, state, crate::secrets::Provider::OpenRouter);
        ui.add_space(8.0);
        model_field(ui, "or_model", "Model (text-to-image and edits)", &mut state.settings.or_model, &OR_MODELS);
        ui.add_space(8.0);
        test_row(ui, ctx, state, crate::secrets::Provider::OpenRouter);
    });

    widgets::card(ui, |ui| {
        ui.label(RichText::new("Requests").size(13.0).strong());
        ui.add_space(4.0);
        ui.label(
            RichText::new("Applies to every backend: how many requests run at once.")
                .size(11.0)
                .color(TEXT_LO),
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            widgets::field_label(ui, "Max concurrency", None);
            ui.add(
                egui::TextEdit::singleline(&mut state.settings.max_concurrency)
                    .margin(egui::Margin::symmetric(8, 6))
                    .font(egui::TextStyle::Monospace)
                    .desired_width(140.0),
            );
        });
    });
    widgets::card(ui, |ui| {
        ui.label(RichText::new("LLM for Description Check").size(13.0).strong());
        ui.add_space(10.0);
        key_row(ui, ctx, state, crate::secrets::Provider::Llm);
        ui.add_space(8.0);
        model_field(ui, "llm_model", "Model", &mut state.settings.llm_model, &LLM_MODELS);
        ui.add_space(8.0);
        mono_line(
            ui,
            "Endpoint (OpenAI-compatible; point at a local server if you like)",
            &mut state.settings.endpoint,
        );
    });
    if widgets::ghost_button(ui, &format!("{} Add provider (Stability, Replicate, local endpoint…)", crate::icon::ADD)).clicked() {
        state.toast(
            "More providers are planned — v1 ships fal.ai, OpenRouter, and the free offline renderer",
            ToastKind::Info,
        );
    }
}

/// A labeled model field with hint (settings-page wrapper over `widgets::model_field`).
fn model_field(ui: &mut Ui, salt: &str, label: &str, value: &mut String, suggestions: &[(&str, &str)]) {
    widgets::field_label(ui, label, None);
    widgets::model_field(ui, salt, value, suggestions);
    ui.label(RichText::new("Type any model id — the dropdown is only suggestions.").size(10.0).color(TEXT_LO));
}

/// The "Test connection" button plus the persistent inline result (pending / ok / failed),
/// so the outcome can't be missed even if the toast goes unnoticed.
fn test_row(ui: &mut Ui, ctx: &egui::Context, state: &mut AppState, provider: crate::secrets::Provider) {
    ui.horizontal(|ui| {
        if widgets::secondary_button(ui, &format!("{} Test connection", crate::icon::ORBIT)).clicked() {
            state.test_connection(ctx, provider);
        }
        match state.settings.test_status.get(&provider) {
            Some(TestStatus::Pending) => {
                ui.label(RichText::new("testing…").size(11.5).color(TEXT_LO));
            }
            Some(TestStatus::Ok(msg)) => {
                ui.label(RichText::new(format!("{} {}", crate::icon::CONFIRM, msg)).size(11.5).color(READY));
            }
            Some(TestStatus::Failed(msg)) => {
                let short = if msg.chars().count() > 72 {
                    format!("{}…", msg.chars().take(72).collect::<String>())
                } else {
                    msg.clone()
                };
                ui.label(RichText::new(format!("{} {}", crate::icon::WARNING, short)).size(11.5).color(DANGER))
                    .on_hover_text(msg);
            }
            None => {}
        }
    });
}

/// Which API-key row a provider maps to in `SettingsState`. Splitting the field access out
/// avoids borrowing all of `state` across the UI closure.
struct KeyFields {
    set: bool,
    show: bool,
}

fn key_fields(state: &AppState, provider: crate::secrets::Provider) -> KeyFields {
    let s = &state.settings;
    let (set, show) = match provider {
        crate::secrets::Provider::Fal => (s.fal_key_set, s.show_fal_key),
        crate::secrets::Provider::OpenRouter => (s.openrouter_key_set, s.show_openrouter_key),
        crate::secrets::Provider::OpenAi => (s.openai_key_set, s.show_openai_key),
        crate::secrets::Provider::Llm => (s.llm_key_set, s.show_llm_key),
    };
    KeyFields { set, show }
}

fn toggle_show(state: &mut AppState, provider: crate::secrets::Provider) {
    let s = &mut state.settings;
    match provider {
        crate::secrets::Provider::Fal => s.show_fal_key = !s.show_fal_key,
        crate::secrets::Provider::OpenRouter => s.show_openrouter_key = !s.show_openrouter_key,
        crate::secrets::Provider::OpenAi => s.show_openai_key = !s.show_openai_key,
        crate::secrets::Provider::Llm => s.show_llm_key = !s.show_llm_key,
    }
}

/// A password-masked API-key row: edit buffer + show/hide eye + Save to the OS vault. The
/// buffer and the "key set" flag live in settings; the key itself only ever lands in the
/// vault (keyring skill). Works for every provider via the shared `secrets::Provider`.
fn key_row(ui: &mut Ui, ctx: &egui::Context, state: &mut AppState, provider: crate::secrets::Provider) {
    let fields = key_fields(state, provider);
    let label = if fields.set { "API key · saved in keychain" } else { "API key" };
    widgets::field_label(ui, label, None);
    let masked = !fields.show;
    ui.horizontal(|ui| {
        // Reserve room for the show/Save buttons — an infinite-width editor would push them
        // past the card edge and stretch the card.
        let editor_w = (ui.available_width() - 130.0).max(120.0);
        // Borrow only the one input field for the editor, then drop it before the buttons.
        {
            let input = match provider {
                crate::secrets::Provider::Fal => &mut state.settings.fal_key_input,
                crate::secrets::Provider::OpenRouter => &mut state.settings.openrouter_key_input,
                crate::secrets::Provider::OpenAi => &mut state.settings.openai_key_input,
                crate::secrets::Provider::Llm => &mut state.settings.llm_key_input,
            };
            ui.add(
                egui::TextEdit::singleline(input)
                    .margin(egui::Margin::symmetric(8, 6))
                    .font(egui::TextStyle::Monospace)
                    .desired_width(editor_w)
                    .password(masked),
            );
        }
        let eye = if fields.show { "hide" } else { "show" };
        if widgets::ghost_button(ui, eye).clicked() {
            toggle_show(state, provider);
        }
        if widgets::secondary_button(ui, "Save").clicked() {
            state.save_key(ctx, provider);
        }
    });
}

fn generation(ui: &mut Ui, state: &mut AppState) {
    heading(
        ui,
        "Generation defaults",
        "Applied to new materials, override per material. These map 1:1 to your script's arguments.",
    );
    widgets::card(ui, |ui| {
        egui::Grid::new("gen_grid").num_columns(2).spacing(Vec2::new(12.0, 10.0)).show(ui, |ui| {
            ui.vertical(|ui| {
                indexed_combo(ui, "api_size", "API size", &["2880 × 2880", "2048 × 2048", "3840 × 3840"], 320.0, &mut state.settings.api_size);
            });
            ui.vertical(|ui| {
                indexed_combo(ui, "final_size", "Final size", &FINAL_SIZE_LABELS, 320.0, &mut state.settings.final_size);
            });
            ui.end_row();
            ui.vertical(|ui| {
                indexed_combo(ui, "quality", "Quality", &["low", "medium", "high", "auto"], 320.0, &mut state.settings.quality);
            });
            ui.vertical(|ui| {
                widgets::field_label(ui, "Seam margin (px)", None);
                ui.add(
                    egui::TextEdit::singleline(&mut state.settings.seam_margin)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace),
                );
            });
            ui.end_row();
            ui.vertical(|ui| {
                widgets::field_label(ui, "Retries", None);
                ui.add(
                    egui::TextEdit::singleline(&mut state.settings.retries)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace),
                );
            });
            ui.vertical(|ui| {
                widgets::field_label(ui, "Background", None);
                let bg = if state.settings.background_opaque { "opaque" } else { "transparent" };
                egui::ComboBox::from_id_salt("bg").selected_text(bg).width(320.0).show_ui(ui, |ui| {
                    let mut opaque = state.settings.background_opaque;
                    ui.selectable_value(&mut opaque, true, "opaque");
                    ui.selectable_value(&mut opaque, false, "transparent");
                    state.settings.background_opaque = opaque;
                });
            });
            ui.end_row();
            ui.vertical(|ui| {
                widgets::field_label(ui, "Job timeout (seconds)", None);
                ui.add(
                    egui::TextEdit::singleline(&mut state.settings.job_timeout)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace),
                );
                ui.label(
                    RichText::new("Whole submit → result deadline per image. Big models can need several minutes; 60–3600.")
                        .size(10.0)
                        .color(TEXT_LO),
                );
            });
            ui.vertical(|ui| {
                widgets::field_label(ui, "Upscaler model (Topaz) — Step ③ Refine", None);
                egui::ComboBox::from_id_salt("upscale_model")
                    .selected_text(state.settings.upscale_model.clone())
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for m in TOPAZ_MODELS {
                            ui.selectable_value(&mut state.settings.upscale_model, m.to_string(), m);
                        }
                    });
                ui.label(
                    RichText::new("High Fidelity V2 preserves fine texture best (default); CGI targets rendered/art sources; Wonder/Redefine add generative detail.")
                        .size(10.0)
                        .color(TEXT_LO),
                );
            });
            ui.end_row();
            ui.vertical(|ui| {
                indexed_combo(
                    ui,
                    "map_path_policy",
                    "Map paths (default for new materials)",
                    &[
                        "AI for semantic maps (roughness · metallic · emission)",
                        "AI for every map",
                        "Derived for every map",
                    ],
                    320.0,
                    &mut state.settings.map_path_policy,
                );
                ui.add_space(4.0);
                if widgets::ghost_button(ui, "Apply to the open material").clicked() {
                    let now = ui.input(|i| i.time);
                    state.apply_path_policy(now);
                    state.toast("Map paths updated — each map's Path section can still override", ToastKind::Info);
                }
                ui.label(
                    RichText::new("AI understands the material; derived computes locally — instant and pixel-aligned. Any map's Path section overrides this per material.")
                        .size(10.0)
                        .color(TEXT_LO),
                );
            });
            ui.end_row();
        });
    });
}

fn prompts(ui: &mut Ui, state: &mut AppState) {
    heading(
        ui,
        "Prompts & templates",
        "Every prompt is editable — nothing hard-coded. Edits layer on the active pack; per-material overrides sit on top.",
    );
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Template pack:").size(13.0).strong());
            ui.label(RichText::new("Default").size(13.0).color(TEXT_LO));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::ghost_button(ui, "Reset").clicked() {
                    reset_templates(state);
                    state.toast("Templates reset to defaults", ToastKind::Info);
                }
                if widgets::ghost_button(ui, "Export").clicked() {
                    let pack = state.to_prompt_pack();
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Prompt pack", &["json"])
                        .set_file_name("prompts.json")
                        .save_file()
                    {
                        match pack.save(&path) {
                            Ok(()) => state.toast(format!("Exported prompt pack → {}", path.display()), ToastKind::Success),
                            Err(e) => state.toast(format!("Export failed: {e}"), ToastKind::Error),
                        }
                    }
                }
                if widgets::ghost_button(ui, "Import").clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Prompt pack", &["json"]).pick_file()
                {
                    match lumagen_core::promptpack::PromptPack::load(&path) {
                        Ok(pack) => {
                            state.apply_prompt_pack(pack);
                            state.toast("Imported prompt pack", ToastKind::Success);
                        }
                        Err(e) => state.toast(format!("Import failed: {e}"), ToastKind::Error),
                    }
                }
            });
        });
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for (tab, label) in [
                (PromptTab::Base, "Base / system"),
                (PromptTab::PerMap, "Per-map"),
                (PromptTab::Negative, "Negative"),
                (PromptTab::Suffixes, "Suffixes"),
                (PromptTab::Tokens, "Tokens"),
            ] {
                let on = state.settings.prompt_tab == tab;
                let btn = if on {
                    egui::Button::new(RichText::new(label).size(11.5).color(accent_text()))
                        .fill(accent_dim())
                        .stroke(Stroke::new(1.0_f32, accent()))
                } else {
                    egui::Button::new(RichText::new(label).size(11.5).color(TEXT_MID))
                        .fill(BG_2())
                        .stroke(Stroke::new(1.0_f32, BORDER))
                };
                if ui.add(btn).clicked() {
                    state.settings.prompt_tab = tab;
                }
            }
        });
        ui.add_space(10.0);
        match state.settings.prompt_tab {
            PromptTab::Base => {
                let text = &mut state.settings.base_template;
                ui.add(
                    egui::TextEdit::multiline(text)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(9)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                widgets::field_label(ui, "Final requirement", None);
                ui.add(
                    egui::TextEdit::multiline(&mut state.settings.final_requirement)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);
                token_legend(ui, &["{material_description}", "{map_instructions}", "{resolution}", "{seed}"]);
            }
            PromptTab::PerMap => {
                widgets::field_label(ui, "Map template", None);
                let current = state.settings.editor_map;
                egui::ComboBox::from_id_salt("tpl_map")
                    .selected_text(crate::data::map_def(current).name)
                    .show_ui(ui, |ui| {
                        for id in MapId::ALL {
                            ui.selectable_value(&mut state.settings.editor_map, id, crate::data::map_def(id).name);
                        }
                    });
                ui.add_space(6.0);
                let id = state.settings.editor_map;
                let text = state.settings.map_templates.entry(id).or_default();
                ui.add(
                    egui::TextEdit::multiline(text)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(7)
                        .desired_width(f32::INFINITY),
                );
            }
            PromptTab::Negative => {
                ui.add(
                    egui::TextEdit::multiline(&mut state.settings.negative_prompt)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                );
            }
            PromptTab::Suffixes => {
                code_block(
                    ui,
                    "tpl_suffixes",
                    &MapId::ALL.iter().map(|id| crate::data::single_map_suffix(*id)).collect::<Vec<_>>().join("\n\n"),
                    10,
                );
            }
            PromptTab::Tokens => {
                code_block(
                    ui,
                    "tpl_tokens",
                    "{material_description}  — the material's semantic description\n{map_instructions}     — the selected map's instruction block\n{resolution}           — final size, e.g. 4096\n{seed}                 — the shared project seed\n\n$material $map $res     — export filename tokens",
                    7,
                );
            }
        }
    });
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Compiled preview").size(13.0).strong());
            ui.label(RichText::new("what the model receives").size(11.0).color(TEXT_LO));
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                widgets::field_label(ui, "Material preset", None);
                egui::ComboBox::from_id_salt("pv_mat")
                    .selected_text(state.settings.preview_preset.clone())
                    .width(360.0)
                    .show_ui(ui, |ui| {
                        for p in &PRESETS {
                            ui.selectable_value(&mut state.settings.preview_preset, p.slug.to_string(), p.slug);
                        }
                    });
            });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                widgets::field_label(ui, "Map", None);
                let cur = state.settings.preview_map;
                egui::ComboBox::from_id_salt("pv_map")
                    .selected_text(crate::data::map_def(cur).name)
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for id in MapId::ALL {
                            ui.selectable_value(&mut state.settings.preview_map, id, crate::data::map_def(id).name);
                        }
                    });
            });
        });
        ui.add_space(8.0);
        let compiled = state.settings.compiled_preview();
        code_block(ui, "tpl_compiled", &compiled, 12);
    });
}

fn reset_templates(state: &mut AppState) {
    state.settings.base_template = crate::data::BASE_TEMPLATE.into();
    state.settings.final_requirement = crate::data::FINAL_REQUIREMENT.into();
    state.settings.negative_prompt = crate::data::NEGATIVE_PROMPT.into();
    for id in MapId::ALL {
        state.settings.map_templates.insert(id, crate::data::default_map_template(id).into());
    }
}

fn token_legend(ui: &mut Ui, tokens: &[&str]) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Tokens:").size(11.0).color(TEXT_LO));
        for t in tokens {
            Frame::new()
                .fill(accent_dim())
                .corner_radius(3)
                .inner_margin(egui::Margin::symmetric(5, 1))
                .show(ui, |ui| {
                    ui.label(RichText::new(*t).monospace().size(11.0).color(accent_text()));
                });
        }
    });
}

/// `salt` must be unique per on-screen instance — two of these render on the Prompts
/// screen at once (tab content + compiled preview) and their ScrollAreas clash otherwise.
fn code_block(ui: &mut Ui, salt: &str, text: &str, rows: usize) {
    Frame::new()
        .fill(BG_0())
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical().id_salt(salt).max_height(rows as f32 * 18.0).show(ui, |ui| {
                ui.label(RichText::new(text).monospace().size(11.0).color(TEXT_MID));
            });
        });
}

fn presets(ui: &mut Ui, state: &mut AppState) {
    heading(
        ui,
        "Material presets",
        "Reusable material descriptions — the semantic anchor the Description Check scores against. Edit or add your own.",
    );
    for (i, p) in PRESETS.iter().enumerate() {
        // Unique id per stateful editor in the loop (cursor/scroll state) — id_salt by index.
        ui.push_id(i, |ui| {
            widgets::card(ui, |ui| {
                ui.label(RichText::new(p.slug).size(13.0).strong());
                ui.add_space(6.0);
                // Skip the editor if preset_texts ever diverges from PRESETS in length.
                let Some(text) = state.settings.preset_texts.get_mut(i) else {
                    return;
                };
                ui.add(
                    egui::TextEdit::multiline(text)
                        .margin(egui::Margin::symmetric(8, 6))
                        .font(egui::TextStyle::Monospace)
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
            });
        });
    }
    if widgets::secondary_button(ui, &format!("{} New preset", crate::icon::ADD)).clicked() {
        state.toast("New preset — add your own material semantics", ToastKind::Info);
    }
}

fn export(ui: &mut Ui, state: &mut AppState) {
    heading(ui, "Export defaults", "Filename pattern and normal convention for the exported maps.");
    widgets::card(ui, |ui| {
        mono_line(ui, "Filename pattern", &mut state.settings.filename_pattern);
        ui.add_space(8.0);
        indexed_combo(
            ui,
            "norm_conv",
            "Normal convention",
            &["OpenGL (Y+)", "DirectX (Y−)"],
            320.0,
            &mut state.settings.normal_convention,
        );
    });
}

fn library(ui: &mut Ui, state: &mut AppState) {
    heading(
        ui,
        "Library",
        "Lumagen manages your projects and materials; import/export bridges to plain folders.",
    );
    widgets::card(ui, |ui| {
        mono_line(ui, "Library location", &mut state.settings.library_location);
        ui.add_space(8.0);
        mono_line(ui, "Watch folders for import", &mut state.settings.watch_folders);
        ui.add_space(8.0);
        let mut autosave = state.settings.autosave;
        if widgets::toggle_row(ui, &mut autosave, "Autosave materials").changed() {
            state.settings.autosave = autosave;
        }
    });
}

fn interface(ui: &mut Ui, state: &mut AppState) {
    heading(ui, "Interface", "Theme and navigation to match your muscle memory.");
    widgets::card(ui, |ui| {
        let themes = ["Dark (default)", "Darker"];
        // Persisted prefs are clamped on restore, but an out-of-range value must never
        // index past the array even if a newer build shipped more themes.
        state.settings.theme = state.settings.theme.min(themes.len() - 1);
        let theme_before = state.settings.theme;
        indexed_combo(ui, "theme", "Theme", &themes, 320.0, &mut state.settings.theme);
        if state.settings.theme != theme_before {
            crate::theme::set_theme(state.settings.theme);
            crate::theme::apply(ui.ctx());
        }
        ui.add_space(8.0);
        let navs = ["Blender", "Maya"];
        state.settings.nav_preset = state.settings.nav_preset.min(navs.len() - 1);
        indexed_combo(ui, "navpreset", "Viewport navigation", &navs, 320.0, &mut state.settings.nav_preset);
        ui.add_space(8.0);
        widgets::field_label(ui, "Accent", None);
        const ACCENTS: [&str; 4] = ["Blue", "Violet", "Teal", "Amber"];
        let before = state.settings.accent.clone();
        widgets::chip_row(ui, &ACCENTS, &mut state.settings.accent);
        if state.settings.accent != before {
            let accent = state.settings.accent.clone();
            crate::theme::set_accent_name(&accent);
            crate::theme::apply(ui.ctx());
        }
    });
}
