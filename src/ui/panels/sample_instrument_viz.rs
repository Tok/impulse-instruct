// ─── ui/panels/sample_instrument_viz.rs ─────────────────────────────────────
// V2 Stage 7: visualization strip rendered at the bottom of the
// SampleInstrument panel.
//
// Two modes, picked by what's loaded into the voice:
//   * Single-WAV: min/max waveform thumbnail (cached on path) +
//     loop-window shading.
//   * SFZ multisample: piano-keyboard zone map — each region shaded
//     across its `lokey..=hikey` range.  Region's `pitch_keycenter`
//     gets a vertical tick so the user can see at a glance which key
//     each sample is anchored at.
//
// Both helpers stay paint-only (no drag/click handling yet — Stage
// 7.5 adds drag markers + per-zone selection).

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::audio::dsp::sample_instrument::SfzRegionRuntime;
use crate::ui::theme;

/// Poly-meter dot count — must match `SampleInstrumentVoice::POLY_VOICES`.
/// Hard-coded here (not pulled from the test-only constant) so the UI
/// crate doesn't widen its public surface for a paint-only number.
/// Kept in sync via `poly_dots_matches_voice_pool` in the test module.
pub(crate) const POLY_DOTS: u8 = 8;

/// Build (min, max) thumbnail pairs for a sample buffer.  Bins the
/// buffer into `cols` columns; each column's pair is the min/max of
/// the samples in that bin.  Cheap to call once on load and stash the
/// result in `ImpulseApp.sample_wave_cache`.
pub(crate) fn build_thumbnail(samples: &[f32], cols: usize) -> Vec<(f32, f32)> {
    if samples.is_empty() || cols == 0 {
        return Vec::new();
    }
    let bin = samples.len() / cols.max(1);
    let bin = bin.max(1);
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(cols);
    for c in 0..cols {
        let lo = c * bin;
        let hi = ((c + 1) * bin).min(samples.len());
        if lo >= hi {
            out.push((0.0, 0.0));
            continue;
        }
        let mut mn = f32::INFINITY;
        let mut mx = f32::NEG_INFINITY;
        for &s in &samples[lo..hi] {
            if s < mn {
                mn = s;
            }
            if s > mx {
                mx = s;
            }
        }
        out.push((mn, mx));
    }
    out
}

/// Paint a horizontal row of `POLY_DOTS` dots — bright for active
/// slots (left-aligned: dot 0..active_count are lit), dim for free.
/// Surfaces the SampleInstrument poly pool occupancy so the user can
/// see voice-stealing pressure before they hit it.  Compact (fits
/// next to the LOAD/filename row); read-only by design.
pub(crate) fn draw_poly_meter(ui: &mut Ui, active: u8) {
    let n = POLY_DOTS as f32;
    let dot_r = 2.5_f32;
    let gap = 3.0_f32;
    let width = n * dot_r * 2.0 + (n - 1.0) * gap + 4.0;
    let height = dot_r * 2.0 + 2.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let active = active.min(POLY_DOTS);
    for i in 0..POLY_DOTS {
        let cx = rect.min.x + 2.0 + dot_r + i as f32 * (dot_r * 2.0 + gap);
        let cy = rect.center().y;
        let lit = i < active;
        // Lit dots use FOG (primary text); idle dots use IRON
        // (inactive widget) — both grayscale, palette-compliant.
        let fill = if lit { theme::FOG } else { theme::IRON };
        painter.circle_filled(Pos2::new(cx, cy), dot_r, fill);
    }
    resp.on_hover_text(format!("Polyphony: {} / {}", active, POLY_DOTS));
}

/// Paint a waveform thumbnail into `rect`.  `loop_start_frac` and
/// `loop_end_frac` are 0..1 positions in the buffer; when
/// `loop_enabled` is true the section between them is highlighted
/// (rest gets shaded).  Bezel + grid styling matches the rest of the
/// engine's recessed-screen aesthetic.
pub(crate) fn draw_waveform(
    ui: &mut Ui,
    thumb: &[(f32, f32)],
    loop_start_frac: f32,
    loop_end_frac: f32,
    loop_enabled: bool,
    width: f32,
    height: f32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));
    if thumb.is_empty() {
        return;
    }
    let mid = rect.center().y;
    let half_h = rect.height() * 0.45;
    let cols = thumb.len();
    let col_w = rect.width() / cols as f32;
    for (i, (mn, mx)) in thumb.iter().enumerate() {
        let x = rect.min.x + i as f32 * col_w + col_w * 0.5;
        let y_top = mid - mx * half_h;
        let y_bot = mid - mn * half_h;
        painter.line_segment(
            [Pos2::new(x, y_top), Pos2::new(x, y_bot)],
            Stroke::new(col_w.max(1.0), Color32::from_gray(160)),
        );
    }
    // Shade outside the loop window when looping is on.
    if loop_enabled && loop_end_frac > loop_start_frac {
        let shade = Color32::from_rgba_unmultiplied(8, 8, 8, 160);
        let ls_x = rect.min.x + loop_start_frac.clamp(0.0, 1.0) * rect.width();
        let le_x = rect.min.x + loop_end_frac.clamp(0.0, 1.0) * rect.width();
        if ls_x > rect.min.x {
            painter.rect_filled(
                Rect::from_min_max(rect.min, Pos2::new(ls_x, rect.max.y)),
                egui::Rounding::ZERO,
                shade,
            );
        }
        if le_x < rect.max.x {
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(le_x, rect.min.y), rect.max),
                egui::Rounding::ZERO,
                shade,
            );
        }
        // Loop boundary stems — bright lines so the shading reads as
        // "this bit loops" rather than "this bit is muted".
        let stem_color = Color32::from_gray(140);
        painter.line_segment(
            [Pos2::new(ls_x, rect.min.y), Pos2::new(ls_x, rect.max.y)],
            Stroke::new(1.0, stem_color),
        );
        painter.line_segment(
            [Pos2::new(le_x, rect.min.y), Pos2::new(le_x, rect.max.y)],
            Stroke::new(1.0, stem_color),
        );
    }
}

/// Paint an SFZ zone-map: horizontal piano-keyboard strip with each
/// region shaded across its key range.  Multiple overlapping regions
/// are rendered with darker shading so dense clusters read as deeper
/// gray.  `pitch_keycenter` per region gets a tiny vertical tick.
pub(crate) fn draw_zone_map(ui: &mut Ui, regions: &[SfzRegionRuntime], width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));

    // Show MIDI 21..=108 (piano range) so the strip uses the visible
    // area for the notes a user actually plays.  Out-of-range regions
    // still get drawn — clipped to the visible window.
    const KEY_LO: u8 = 21;
    const KEY_HI: u8 = 108;
    let span = (KEY_HI - KEY_LO) as f32;
    let to_x = |note: u8| -> f32 {
        let n = note.clamp(KEY_LO, KEY_HI) as f32 - KEY_LO as f32;
        rect.min.x + (n / span) * rect.width()
    };

    // C-major reference grid — every C gets a faint vertical line so
    // the user can identify octaves at a glance.
    for c in (KEY_LO..=KEY_HI).filter(|n| n.is_multiple_of(12)) {
        let x = to_x(c);
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(0.5, Color32::from_gray(30)),
        );
        // Only label C4 to keep the strip uncluttered.
        if c == 60 {
            painter.text(
                Pos2::new(x + 2.0, rect.min.y + 1.0),
                egui::Align2::LEFT_TOP,
                "C4",
                egui::FontId::monospace(6.0),
                Color32::from_gray(60),
            );
        }
    }

    // Each region paints a translucent rectangle across its key range.
    // Stacking blends darker, so overlapping zones read clearly.  Y
    // banding spreads regions vertically in their declared order so a
    // visually stacked pack (close + room mics) shows as parallel
    // strips, not one merged block.
    if regions.is_empty() {
        return;
    }
    let band_h = (rect.height() - 4.0) / regions.len().max(1) as f32;
    for (i, r) in regions.iter().enumerate() {
        let y_top = rect.min.y + 2.0 + i as f32 * band_h;
        let y_bot = (y_top + band_h).min(rect.max.y - 2.0);
        let x_lo = to_x(r.region.lokey);
        let x_hi = to_x(r.region.hikey);
        let band = Rect::from_min_max(Pos2::new(x_lo, y_top), Pos2::new(x_hi, y_bot));
        // Alternate two shades by row index so adjacent regions stay
        // visually distinct without needing colour.
        let shade = if i.is_multiple_of(2) {
            Color32::from_gray(110)
        } else {
            Color32::from_gray(150)
        };
        painter.rect_filled(band, egui::Rounding::same(1.0), shade);
        // Tick at pitch_keycenter — bright pip so the user sees where
        // each region's "natural" note sits.
        let kc_x = to_x(r.region.pitch_keycenter);
        painter.line_segment(
            [Pos2::new(kc_x, y_top), Pos2::new(kc_x, y_bot)],
            Stroke::new(1.0, Color32::from_gray(220)),
        );
    }
}
