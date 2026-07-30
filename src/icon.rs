//! Central icon set (egui-phosphor). All glyphs the UI draws go through here so icons are
//! consistent in family, size, and color.

use eframe::egui::{self, Color32, RichText, Ui};

pub use egui_phosphor::regular;

/// An icon glyph as `RichText` at the given size and color.
pub fn text(glyph: &'static str, size: f32, color: Color32) -> RichText {
    RichText::new(glyph).size(size).color(color)
}

/// Add a centered icon as a label (non-interactive) at a given size/color.
pub fn label(ui: &mut Ui, glyph: &'static str, size: f32, color: Color32) -> egui::Response {
    ui.label(text(glyph, size, color))
}

// ── The app's icon vocabulary ──────────────────────────────────────────────
// Named for what the icon *means* in Lumagen, so call sites read semantically.

pub const SETTINGS: &str = regular::GEAR;
pub const CLOSE: &str = regular::X;
pub const GENERATE: &str = regular::SPARKLE;
pub const REGENERATE: &str = regular::ARROW_COUNTER_CLOCKWISE;
pub const EXPORT: &str = regular::EXPORT;
pub const DOWNLOAD: &str = regular::DOWNLOAD_SIMPLE;
pub const IMPORT: &str = regular::FOLDER;
pub const REPLACE_FILE: &str = regular::FILE_ARROW_UP;
pub const LOCKED: &str = regular::LOCK;
pub const UNLOCKED: &str = regular::LOCK_OPEN;
pub const ADD: &str = regular::PLUS;
pub const CONFIRM: &str = regular::CHECK;
pub const WARNING: &str = regular::WARNING_CIRCLE;
pub const BACK: &str = regular::ARROW_LEFT;
pub const EXPAND: &str = regular::CARET_UP;
pub const COLLAPSE: &str = regular::CARET_DOWN;
pub const FIT: &str = regular::FRAME_CORNERS;
pub const BEFORE_AFTER: &str = regular::CIRCLE_HALF;
pub const ORBIT: &str = regular::ARROW_CLOCKWISE;
pub const VARIATIONS: &str = regular::SQUARES_FOUR;
pub const SEARCH: &str = regular::MAGNIFYING_GLASS;
pub const MATERIAL_3D: &str = regular::CUBE;
pub const NEW_PROJECT: &str = regular::FOLDER;
pub const RANDOMIZE: &str = regular::ARROWS_CLOCKWISE;
pub const EDIT: &str = regular::PENCIL_SIMPLE;
