// ─── ui/widgets/temperature.rs ───────────────────────────────────────────────
// Huth warm/cold (Warme/Kalte Töne) temperature chip.
//
// Two signals share one strip:
//   • Live needle — master-output spectrum timbre (Fourier-weighted note
//     average).
//   • Bank tick — melody-pattern intent (notes × gate × accent).
//
// Previously drawn inside the event-stream header; now lives in a dedicated
// header chip so the live value is always visible regardless of the lower
// panel's size.

use crate::state::AppState;
use crate::state::sequencer_state::pattern_temperature_acc;
use crate::ui::theme;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Sum the Huth warm/cold value across all enabled melodic voices in `state`,
/// weighted by gate × accent.  Returns NaN if no active note is found.
pub fn bank_temperature(state: &AppState) -> f32 {
    let seq = &state.sequencer;
    let mut sum = 0.0_f32;
    let mut wsum = 0.0_f32;
    for (vi, voice) in state.bass_voices.iter().enumerate() {
        if !voice.enabled {
            continue;
        }
        let pattern = if vi == 0 {
            &seq.bass_pattern
        } else if let Some(p) = seq.bass_patterns.get(vi) {
            p
        } else {
            continue;
        };
        let len = seq
            .bass_voice_steps
            .get(vi)
            .copied()
            .unwrap_or(seq.steps)
            .min(pattern.len());
        let (s, w) = pattern_temperature_acc(pattern, len, &theme::NOTE_TEMP);
        sum += s;
        wsum += w;
    }
    if state.an1x.enabled {
        let len = seq.an1x_steps.min(seq.an1x_pattern.len());
        let (s, w) = pattern_temperature_acc(&seq.an1x_pattern, len, &theme::NOTE_TEMP);
        sum += s;
        wsum += w;
    }
    if wsum > 0.0 {
        (sum / wsum).clamp(-1.0, 1.0)
    } else {
        f32::NAN
    }
}

/// Draw the temperature chip into the caller's available UI rect.
///
/// Layout: gradient strip on the left, numeric value on the right.  Live
/// needle and bank-intent tick both render when finite.
pub fn draw_temp_chip(ui: &mut Ui, live: f32, bank: f32) {
    let inner = ui.available_rect_before_wrap();
    let painter = ui.painter_at(inner);

    // Reserve ~40 px on the right for the numeric label.
    let label_w = 34.0_f32.min(inner.width() * 0.35);
    let strip_rect = Rect::from_min_max(
        Pos2::new(inner.min.x + 2.0, inner.center().y - 3.0),
        Pos2::new(inner.max.x - label_w - 2.0, inner.center().y + 3.0),
    );
    if strip_rect.width() < 10.0 {
        return;
    }

    // Gradient bar cold → neutral → warm.
    let band_count = 14;
    let strip_w = strip_rect.width();
    for i in 0..band_count {
        let t = -1.0 + 2.0 * (i as f32 / (band_count - 1) as f32);
        let band_x = strip_rect.min.x + (i as f32 / band_count as f32) * strip_w;
        let band_w = (strip_w / band_count as f32) + 0.5;
        let mut col = theme::temperature_color(t);
        col = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 190);
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(band_x, strip_rect.min.y),
                Vec2::new(band_w, strip_rect.height()),
            ),
            egui::Rounding::ZERO,
            col,
        );
    }
    // Frame.
    painter.rect_stroke(
        strip_rect,
        egui::Rounding::ZERO,
        Stroke::new(0.5, Color32::from_gray(60)),
    );

    // Bank intent — static tick below the strip.
    if bank.is_finite() {
        let bx = strip_rect.min.x + (bank.clamp(-1.0, 1.0) * 0.5 + 0.5) * strip_w;
        let by = strip_rect.max.y + 0.5;
        let tri = vec![
            Pos2::new(bx, by),
            Pos2::new(bx - 2.5, by + 3.0),
            Pos2::new(bx + 2.5, by + 3.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            tri,
            Color32::from_gray(180),
            Stroke::NONE,
        ));
    }
    // Live needle — vertical line.
    if live.is_finite() {
        let n_t = live.clamp(-1.0, 1.0);
        let nx = strip_rect.min.x + (n_t * 0.5 + 0.5) * strip_w;
        painter.line_segment(
            [
                Pos2::new(nx, strip_rect.min.y - 1.5),
                Pos2::new(nx, strip_rect.max.y + 1.5),
            ],
            Stroke::new(1.2, Color32::from_gray(235)),
        );
    }
    // Numeric value — prefer live, fall back to bank.
    let (val, val_col) = if live.is_finite() {
        let t = live.clamp(-1.0, 1.0);
        (t, theme::temperature_color(t))
    } else if bank.is_finite() {
        let t = bank.clamp(-1.0, 1.0);
        (t, theme::temperature_color(t))
    } else {
        (f32::NAN, Color32::from_gray(100))
    };
    let txt = if val.is_finite() {
        format!("{:+.2}", val)
    } else {
        "—".to_string()
    };
    painter.text(
        Pos2::new(inner.max.x - 3.0, inner.center().y),
        egui::Align2::RIGHT_CENTER,
        txt,
        egui::FontId::monospace(10.0),
        val_col,
    );

    // Hover tooltip.
    let hit = ui.interact(inner, egui::Id::new("header_temp_chip"), Sense::hover());
    if hit.hovered() {
        hit.on_hover_ui(|ui| {
            ui.label("Huth Warme / Kalte Töne");
            ui.separator();
            ui.label("Needle: live master-out timbre");
            ui.label("▲ tick: melody-bank intent (pattern notes)");
            let live_str = if live.is_finite() {
                format!("{:+.2}", live)
            } else {
                "—".to_string()
            };
            let bank_str = if bank.is_finite() {
                format!("{:+.2}", bank)
            } else {
                "—".to_string()
            };
            ui.small(format!("live {live_str}   bank {bank_str}"));
        });
    }
}
