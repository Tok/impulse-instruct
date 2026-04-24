// ─── ui/widgets/param_eq_curve.rs ────────────────────────────────────────────
// Draggable curve editor for the 8-band parametric EQ.
//
// X axis: log frequency 20 Hz → 20 kHz.  Y axis: linear gain dB,
// ±CURVE_DB_RANGE.  For each band a draggable handle lives at
// (freq, gain) with these interactions:
//
//   • primary drag on a handle   → moves the band's freq + gain
//   • scroll wheel over a handle → adjusts Q (up = narrower, down = wider)
//   • secondary (right) click    → cycles band kind (LowShelf → Peak → HighShelf)
//   • double-click               → toggles `enabled`
//
// The composite freq response is sampled at CURVE_RESOLUTION log-
// spaced frequencies and rendered as a polyline; grid lines mark
// octaves on the x-axis and ±6 dB steps on the y-axis so the user
// has a reference for how far they're pushing a band.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use super::super::theme;
use crate::audio::dsp::param_eq::cascade_db;
use crate::state::{ParamEqBand, ParamEqBandKind};

const MIN_FREQ_HZ: f32 = 20.0;
const MAX_FREQ_HZ: f32 = 20_000.0;
const CURVE_DB_RANGE: f32 = 18.0;
const CURVE_RESOLUTION: usize = 192;
const HANDLE_RADIUS: f32 = 4.5;
const HANDLE_HIT_RADIUS: f32 = 8.0;
const Q_SCROLL_STEP: f32 = 0.12;

/// Curve editor widget.  Returns `true` when any band was edited this
/// frame so the caller can push the updated state to the audio thread.
pub fn param_eq_curve(
    ui: &mut Ui,
    id_source: &str,
    bands: &mut [ParamEqBand; 8],
    sr: f32,
    size: Vec2,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);

    // ── Backdrop + grid ──────────────────────────────────────────────────
    painter.rect_filled(rect, 2.0, theme::VOID);
    draw_grid(&painter, rect);

    // ── Composite frequency response polyline ────────────────────────────
    let mut curve_points: Vec<Pos2> = Vec::with_capacity(CURVE_RESOLUTION);
    for i in 0..CURVE_RESOLUTION {
        let t = i as f32 / (CURVE_RESOLUTION - 1) as f32;
        let freq = freq_at_t(t);
        let db = cascade_db(bands, sr, freq);
        let x = rect.left() + t * rect.width();
        let y = db_to_y(db, rect);
        curve_points.push(Pos2::new(x, y));
    }
    painter.add(egui::Shape::line(
        curve_points,
        Stroke::new(1.5, theme::ASH),
    ));

    // ── Per-band draggable handles ───────────────────────────────────────
    let mut changed = false;
    let pointer = ui.ctx().pointer_latest_pos();
    let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
    let secondary_clicked = ui.ctx().input(|i| i.pointer.secondary_clicked());
    let scroll_delta = ui.ctx().input(|i| i.smooth_scroll_delta.y);

    for (i, band) in bands.iter_mut().enumerate() {
        let handle_pos = band_handle_pos(band, rect);
        let hit_id = ui.id().with(id_source).with(("param_eq_handle", i));
        // Allocate an interact rect around the handle for click/drag.
        let hit_rect = Rect::from_center_size(handle_pos, Vec2::splat(HANDLE_HIT_RADIUS * 2.0));
        let handle_resp = ui.interact(hit_rect, hit_id, Sense::click_and_drag());

        // Primary drag — move the handle.  Pointer-position based so
        // the handle tracks the cursor (no drag-delta drift over many
        // frames).  Clamp to the allowed freq/gain range.
        if handle_resp.dragged()
            && primary_down
            && let Some(p) = pointer
        {
            let new_freq = t_to_freq(((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
            let new_db = y_to_db(p.y, rect).clamp(-CURVE_DB_RANGE, CURVE_DB_RANGE);
            if (band.freq_hz - new_freq).abs() > 1e-3 || (band.gain_db - new_db).abs() > 1e-3 {
                band.freq_hz = new_freq;
                band.gain_db = new_db;
                changed = true;
            }
        }

        // Scroll over the handle → adjust Q.  Hovered-only so scrolling
        // elsewhere on the curve doesn't accidentally change Q.
        if handle_resp.hovered() && scroll_delta.abs() > 0.01 {
            let factor = (1.0 + scroll_delta.signum() * Q_SCROLL_STEP).max(0.1);
            let new_q = (band.q * factor).clamp(0.1, 10.0);
            if (band.q - new_q).abs() > 1e-4 {
                band.q = new_q;
                changed = true;
            }
        }

        // Secondary click → cycle kind.  The hit rect catches the
        // right-click even though egui's `secondary_clicked()` is a
        // global input flag (guard with hover to keep it scoped).
        if handle_resp.hovered() && secondary_clicked {
            band.kind = match band.kind {
                ParamEqBandKind::LowShelf => ParamEqBandKind::Peak,
                ParamEqBandKind::Peak => ParamEqBandKind::HighShelf,
                ParamEqBandKind::HighShelf => ParamEqBandKind::LowShelf,
            };
            changed = true;
        }

        // Double-click → toggle enabled.
        if handle_resp.double_clicked() {
            band.enabled = !band.enabled;
            changed = true;
        }

        // ── Draw the handle ──────────────────────────────────────────
        let hovered = handle_resp.hovered();
        let col = handle_colour(band, hovered);
        if band.enabled {
            painter.circle_filled(handle_pos, HANDLE_RADIUS, col);
        } else {
            painter.circle_stroke(handle_pos, HANDLE_RADIUS, Stroke::new(1.5, col));
        }
        // Band number label on top of each handle (tiny).
        painter.text(
            handle_pos + Vec2::new(0.0, -HANDLE_RADIUS - 7.0),
            egui::Align2::CENTER_CENTER,
            (i + 1).to_string(),
            egui::FontId::monospace(8.0),
            theme::PIT,
        );
    }

    let _ = response;
    changed
}

// ─── Coordinate mappings ─────────────────────────────────────────────────────

/// Map a curve parameter t ∈ [0, 1] (left → right on the widget) to a
/// log-spaced frequency in [MIN_FREQ_HZ, MAX_FREQ_HZ].
fn freq_at_t(t: f32) -> f32 {
    let lg_min = MIN_FREQ_HZ.log10();
    let lg_max = MAX_FREQ_HZ.log10();
    10.0_f32.powf(lg_min + t * (lg_max - lg_min))
}

/// Inverse of `freq_at_t` — turn a frequency into the t value that
/// `freq_at_t` would have produced it from.
fn t_at_freq(freq_hz: f32) -> f32 {
    let lg_min = MIN_FREQ_HZ.log10();
    let lg_max = MAX_FREQ_HZ.log10();
    ((freq_hz.log10() - lg_min) / (lg_max - lg_min)).clamp(0.0, 1.0)
}

/// Convert a pointer x-coordinate relative to the plot rect into Hz.
fn t_to_freq(t: f32) -> f32 {
    freq_at_t(t.clamp(0.0, 1.0))
}

fn db_to_y(db: f32, rect: Rect) -> f32 {
    let mid = rect.center().y;
    let half = rect.height() * 0.5;
    mid - (db / CURVE_DB_RANGE) * half
}

fn y_to_db(y: f32, rect: Rect) -> f32 {
    let mid = rect.center().y;
    let half = rect.height() * 0.5;
    -((y - mid) / half) * CURVE_DB_RANGE
}

fn band_handle_pos(band: &ParamEqBand, rect: Rect) -> Pos2 {
    let t = t_at_freq(band.freq_hz);
    let x = rect.left() + t * rect.width();
    let y = db_to_y(band.gain_db, rect);
    Pos2::new(x, y)
}

fn handle_colour(band: &ParamEqBand, hovered: bool) -> Color32 {
    // Hovered handles brighten; disabled bands stay dim regardless.
    let base_gray = if hovered { 220 } else { 175 };
    let gray = if band.enabled { base_gray } else { 90 };
    Color32::from_gray(gray)
}

// ─── Grid painter ────────────────────────────────────────────────────────────

fn draw_grid(painter: &egui::Painter, rect: Rect) {
    let line_col = Color32::from_gray(40);
    let centre_col = Color32::from_gray(60);
    let stroke = Stroke::new(0.5, line_col);
    // Vertical grid: one line per octave from 31 Hz to 16 kHz.
    let octaves = [
        31.25, 62.5, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
    ];
    for &f in &octaves {
        let x = rect.left() + t_at_freq(f) * rect.width();
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
    }
    // Horizontal grid: ±6 dB, ±12 dB plus the 0 dB centre line at higher
    // contrast so it's obvious where unity sits.
    for &db in &[-12.0_f32, -6.0, 6.0, 12.0] {
        let y = db_to_y(db, rect);
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
    }
    let mid_y = db_to_y(0.0, rect);
    painter.line_segment(
        [
            Pos2::new(rect.left(), mid_y),
            Pos2::new(rect.right(), mid_y),
        ],
        Stroke::new(0.8, centre_col),
    );
}
