// ─── ui/panels/sequencer_automation.rs ───────────────────────────────────────
// Automation lane overlay paint pass — extracted from sequencer.rs to
// keep that file under the 1000-line cap.  Reads
// `UiPrefs.show_automation_overlay` and a bass voice's per-voice LFO
// state, then paints a slim sparkline aligned to the visible step grid.
// The sparkline value math lives in
// `crate::state::bass_lfo_curve_for_view` (pure, unit-tested).

use super::sequencer::{STEPS_PER_PAGE, STEPS_PER_ROW, fixed_label, fixed_space};
use crate::ui::{ImpulseApp, SEQ_LABEL_W, SEQ_VOL_W, theme};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_bass_voice_overlay(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    vi: usize,
    page_start: usize,
    seq_steps: usize,
    sub_rows: usize,
    pad_px: f32,
    marker_h: f32,
    row_spacer: f32,
) {
    if !app.state.read().ui_prefs.show_automation_overlay {
        return;
    }
    let (curve, lfo_label) = {
        let s = app.state.read();
        let synth = &s.bass_voices[vi.min(s.bass_voices.len() - 1)].synth;
        let curve = crate::state::bass_lfo_curve_for_view(
            synth,
            s.sequencer.bpm,
            s.sequencer.step_division,
            page_start,
            STEPS_PER_PAGE.min(seq_steps),
        );
        (curve, synth.lfo_target.label())
    };
    if curve.is_empty() || curve.iter().all(|v| v.abs() <= 1e-4) {
        return;
    }
    for sub in 0..sub_rows {
        ui.horizontal(|ui| {
            fixed_space(ui, 10.0);
            fixed_space(ui, 10.0);
            if sub == 0 {
                fixed_label(
                    ui,
                    SEQ_LABEL_W - 20.0,
                    marker_h,
                    lfo_label,
                    theme::IRON,
                    7.0,
                );
                fixed_space(ui, SEQ_VOL_W);
            } else {
                fixed_space(ui, SEQ_LABEL_W - 20.0);
                fixed_space(ui, SEQ_VOL_W);
            }
            fixed_space(ui, 18.0);
            if row_spacer > 0.0 {
                ui.add_space(row_spacer);
            }
            let base = sub * STEPS_PER_ROW;
            let row_w = (STEPS_PER_ROW as f32) * pad_px + (STEPS_PER_ROW as f32 - 1.0) * 2.0;
            let h = (marker_h * 0.6).max(8.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(row_w, h), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            let mid_y = rect.center().y;
            painter.line_segment(
                [egui::pos2(rect.min.x, mid_y), egui::pos2(rect.max.x, mid_y)],
                egui::Stroke::new(0.5, theme::PIT),
            );
            let n = STEPS_PER_ROW.min(curve.len().saturating_sub(base));
            if n >= 2 {
                let mut pts = Vec::with_capacity(n);
                for j in 0..n {
                    let v = curve[base + j].clamp(-1.0, 1.0);
                    let x = rect.min.x + (j as f32 + 0.5) * (rect.width() / n as f32);
                    let y = mid_y - v * (h * 0.45);
                    pts.push(egui::pos2(x, y));
                }
                painter.add(egui::Shape::line(pts, egui::Stroke::new(1.2, theme::SMOKE)));
            }
        });
    }
}
