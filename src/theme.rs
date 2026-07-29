//! Lumagen design system — color tokens, visuals, fonts (spec §13).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, Style, TextStyle, Visuals};

// ── Color tokens (spec §13) ──────────────────────────────────────────────
// The four background tokens are theme-dependent (Interface → Theme: Dark / Darker), so they
// are functions that read the active theme rather than consts. Everything else is fixed.
const BG0_DARK: Color32 = Color32::from_rgb(0x14, 0x16, 0x1A); // window / darkest
const BG1_DARK: Color32 = Color32::from_rgb(0x1B, 0x1E, 0x24); // panels
const BG2_DARK: Color32 = Color32::from_rgb(0x22, 0x26, 0x2E); // cards / rows
const BG3_DARK: Color32 = Color32::from_rgb(0x2A, 0x2F, 0x38); // hover
const BG0_DARKER: Color32 = Color32::from_rgb(0x0C, 0x0D, 0x10);
const BG1_DARKER: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
const BG2_DARKER: Color32 = Color32::from_rgb(0x18, 0x1B, 0x21);
const BG3_DARKER: Color32 = Color32::from_rgb(0x20, 0x24, 0x2B);
pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x3A, 0x45);
pub const BORDER_2: Color32 = Color32::from_rgb(0x3A, 0x41, 0x50);

thread_local! {
    static THEME: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Record the active theme index (0 = Dark, 1 = Darker). Called when prefs load or the theme
/// changes; `apply` and the `BG_*` accessors read it.
pub fn set_theme(theme: usize) {
    THEME.with(|t| t.set(theme));
}

fn darker() -> bool {
    THEME.with(std::cell::Cell::get) == 1
}

/// Window / darkest background (theme-aware).
#[allow(non_snake_case)]
pub fn BG_0() -> Color32 {
    if darker() { BG0_DARKER } else { BG0_DARK }
}
/// Panel background (theme-aware).
#[allow(non_snake_case)]
pub fn BG_1() -> Color32 {
    if darker() { BG1_DARKER } else { BG1_DARK }
}
/// Card / row background (theme-aware).
#[allow(non_snake_case)]
pub fn BG_2() -> Color32 {
    if darker() { BG2_DARKER } else { BG2_DARK }
}
/// Hover background (theme-aware).
#[allow(non_snake_case)]
pub fn BG_3() -> Color32 {
    if darker() { BG3_DARKER } else { BG3_DARK }
}

pub const TEXT_HI: Color32 = Color32::from_rgb(0xE8, 0xEB, 0xF0);
pub const TEXT_MID: Color32 = Color32::from_rgb(0xA9, 0xB2, 0xC0);
pub const TEXT_LO: Color32 = Color32::from_rgb(0x6E, 0x77, 0x87);

pub const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x8C, 0xFF);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x33, 0x43, 0x6E);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0xCF, 0xE0, 0xFF);
pub const READY: Color32 = Color32::from_rgb(0x4E, 0xCB, 0x8E);
pub const GENERATING: Color32 = Color32::from_rgb(0x9B, 0x7B, 0xFF);
pub const COST: Color32 = Color32::from_rgb(0xF0, 0xB5, 0x4A);
pub const DANGER: Color32 = Color32::from_rgb(0xF0, 0x68, 0x4A);

pub const R_CARD: u8 = 6;
pub const R_CTL: u8 = 4;

pub const CHECKER_A: Color32 = Color32::from_rgb(0x20, 0x24, 0x2B);
pub const CHECKER_B: Color32 = Color32::from_rgb(0x17, 0x1A, 0x1F);

/// Resolve the accent name (Interface → Accent) to its color triple. Returns
/// `(ACCENT, ACCENT_DIM, ACCENT_TEXT)`; unknown names fall back to the default blue.
pub fn accent_colors(name: &str) -> (Color32, Color32, Color32) {
    match name {
        "Violet" => (
            Color32::from_rgb(0x9B, 0x7B, 0xFF),
            Color32::from_rgb(0x45, 0x38, 0x6E),
            Color32::from_rgb(0xE4, 0xDA, 0xFF),
        ),
        "Teal" => (
            Color32::from_rgb(0x2E, 0xC4, 0xB6),
            Color32::from_rgb(0x1E, 0x50, 0x4C),
            Color32::from_rgb(0xC8, 0xF2, 0xEE),
        ),
        "Amber" => (
            Color32::from_rgb(0xF0, 0xB5, 0x4A),
            Color32::from_rgb(0x5E, 0x47, 0x1E),
            Color32::from_rgb(0xFB, 0xE6, 0xC4),
        ),
        // "Blue" (default) and anything unrecognized.
        _ => (ACCENT, ACCENT_DIM, ACCENT_TEXT),
    }
}

/// The accent name chosen in Interface settings, read back from the context's persisted
/// memory so `apply` (which runs before app state exists) can pick the right palette. Defaults
/// to "Blue" on first run. The settings UI writes this whenever the accent changes.
pub fn current_accent_name() -> String {
    ACCENT_NAME.with(|n| n.borrow().clone())
}

thread_local! {
    static ACCENT_NAME: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Record the active accent name (called by the app when prefs load or the accent changes).
pub fn set_accent_name(name: &str) {
    ACCENT_NAME.with(|n| *n.borrow_mut() = name.to_string());
}

/// The active accent color (Interface → Accent). UI code should call this instead of the
/// static `ACCENT` const so the user's choice shows everywhere.
pub fn accent() -> Color32 {
    accent_colors(&current_accent_name()).0
}

/// The active accent's dim variant.
pub fn accent_dim() -> Color32 {
    accent_colors(&current_accent_name()).1
}

/// The active accent's on-accent text color.
pub fn accent_text() -> Color32 {
    accent_colors(&current_accent_name()).2
}

/// Apply the full Lumagen style to the egui context.
pub fn apply(ctx: &egui::Context) {
    // Register the Phosphor icon fonts, one per variant, each under its own named family
    // (regular for most glyphs, bold for emphasis, fill for solid accents), and as a fallback
    // on the proportional family so a glyph still renders if a style doesn't name a family.
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("phosphor-regular".into(), egui_phosphor::Variant::Regular.font_data().into());
    fonts.font_data.insert("phosphor-bold".into(), egui_phosphor::Variant::Bold.font_data().into());
    fonts.font_data.insert("phosphor-fill".into(), egui_phosphor::Variant::Fill.font_data().into());
    fonts
        .families
        .insert(egui::FontFamily::Name("phosphor-regular".into()), vec!["phosphor-regular".into()]);
    fonts
        .families
        .insert(egui::FontFamily::Name("phosphor-bold".into()), vec!["phosphor-bold".into()]);
    fonts
        .families
        .insert(egui::FontFamily::Name("phosphor-fill".into()), vec!["phosphor-fill".into()]);
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.push("phosphor-regular".into());
        prop.push("phosphor-bold".into());
        prop.push("phosphor-fill".into());
    }
    // Monospace too: labels like the seed row mix mono text with an icon glyph, which
    // otherwise renders as a tofu box.
    if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        mono.push("phosphor-regular".into());
    }
    ctx.set_fonts(fonts);

    let mut style = Style::default();
    let mut visuals = Visuals::dark();

    visuals.dark_mode = true;
    visuals.panel_fill = BG_1();
    visuals.window_fill = BG_1();
    visuals.extreme_bg_color = BG_0();
    visuals.faint_bg_color = BG_2();
    visuals.override_text_color = Some(TEXT_HI);
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.window_corner_radius = CornerRadius::same(R_CARD);

    visuals.widgets.noninteractive.bg_fill = BG_2();
    visuals.widgets.noninteractive.weak_bg_fill = BG_1();
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_MID);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(R_CTL);

    visuals.widgets.inactive.bg_fill = BG_2();
    visuals.widgets.inactive.weak_bg_fill = BG_2();
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_HI);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(R_CTL);

    visuals.widgets.hovered.bg_fill = BG_3();
    visuals.widgets.hovered.weak_bg_fill = BG_3();
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, BORDER_2);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_HI);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(R_CTL);

    // The accent is user-selectable (Interface → Accent); resolve it once and thread it
    // through the style pieces that read it. The static `ACCENT*` consts stay as the default
    // for code that paints directly.
    let name = current_accent_name();
    let (accent, accent_dim, accent_text) = accent_colors(if name.is_empty() { "Blue" } else { &name });

    visuals.widgets.active.bg_fill = BG_3();
    visuals.widgets.active.weak_bg_fill = BG_3();
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, TEXT_HI);
    visuals.widgets.active.corner_radius = CornerRadius::same(R_CTL);

    visuals.selection.bg_fill = accent_dim;
    visuals.selection.stroke = Stroke::new(1.0_f32, accent_text);

    visuals.hyperlink_color = accent;
    visuals.striped = false;

    style.visuals = visuals;

    style.text_styles = [
        (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Name("panel".into()), FontId::new(11.0, FontFamily::Proportional)),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    // Taller interactive widgets (buttons, and the single-line TextEdits that size off
    // interact_size.y) so text sits comfortably centered instead of clipped at the top.
    style.spacing.interact_size = egui::vec2(20.0, 26.0);
    style.spacing.window_margin = egui::Margin::same(0);
    style.spacing.scroll = egui::style::ScrollStyle::solid();

    ctx.set_global_style(style);
}
