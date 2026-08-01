//! Library screen (spec §6): projects sidebar + material grid with coverage pips.

use eframe::egui::{self, Align2, Color32, FontId, Frame, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::state::{AppState, Screen};
use crate::theme::*;
use crate::widgets::{self, Pip};

/// Material-class filter chips.
const CLASSES: [&str; 7] = ["All", "Metal", "Painted", "Glass", "Fabric", "Stone", "Wood"];

/// How many materials the "Recent" view shows (the library is recency-ordered).
const RECENT_COUNT: usize = 5;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    // Scan the library once per session, on the worker — per-document zstd + MessagePack
    // loads must not block a frame.
    if !state.library_scanned {
        state.library_scanned = true;
        let ctx = ui.ctx().clone();
        state.request_library_scan(&ctx);
    }
    egui::Panel::top("lib_head")
        .exact_size(52.0)
        .frame(Frame::new().fill(BG_1()).stroke(Stroke::new(1.0_f32, BORDER)))
        .show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.add(crate::brand::wordmark(22.0));
                ui.label(RichText::new("· Library").size(13.0).color(TEXT_MID));
                if state.library_scanning {
                    ui.add_space(8.0);
                    ui.label(RichText::new("scanning…").size(11.0).color(TEXT_LO));
                    ui.spinner();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.0);
                    if widgets::icon_button(ui, crate::icon::SETTINGS, "Settings  (Ctrl+,)").clicked() {
                        state.settings.return_to = Screen::Library;
                        state.screen = Screen::Settings;
                    }
                    ui.add_space(6.0);
                    if widgets::primary_button(ui, &format!("{} New material", crate::icon::ADD)).clicked() {
                        state.create.stage = crate::state::CreateStage::Choice;
                        state.show_create = true;
                    }
                    ui.add_space(6.0);
                    if widgets::secondary_button(ui, &format!("{} New project", crate::icon::NEW_PROJECT)).clicked() {
                        state.new_project_name.clear();
                        state.show_new_project = true;
                    }
                    ui.add_space(6.0);
                    if widgets::secondary_button(ui, &format!("{} Import from folder", crate::icon::IMPORT)).clicked()
                        && let Some(dir) = rfd::FileDialog::new().set_title("Import material from folder").pick_folder()
                    {
                        let ctx = ui.ctx().clone();
                        let epoch = state.material_epoch;
                        state.submit_io(&ctx, crate::render::IoTask::ImportFolder { dir, epoch });
                    }
                });
            });
        });

    egui::Panel::left("lib_side")
        .exact_size(236.0)
        .resizable(false)
        .frame(Frame::new().fill(BG_1()).stroke(Stroke::new(1.0_f32, BORDER)))
        .show_inside(ui, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                crate::icon::label(ui, crate::icon::SEARCH, 13.0, TEXT_LO);
                ui.add(
                    egui::TextEdit::singleline(&mut state.lib_search)
                        .margin(egui::Margin::symmetric(8, 6))
                        .hint_text(RichText::new("Search materials…").color(TEXT_LO))
                        .desired_width(180.0),
                );
            });
            ui.add_space(10.0);
            nav_item(ui, &mut state.lib_nav, "All materials", state.library.len());
            nav_item(ui, &mut state.lib_nav, "Recent", state.library.len().min(RECENT_COUNT));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                widgets::section_header(ui, "Projects");
            });
            ui.add_space(4.0);
            for p in &state.projects {
                let count = state.library.iter().filter(|m| m.project == p.name).count();
                // Selecting a project also makes it the ACTIVE one — new materials are
                // filed under it, not silently saved with no project.
                if nav_item(ui, &mut state.lib_nav, &p.name, count) {
                    state.project_name.clone_from(&p.name);
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                widgets::section_header(ui, "Material class");
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    // Constrain the chip row to the sidebar width so the class chips wrap
                    // instead of overflowing the panel edge.
                    ui.set_max_width(ui.available_width() - 14.0);
                    widgets::chip_row(ui, &CLASSES, &mut state.lib_class);
                });
            });
        });

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_0()).inner_margin(egui::Margin::symmetric(20, 16)))
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(state.lib_nav.clone()).size(15.0).strong());
                let n = filtered(state).len();
                ui.label(
                    RichText::new(format!("· {} material{}", n, if n == 1 { "" } else { "s" }))
                        .size(12.0)
                        .color(TEXT_LO),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let sorts = ["Sort: Recent", "Sort: Name", "Sort: Coverage"];
                    egui::ComboBox::from_id_salt("libsort")
                        .selected_text(RichText::new(sorts[state.lib_sort.min(sorts.len() - 1)]).size(12.0).color(TEXT_MID))
                        .show_ui(ui, |ui| {
                            for (i, s) in sorts.iter().enumerate() {
                                ui.selectable_value(&mut state.lib_sort, i, *s);
                            }
                        });
                });
            });
            ui.add_space(14.0);

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mats = filtered(state);
                if mats.is_empty() {
                    empty_library(ui, state);
                    return;
                }
                let min_tile = 240.0f32;
                let gap = 18.0f32;
                let width = ui.available_width();
                let cols = ((width + gap) / (min_tile + gap)).floor().max(1.0) as usize;
                let tile_w = ((width - gap * (cols as f32 - 1.0)) / cols as f32).floor();

                // Split the borrows: the grid reads the (borrowed) filtered materials and
                // the preview-texture map, and only records the clicked slug. Mutation of
                // `state` happens after the loop, once those borrows are dropped — so the
                // filtered view never has to deep-clone the library.
                let mut clicked_slug: Option<String> = None;
                {
                    let lib_previews = &state.lib_previews;
                    let opening = state.opening.as_deref();
                    egui::Grid::new("mat_grid").num_columns(cols).spacing(Vec2::splat(gap)).show(ui, |ui| {
                        for (i, m) in mats.iter().enumerate() {
                            // The clicked card shows a spinner while its document opens on
                            // the worker; further clicks are ignored until it lands.
                            let is_opening = opening == Some(m.slug.as_str());
                            if material_tile(ui, lib_previews, m, tile_w, is_opening) && opening.is_none() {
                                clicked_slug = Some(m.slug.clone());
                            }
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
                }
                if let Some(slug) = clicked_slug {
                    let ctx = ui.ctx().clone();
                    state.open_material(&ctx, &slug);
                }
            });
        });
}

/// Friendly empty state: distinguishes "no materials at all" from "nothing matches the filter".
fn empty_library(ui: &mut egui::Ui, state: &mut AppState) {
    let filtered_out = !state.library.is_empty();
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        crate::icon::label(ui, crate::icon::SEARCH, 28.0, TEXT_LO);
        ui.add_space(8.0);
        if filtered_out {
            ui.label(RichText::new("No materials match").size(14.0).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("Try a different search, class, or project filter.").size(12.0).color(TEXT_MID));
        } else {
            ui.label(RichText::new("Your library is empty").size(14.0).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("Create a material to get started.").size(12.0).color(TEXT_MID));
            ui.add_space(12.0);
            if widgets::primary_button(ui, &format!("{} New material", crate::icon::ADD)).clicked() {
                state.create.stage = crate::state::CreateStage::Choice;
                state.show_create = true;
            }
        }
    });
}

/// Borrowed view of the library, filtered/sorted — no per-frame deep clone. The library is
/// recency-ordered, so "Recent" is simply the first few entries; a project nav filters by the
/// material's project.
fn filtered(state: &AppState) -> Vec<&crate::state::LibMaterial> {
    let search = state.lib_search.to_lowercase();
    let nav = state.lib_nav.as_str();
    let mut mats: Vec<&crate::state::LibMaterial> = state
        .library
        .iter()
        .filter(|m| {
            let class_ok = state.lib_class == "All" || m.class == state.lib_class;
            let search_ok = search.is_empty() || m.slug.to_lowercase().contains(&search);
            let nav_ok = matches!(nav, "All materials" | "Recent") || m.project == nav;
            class_ok && search_ok && nav_ok
        })
        .collect();
    if nav == "Recent" {
        mats.truncate(RECENT_COUNT);
    }
    match state.lib_sort {
        1 => mats.sort_by(|a, b| a.slug.cmp(&b.slug)),
        2 => mats.sort_by(|a, b| b.coverage.cmp(&a.coverage)),
        _ => {}
    }
    mats
}

/// A sidebar navigation row: label left, count right, fixed height. Painted directly (no
/// nested layouts) so it can never expand to fill the panel. Returns true on click, after
/// making `label` the active nav.
fn nav_item(ui: &mut egui::Ui, lib_nav: &mut String, label: &str, count: usize) -> bool {
    let on = lib_nav.as_str() == label;
    let (outer, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
    let rect = outer.shrink2(Vec2::new(8.0, 1.0));
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, R_CTL, BG_2());
    } else if resp.hovered() {
        painter.rect_filled(rect, R_CTL, BG_2().gamma_multiply(0.6));
    }
    if on {
        let bar = Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 5.0), Vec2::new(2.0, rect.height() - 10.0));
        painter.rect_filled(bar, 1.0, accent());
    }
    let count_str = count.to_string();
    let count_galley = painter.layout_no_wrap(count_str, FontId::monospace(11.0), TEXT_LO);
    let count_w = count_galley.size().x;
    painter.galley(
        Pos2::new(rect.max.x - 10.0 - count_w, rect.center().y - count_galley.size().y / 2.0),
        count_galley,
        TEXT_LO,
    );
    let max_label_w = rect.width() - 22.0 - count_w - 14.0;
    let label_text = truncate_to_width(painter, label, &FontId::proportional(12.5), max_label_w);
    painter.text(
        Pos2::new(rect.min.x + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        label_text,
        FontId::proportional(12.5),
        if on { TEXT_HI } else { TEXT_MID },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *lib_nav = label.to_string();
    }
    resp.clicked()
}

/// Elide `text` with `…` so it fits in `max_w` points.
fn truncate_to_width(painter: &egui::Painter, text: &str, font: &FontId, max_w: f32) -> String {
    let full = painter.layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE);
    if full.size().x <= max_w {
        return text.to_string();
    }
    let mut s: String = text.to_string();
    while !s.is_empty() {
        s.pop();
        let g = painter.layout_no_wrap(format!("{s}…"), font.clone(), Color32::WHITE);
        if g.size().x <= max_w {
            return format!("{s}…");
        }
    }
    "…".into()
}

/// Render one library tile. Takes the preview-texture map directly (not `&mut AppState`)
/// so the borrowed filtered view can drive it. Returns true when the tile was clicked.
///
/// The tile lives inside an `egui::Grid` cell, whose content flows left-to-right — every row
/// here is painted into an explicitly allocated full-width rect inside a `ui.vertical`, so
/// the stack can never collapse into a horizontal strip.
fn material_tile(
    ui: &mut egui::Ui,
    lib_previews: &std::collections::HashMap<String, egui::TextureHandle>,
    m: &crate::state::LibMaterial,
    width: f32,
    opening: bool,
) -> bool {
    let pad = 12.0;
    let frame = Frame::new().fill(BG_1()).stroke(Stroke::new(1.0_f32, BORDER)).corner_radius(8);
    let resp = frame
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                // ── preview band ──
                let img_h = (width * 0.62).min(180.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, img_h), Sense::hover());
                if let Some(tex) = lib_previews.get(&m.slug) {
                    ui.painter()
                        .image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                } else {
                    ui.painter().rect_filled(rect, 0.0, BG_3());
                    ui.painter()
                        .text(rect.center(), Align2::CENTER_CENTER, "rendering…", FontId::proportional(10.5), TEXT_LO);
                }
                if opening {
                    ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(120));
                    egui::Spinner::new()
                        .size(26.0)
                        .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(26.0)));
                    ui.painter().text(
                        Pos2::new(rect.center().x, rect.center().y + 24.0),
                        Align2::CENTER_CENTER,
                        "opening…",
                        FontId::proportional(10.5),
                        TEXT_MID,
                    );
                }

                // ── title row: slug (truncated) + class chip ──
                let (row, _) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::hover());
                let painter = ui.painter();
                let chip_galley = painter.layout_no_wrap(m.class.clone(), FontId::proportional(9.5), TEXT_MID);
                let chip_w = chip_galley.size().x + 12.0;
                let chip_rect = Rect::from_center_size(Pos2::new(row.max.x - pad - chip_w / 2.0, row.center().y), Vec2::new(chip_w, 18.0));
                painter.rect_filled(chip_rect, 3.0, BG_3());
                painter.galley(chip_rect.center() - chip_galley.size() / 2.0, chip_galley, TEXT_MID);
                let max_slug_w = chip_rect.min.x - row.min.x - pad - 8.0;
                let slug = truncate_to_width(painter, &m.slug, &FontId::proportional(12.5), max_slug_w);
                painter.text(
                    Pos2::new(row.min.x + pad, row.center().y),
                    Align2::LEFT_CENTER,
                    slug,
                    FontId::proportional(12.5),
                    TEXT_HI,
                );

                // ── coverage pips ──
                let (prow, _) = ui.allocate_exact_size(Vec2::new(width, 6.0), Sense::hover());
                let pips: Vec<Pip> = (0..8)
                    .map(|i| {
                        if Some(i) == m.stale_index {
                            Pip::Stale
                        } else if (i as u8) < m.coverage {
                            Pip::Present
                        } else {
                            Pip::Missing
                        }
                    })
                    .collect();
                widgets::paint_coverage_pips(ui.painter(), prow.shrink2(Vec2::new(pad, 1.0)), &pips);

                // ── meta row: coverage text + resolution ──
                let (mrow, _) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::hover());
                let cov = match m.coverage {
                    8 => "Complete".to_string(),
                    0 => "Albedo only".to_string(),
                    n => format!("{} / 8 maps", n),
                };
                ui.painter().text(
                    Pos2::new(mrow.min.x + pad, mrow.center().y),
                    Align2::LEFT_CENTER,
                    cov,
                    FontId::monospace(10.5),
                    TEXT_LO,
                );
                ui.painter().text(
                    Pos2::new(mrow.max.x - pad, mrow.center().y),
                    Align2::RIGHT_CENTER,
                    &m.res,
                    FontId::monospace(10.5),
                    TEXT_LO,
                );
            });
        })
        .response
        .interact(Sense::click());
    if resp.hovered() {
        ui.painter().rect_stroke(resp.rect, 8, Stroke::new(1.0_f32, accent()), egui::StrokeKind::Inside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}
