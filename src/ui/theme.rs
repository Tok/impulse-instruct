// ─── ui/theme.rs ─────────────────────────────────────────────────────────────
#![allow(dead_code)] // palette constants used as features are added
// Grayscale palette — all colors are off-tint R=G=B.
// Color channels will later be used for highlights/accents.

use egui::{Color32, FontId, Pos2, Rect, Rounding, Shadow, Stroke, Style, Vec2, Visuals};

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

// ─── Huth Warme/Kalte Töne — per-semitone warm/cold scalar ───────────────────
// Per TAFEL VI: warm pole = Yellow→Orange→Red, cold pole = Blue→Cyan→Green,
// bridge = Rose→Carmine→Lilac→Indigo (warm back to cold).
// Derived as cos(hue − 60°), with 60° (Orange/F) the warm pole and 240°
// (Blue/C) the cold pole. Values clamped to [-1.0, 1.0]; Huth's stated
// hue angles produce the exact ranking he describes.

pub const NOTE_TEMP: [f32; 12] = [
    -1.00, // C  — Blue        (cold)
    -0.87, // C# — Sea-green
    -0.50, // D  — Vert/Teal
    0.00,  // D# — Yellow-green (neutral)
    0.50,  // E  — Yellow
    1.00,  // F  — Orange      (warm)
    0.87,  // F# — Vermilion
    0.34,  // G  — Rose
    -0.17, // G# — Carmine     (bridge)
    -0.64, // A  — Lilac
    -0.91, // A# — Pensée
    -1.00, // B  — Indigo
];

/// Per-semitone warm/cold value in [-1.0, +1.0] (-1 cold, +1 warm).
pub fn note_temperature(midi_note: u8) -> f32 {
    NOTE_TEMP[(midi_note % 12) as usize]
}

/// Color a temperature value [-1, +1] on the cold-blue → neutral → warm-orange axis.
/// Matches the Huth note-color palette (uses C-blue and F-orange as poles).
pub fn temperature_color(temp: f32) -> Color32 {
    let t = temp.clamp(-1.0, 1.0);
    let neutral = Color32::from_rgb(140, 140, 140);
    if t < 0.0 {
        lerp_color(neutral, NOTE_COLORS[0], -t) // toward C-blue
    } else {
        lerp_color(neutral, NOTE_COLORS[5], t) // toward F-orange
    }
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

    // Suppress egui's debug-build overlays: the "first use of ID …" red
    // labels on widget-id collisions and the debug-on-hover red boxes.
    // These show up even on cargo-build debug (which is most of the time
    // for our dev loop) and clutter the UI.
    ctx.options_mut(|opt| {
        opt.warn_on_id_clash = false;
    });
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
        Stroke::new(1.0, VOID),
    );
    // 1px dim side borders
    painter.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, PIT));
    painter.line_segment(
        [rect.right_top(), rect.right_bottom()],
        Stroke::new(1.0, PIT),
    );
}

// ─── LED dot ──────────────────────────────────────────────────────────────────
//
// Skeuomorphic indicator LED: outer halo + soft glow + saturated core + dark
// housing rim + bright specular highlight near top-left.  All component
// alphas are scaled by `intensity` (0–1) so the same call site can render
// dim/distant LEDs (low intensity) and bright/recent ones (high intensity).
//
// The Huth note palette is muted by design (≤80% saturation), so for a
// believable LED we ALSO brighten the core toward white as intensity rises —
// real LEDs bloom toward white at their hot spot.

/// 16-ring LED falloff: (radius_multiplier, alpha).  Twice as many rings as
/// the previous 8-ring version for a smoother gradient, with the alpha curve
/// reshaped so the halo fades to translucent quicker — the lit core stays
/// bright but the bloom is gentler and stops competing with surrounding
/// panel chrome.  Outermost rings are barely visible.
const LED_RING_LAYERS: [(f32, u8); 16] = [
    (5.0, 1), // outermost — barely there
    (4.5, 2),
    (4.0, 4),
    (3.6, 6),
    (3.2, 9),
    (2.9, 14),
    (2.6, 20),
    (2.35, 28),
    (2.1, 38),
    (1.9, 50),
    (1.7, 65),
    (1.55, 85),
    (1.4, 110),
    (1.25, 145),
    (1.12, 190),
    (1.0, 255), // saturated core
];

/// Draw a 3D-looking glowing LED at `center` (panels, sequencer, modules).
/// `radius` is the visible core radius; halo extends to ~3.2×.  `intensity`
/// 0..=1 fades every layer together.
pub fn led(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, intensity: f32) {
    let i = intensity.clamp(0.0, 1.0);
    if i <= 0.0 || radius <= 0.0 {
        return;
    }
    let scale = |a: u8| -> u8 { (a as f32 * i) as u8 };
    let rgba = |r: u8, g: u8, b: u8, a: u8| Color32::from_rgba_unmultiplied(r, g, b, scale(a));

    // 16 concentric rings, outside-in
    for &(mult, alpha) in &LED_RING_LAYERS {
        painter.circle_filled(
            center,
            radius * mult,
            rgba(color.r(), color.g(), color.b(), alpha),
        );
    }
    // Hot spot — core brightened toward white
    let hot = lerp_color(color, Color32::WHITE, 0.35);
    painter.circle_filled(center, radius * 0.65, rgba(hot.r(), hot.g(), hot.b(), 200));
    // Dark housing rim
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(0.8, Color32::from_rgba_unmultiplied(0, 0, 0, scale(150))),
    );
    // Specular highlight near top-left (the "3D dome" reflection)
    let spec_off = Vec2::new(-radius * 0.32, -radius * 0.32);
    painter.circle_filled(
        center + spec_off,
        (radius * 0.32).max(0.7),
        rgba(255, 255, 255, 200),
    );
}

/// "Dark LED" — inverse-light variant for use on bright surfaces (e.g. white
/// piano keys when Huth coloring is disabled).  Same dome geometry as
/// [`led`] but every ring darkens the surface instead of brightening it,
/// and the hot spot trends toward true black.  The top-left specular
/// highlight is kept (the lens dome is still glass).
pub fn led_dark(painter: &egui::Painter, center: Pos2, radius: f32, intensity: f32) {
    let i = intensity.clamp(0.0, 1.0);
    if i <= 0.0 || radius <= 0.0 {
        return;
    }
    let scale = |a: u8| -> u8 { (a as f32 * i) as u8 };
    let black = |a: u8| -> Color32 { Color32::from_rgba_unmultiplied(0, 0, 0, scale(a)) };

    // 16 concentric rings — same geometry as `led`, all in BLACK.  On a
    // bright key this paints a soft dark halo and a near-solid dark core.
    for &(mult, alpha) in &LED_RING_LAYERS {
        painter.circle_filled(center, radius * mult, black(alpha));
    }
    // Hot spot — pull the centre even darker (the "deep crystal").
    painter.circle_filled(center, radius * 0.6, black(220));
    // Outer rim (a hair lighter than the surrounding dark ring) so the
    // dome reads as a physical object, not just a hole.
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(0.7, Color32::from_rgba_unmultiplied(40, 40, 40, scale(180))),
    );
    // Specular highlight near top-left (glass dome catches light).
    let spec_off = Vec2::new(-radius * 0.32, -radius * 0.32);
    painter.circle_filled(
        center + spec_off,
        (radius * 0.32).max(0.7),
        Color32::from_rgba_unmultiplied(255, 255, 255, scale(180)),
    );
}

/// Flat (2D) LED for in-display use: inside the event stream, oscilloscopes,
/// or anywhere a saturated coloured dot should not look like a physical
/// raised dome.  No halo, no off-centre specular — just a saturated core
/// with a concentric brightening so the centre still reads as "lit".
pub fn led_flat(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
    intensity: f32,
) {
    let i = intensity.clamp(0.0, 1.0);
    if i <= 0.0 || radius <= 0.0 {
        return;
    }
    let scale = |a: u8| -> u8 { (a as f32 * i) as u8 };
    let rgba = |r: u8, g: u8, b: u8, a: u8| Color32::from_rgba_unmultiplied(r, g, b, scale(a));

    // Saturated core
    painter.circle_filled(center, radius, rgba(color.r(), color.g(), color.b(), 255));
    // Concentric centre brightening — toward white, no offset
    let hot = lerp_color(color, Color32::WHITE, 0.45);
    painter.circle_filled(center, radius * 0.5, rgba(hot.r(), hot.g(), hot.b(), 220));
}

// ─── Screen bezel ────────────────────────────────────────────────────────────
//
// Recessed-screen look used for the event stream + oscilloscopes.  Deep void
// fill with a 1-px highlight on the top edge and a 1-px shadow on the bottom
// edge so the rectangle reads as "set into" the surrounding panel.

/// Paint a recessed CRT/screen-style background into `rect` with a custom
/// fill colour (use `VOID` for the deepest screens, `DEEP` for slightly
/// lighter "panel" surfaces like the global log).  Call before rendering
/// the contents so they sit on top of the fill.
pub fn draw_screen_panel(painter: &egui::Painter, rect: Rect, rounding: Rounding, fill: Color32) {
    // Custom fill
    painter.rect_filled(rect, rounding, fill);
    // Subtle dark outer frame
    painter.rect_stroke(rect, rounding, Stroke::new(1.0, Color32::from_gray(2)));
    // 1-px top inner highlight (light hits the top edge of the recess)
    let inner = rect.shrink(1.0);
    painter.line_segment(
        [inner.left_top(), inner.right_top()],
        Stroke::new(0.8, Color32::from_gray(38)),
    );
    // 1-px bottom inner shadow (recess shadow at the bottom)
    painter.line_segment(
        [inner.left_bottom(), inner.right_bottom()],
        Stroke::new(0.8, Color32::from_gray(0)),
    );
    // Subtle side shading
    painter.line_segment(
        [inner.left_top(), inner.left_bottom()],
        Stroke::new(0.5, Color32::from_gray(18)),
    );
    painter.line_segment(
        [inner.right_top(), inner.right_bottom()],
        Stroke::new(0.5, Color32::from_gray(18)),
    );
}

/// Convenience wrapper for the deep-`VOID` screen background used by
/// oscilloscopes and the event stream.
pub fn draw_screen_bezel(painter: &egui::Painter, rect: Rect, rounding: Rounding) {
    draw_screen_panel(painter, rect, rounding, VOID);
}

/// Default rounding used by `screen_chip`.
pub const SCREEN_CHIP_ROUNDING: f32 = 3.0;

// ─── Header grid ─────────────────────────────────────────────────────────────
//
// Both header panels (top transport bar + lower log/scope band) share a
// single virtual grid of `HEADER_TOTAL_COLS` columns so the per-section
// padding and widths line up across the two strips.
//
// Cell width scales with the window so the layout always fits exactly
// horizontally.  Cell height is locked to `HEADER_CELL_H_MIN` so 2-row
// chips stay readable on narrow displays.

/// Virtual cell columns spanning the full header width.
pub const HEADER_TOTAL_COLS: u32 = 105;
/// Cells tall the top transport strip occupies.
pub const HEADER_TOP_ROWS: u32 = 2;
/// Cells tall the lower (log + visualizers) strip occupies by default.
pub const HEADER_LOWER_ROWS: u32 = 6;
/// Minimum cell height in pixels — protects text legibility on narrow displays.
pub const HEADER_CELL_H_MIN: f32 = 20.0;
/// Minimum cell width in pixels.
pub const HEADER_CELL_W_MIN: f32 = 10.0;
/// Inset (px) the chip content gets relative to its outer cell rect.
pub const HEADER_CHIP_INSET: f32 = 4.0;

/// Compute the per-cell pixel size for the given total available pixel width.
pub fn header_cell_size(available_w: f32) -> (f32, f32) {
    let cell_w = (available_w / HEADER_TOTAL_COLS as f32).max(HEADER_CELL_W_MIN);
    let cell_h = cell_w.max(HEADER_CELL_H_MIN);
    (cell_w, cell_h)
}

/// Translate a (col, row, span_cols, span_rows) cell tuple to a pixel rect
/// anchored at `origin`.
pub fn header_cell_rect(
    origin: Pos2,
    cell_w: f32,
    cell_h: f32,
    col: u32,
    row: u32,
    span_cols: u32,
    span_rows: u32,
) -> Rect {
    Rect::from_min_size(
        Pos2::new(
            origin.x + col as f32 * cell_w,
            origin.y + row as f32 * cell_h,
        ),
        Vec2::new(span_cols as f32 * cell_w, span_rows as f32 * cell_h),
    )
}

/// Same screen-chip look as [`screen_chip`] but placed at an explicit
/// `outer_rect` (used by the header grid).  Insets the content by
/// [`HEADER_CHIP_INSET`] so widgets don't touch the bezel.
pub fn screen_chip_at(
    ui: &mut egui::Ui,
    outer_rect: Rect,
    fill: Color32,
    content: impl FnOnce(&mut egui::Ui),
) {
    let rounding = Rounding::same(SCREEN_CHIP_ROUNDING);
    draw_screen_panel(ui.painter(), outer_rect, rounding, fill);
    let inner = outer_rect.shrink(HEADER_CHIP_INSET);
    let mut child = ui.child_ui(inner, *ui.layout(), None);
    content(&mut child);
}

/// Wrap `content` in a recessed-screen "chip" — a small framed area used
/// to group header controls so they read as separate displays instead of
/// raw widgets pinned together by `ui.separator()` lines.  `fill` controls
/// the inner background colour (use `VOID` for matching the oscilloscopes
/// or `DEEP` for a slightly lighter panel-style surface).  Returns the
/// outer Frame's [`egui::Response`].
pub fn screen_chip(
    ui: &mut egui::Ui,
    fill: Color32,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let rounding = Rounding::same(SCREEN_CHIP_ROUNDING);
    let resp = egui::Frame::none()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(6.0, 3.0))
        .rounding(rounding)
        .show(ui, content);
    // Post-paint just the bezel highlights / shadow / outer frame on top of
    // the contents — perimeter pixels only, so widget area is unaffected.
    let rect = resp.response.rect;
    let painter = ui.painter();
    painter.rect_stroke(rect, rounding, Stroke::new(1.0, Color32::from_gray(2)));
    let inner = rect.shrink(1.0);
    painter.line_segment(
        [inner.left_top(), inner.right_top()],
        Stroke::new(0.8, Color32::from_gray(38)),
    );
    painter.line_segment(
        [inner.left_bottom(), inner.right_bottom()],
        Stroke::new(0.8, Color32::from_gray(0)),
    );
    painter.line_segment(
        [inner.left_top(), inner.left_bottom()],
        Stroke::new(0.5, Color32::from_gray(18)),
    );
    painter.line_segment(
        [inner.right_top(), inner.right_bottom()],
        Stroke::new(0.5, Color32::from_gray(18)),
    );
    resp.response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_temperature_poles() {
        // C (Blue) is the cold pole, F (Orange) is the warm pole.
        assert!((note_temperature(60) - (-1.0)).abs() < 1e-6); // C4
        assert!((note_temperature(65) - 1.00).abs() < 1e-6); // F4
    }

    #[test]
    fn note_temperature_wraps_octaves() {
        for octave in 0..10 {
            let n = (octave * 12) as u8;
            assert!((note_temperature(n) - (-1.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn note_temperature_in_range() {
        for n in 0..128u8 {
            let t = note_temperature(n);
            assert!((-1.0..=1.0).contains(&t), "n={n} t={t}");
        }
    }

    #[test]
    fn note_temperature_warm_cold_grouping() {
        // Huth: Warm = E,F,F#,G; Cold = C,C#,D; bridge contains G#,A,A#,B.
        for &warm in &[64u8, 65, 66, 67] {
            assert!(note_temperature(warm) > 0.0, "{warm} should read warm");
        }
        for &cold in &[60u8, 61, 62] {
            assert!(note_temperature(cold) < 0.0, "{cold} should read cold");
        }
    }

    #[test]
    fn temperature_color_extremes_match_palette() {
        // Pure cold = C-blue, pure warm = F-orange.
        assert_eq!(temperature_color(-1.0), NOTE_COLORS[0]);
        assert_eq!(temperature_color(1.0), NOTE_COLORS[5]);
    }
}
