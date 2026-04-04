// ─── ui/theme.rs ─────────────────────────────────────────────────────────────
#![allow(dead_code)] // palette constants used as features are added
// Grayscale palette — all colors are off-tint R=G=B.
// Color channels will later be used for highlights/accents.

use egui::{Color32, FontId, Rounding, Shadow, Stroke, Style, Visuals};

// ─── Palette ──────────────────────────────────────────────────────────────────

pub const VOID: Color32 = Color32::from_rgb(8, 8, 8); // near black
pub const DEEP: Color32 = Color32::from_rgb(18, 18, 18); // bg panels
pub const PIT: Color32 = Color32::from_rgb(28, 28, 28); // widget bg
pub const SLATE: Color32 = Color32::from_rgb(40, 40, 40); // borders
pub const IRON: Color32 = Color32::from_rgb(60, 60, 60); // inactive
pub const ASH: Color32 = Color32::from_rgb(90, 90, 90); // mid
pub const SMOKE: Color32 = Color32::from_rgb(130, 130, 130); // secondary text
pub const FOG: Color32 = Color32::from_rgb(175, 175, 175); // primary text
pub const HAZE: Color32 = Color32::from_rgb(210, 210, 210); // bright text
pub const CHALK: Color32 = Color32::from_rgb(235, 235, 235); // highlights
pub const GHOST: Color32 = Color32::from_rgb(255, 255, 255); // active elements

// Accent colors for active/hot states (subtle, not pure-color)
pub const HOT: Color32 = Color32::from_rgb(210, 210, 210); // bright accent (alias HAZE)
pub const ACTIVE_STEP: Color32 = Color32::from_rgb(200, 200, 200); // active step bg
pub const CURSOR: Color32 = Color32::from_rgb(240, 240, 240); // sequencer cursor

// ─── Huth Farbige Noten — note-to-color mapping ──────────────────────────────
// Ch. A. B. Huth, *Farbige Noten*, Hamburg 1888–1889.
// 12 chromatic semitones mapped counter-clockwise on the RYB wheel, starting
// from Blue at C. One octave = 360° = one full circuit. See docs/colorful-notes.md.

pub const NOTE_COLORS: [Color32; 12] = [
    Color32::from_rgb(0x33, 0x66, 0xDD), // C  — BLU  Blue
    Color32::from_rgb(0x22, 0x99, 0xBB), // C# — SE   Seegrün (cyan-blue)
    Color32::from_rgb(0x33, 0xAA, 0x66), // D  — VER  Vert (green/teal)
    Color32::from_rgb(0x88, 0xCC, 0x22), // D# — MO   Yellow-green
    Color32::from_rgb(0xDD, 0xCC, 0x22), // E  — GEL  Gelb (yellow)
    Color32::from_rgb(0xEE, 0x88, 0x22), // F  — OR   Orange
    Color32::from_rgb(0xDD, 0x44, 0x22), // F# — NER  Vermilion
    Color32::from_rgb(0xEE, 0x33, 0x66), // G  — ROS  Rose
    Color32::from_rgb(0xCC, 0x11, 0x44), // G# — CAR  Carmine
    Color32::from_rgb(0x99, 0x66, 0xCC), // A  — LIL  Lila (lilac-violet)
    Color32::from_rgb(0x77, 0x44, 0xBB), // A# — PEN  Pensée (purple-violet)
    Color32::from_rgb(0x44, 0x33, 0xAA), // B  — IN   Indigo
];

/// Return the Huth color for any MIDI note (wraps at octave boundary).
pub fn note_color(midi_note: u8) -> Color32 {
    NOTE_COLORS[(midi_note % 12) as usize]
}

// ─── Apply theme to egui context ─────────────────────────────────────────────

#[allow(clippy::field_reassign_with_default)] // egui Style has no partial-init API
pub fn apply(ctx: &egui::Context) {
    let mut style = Style::default();

    // Typography
    style.text_styles = [
        (egui::TextStyle::Small, FontId::monospace(9.0)),
        (egui::TextStyle::Body, FontId::monospace(11.0)),
        (egui::TextStyle::Button, FontId::monospace(11.0)),
        (egui::TextStyle::Heading, FontId::monospace(13.0)),
        (egui::TextStyle::Monospace, FontId::monospace(11.0)),
    ]
    .into();

    // Spacing
    style.spacing.item_spacing = egui::vec2(4.0, 4.0);
    style.spacing.button_padding = egui::vec2(6.0, 3.0);
    style.spacing.window_margin = egui::Margin::same(8.0);
    style.spacing.slider_width = 80.0;
    style.spacing.interact_size = egui::vec2(36.0, 18.0);

    // Visuals
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(FOG);
    visuals.window_fill = DEEP;
    visuals.panel_fill = DEEP;
    visuals.faint_bg_color = DEEP;
    visuals.extreme_bg_color = VOID;
    visuals.window_stroke = Stroke::new(1.0, SLATE);
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;
    visuals.window_rounding = Rounding::same(3.0);
    visuals.menu_rounding = Rounding::same(3.0);

    // Widget styles
    let w = &mut visuals.widgets;

    w.noninteractive.bg_fill = PIT;
    w.noninteractive.bg_stroke = Stroke::new(1.0, SLATE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, SMOKE);
    w.noninteractive.rounding = Rounding::same(2.0);

    w.inactive.bg_fill = PIT;
    w.inactive.bg_stroke = Stroke::new(1.0, IRON);
    w.inactive.fg_stroke = Stroke::new(1.0, ASH);
    w.inactive.rounding = Rounding::same(2.0);

    w.hovered.bg_fill = SLATE;
    w.hovered.bg_stroke = Stroke::new(1.0, ASH);
    w.hovered.fg_stroke = Stroke::new(1.5, FOG);
    w.hovered.rounding = Rounding::same(2.0);

    w.active.bg_fill = IRON;
    w.active.bg_stroke = Stroke::new(1.0, FOG);
    w.active.fg_stroke = Stroke::new(2.0, CHALK);
    w.active.rounding = Rounding::same(2.0);

    w.open.bg_fill = IRON;
    w.open.bg_stroke = Stroke::new(1.0, FOG);
    w.open.fg_stroke = Stroke::new(1.5, CHALK);
    w.open.rounding = Rounding::same(2.0);

    visuals.selection.bg_fill = IRON;
    visuals.selection.stroke = Stroke::new(1.0, CHALK);
    visuals.hyperlink_color = CHALK;

    style.visuals = visuals;
    ctx.set_style(style);
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Interpolate between two gray shades.
pub fn lerp_gray(a: u8, b: u8, t: f32) -> Color32 {
    let v = (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8;
    Color32::from_rgb(v, v, v)
}

/// Linearly interpolate between two colors.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}

/// Dim text for labels.
pub fn label_color() -> Color32 {
    SMOKE
}
/// Bright text for values.
pub fn value_color() -> Color32 {
    HAZE
}
/// Panel header color.
pub fn header_color() -> Color32 {
    FOG
}

// ─── Glass panel frame ────────────────────────────────────────────────────────

/// Draw a smoked-glass group background behind `rect`.
/// Call from a `ui.painter()` *before* the group content so the content sits on top.
///
/// Effect: very dark fill + 1px bright top border + 1px dark bottom border —
/// the "edge of smoked glass" look from the neumorphic chrome spec.
pub fn draw_glass_panel(painter: &egui::Painter, rect: egui::Rect, rounding: egui::Rounding) {
    // Dark glass fill
    painter.rect_filled(rect, rounding, Color32::from_gray(15));
    // 1px bright top border
    painter.line_segment(
        [rect.left_top(), rect.right_top()],
        Stroke::new(1.0, Color32::from_gray(64)),
    );
    // 1px dark bottom border
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(8)),
    );
    // 1px dim side borders
    painter.line_segment(
        [rect.left_top(), rect.left_bottom()],
        Stroke::new(1.0, Color32::from_gray(28)),
    );
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, Color32::from_gray(28)),
    );
}
