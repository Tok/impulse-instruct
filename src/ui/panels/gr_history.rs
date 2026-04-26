// ─── ui/panels/gr_history.rs ─────────────────────────────────────────────────
// GrHistory — rolling gain-reduction trace for the dynamics FX
// (FxCompressor / FxLimiter / FxMultibandComp).  Reads the audio
// thread's atomic GR snapshot once per repaint, appends to a
// scrolling ring buffer, paints a downward-falling envelope.
//
// Pure UI: no DSP, no audio path.  The audio thread updates
// `app.gr_levels` inside `apply_fx_chain` and publishes once per
// callback; this panel samples at egui's repaint rate (~60 Hz),
// which is plenty for "how hard is the chain compressing right now".

use crate::audio::gr_levels::linear_to_gr_db;
use crate::ui::{ImpulseApp, theme};

/// Scrolling history capacity — at the typical ~60 Hz repaint rate
/// this is ~8 sec of GR.  Sized to the wide-strip 4×2 grid card.
const GR_HISTORY_CAPACITY: usize = 480;

/// Floor of the y-axis in dB.  GR readings below this clamp; matches
/// `linear_to_gr_db`'s -60 dB floor.
const GR_DB_FLOOR: f32 = -24.0;

pub fn draw_gr_history(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Sample the latest GR ratio and append.  Drop the oldest sample
    // if we'd exceed capacity so the deque stays bounded.
    let latest_linear = app.gr_levels.read();
    if app.gr_history.len() >= GR_HISTORY_CAPACITY {
        app.gr_history.pop_front();
    }
    app.gr_history.push_back(latest_linear);

    // Header row — current GR readout + the dB scale.
    let latest_db = linear_to_gr_db(latest_linear);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("GAIN REDUCTION")
                .color(theme::SMOKE)
                .monospace()
                .size(8.0),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!("{latest_db:>5.1} dB"))
                .color(if latest_db < -1.0 {
                    theme::CHALK
                } else {
                    theme::FOG
                })
                .monospace()
                .size(8.5),
        );
    });

    let avail_w = ui.available_width();
    let avail_h = ui.available_height().max(40.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, avail_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);

    // Background well + scale lines at -3 / -6 / -12 dB so the eye
    // has reference rails (matching standard mastering meter conventions).
    painter.rect_filled(
        rect,
        egui::Rounding::same(2.0),
        egui::Color32::from_gray(10),
    );
    let h = rect.height();
    for &db in &[-3.0_f32, -6.0, -12.0, -18.0] {
        if db < GR_DB_FLOOR {
            continue;
        }
        let y_norm = db / GR_DB_FLOOR;
        let y = rect.top() + y_norm * h;
        painter.line_segment(
            [
                egui::Pos2::new(rect.left(), y),
                egui::Pos2::new(rect.right(), y),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
        );
        painter.text(
            egui::Pos2::new(rect.left() + 2.0, y - 1.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{db:>3.0}"),
            egui::FontId::monospace(7.0),
            egui::Color32::from_gray(50),
        );
    }

    // Trace.  The history is the linear ratio per UI tick (most-recent
    // last); convert to dB and map onto the rect.  GR = 0 dB sits at
    // the top, GR_DB_FLOOR at the bottom — the trace falls downward
    // when the chain is compressing harder.
    let n = app.gr_history.len();
    if n < 2 {
        return;
    }
    let step_x = rect.width() / (n - 1) as f32;
    for i in 1..n {
        let prev = linear_to_gr_db(app.gr_history[i - 1]);
        let curr = linear_to_gr_db(app.gr_history[i]);
        let prev_y = rect.top() + (prev / GR_DB_FLOOR).clamp(0.0, 1.0) * h;
        let curr_y = rect.top() + (curr / GR_DB_FLOOR).clamp(0.0, 1.0) * h;
        let prev_x = rect.left() + (i - 1) as f32 * step_x;
        let curr_x = rect.left() + i as f32 * step_x;
        // Brighter when more attenuation — the eye's drawn to active GR.
        let g = (80.0 + (-curr.min(0.0) / -GR_DB_FLOOR) * 160.0).min(240.0) as u8;
        painter.line_segment(
            [
                egui::Pos2::new(prev_x, prev_y),
                egui::Pos2::new(curr_x, curr_y),
            ],
            egui::Stroke::new(1.2, egui::Color32::from_gray(g)),
        );
    }
}
