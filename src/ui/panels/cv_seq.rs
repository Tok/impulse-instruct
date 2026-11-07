// ─── ui/panels/cv_seq.rs ──────────────────────────────────────────────────────
// CV sequencer module panel — 16 vertical step bars + a target
// cycle button + depth knob + enabled toggle.  Each bar is
// draggable (click + vertical drag) to set its 0..1 value; the
// audio thread reads `step_values[current_step % 16]` per block
// and applies `(value - 0.5) * 2.0 * depth` to the chosen
// `LfoTarget` opcode.
//
// Multiple `CvSequencer` rack instances share the four CV-seq
// slots in `AppState.cv_seq[]` — each instance's slot index is
// derived from its order in the rack (same idiom as `LfoModule`).
//
// Pure UI: state writes go through a brief write-lock of
// `app.state` for the click-to-edit path.

use crate::state::ParamMode;
use crate::ui::panels::lfo::{next_target, target_label};
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

const STEP_BAR_W: f32 = 14.0;
const STEP_BAR_H: f32 = 56.0;
const STEP_BAR_GAP: f32 = 2.0;

pub fn draw_cv_seq(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::CV_SEQ_SLOTS - 1);
    // Snapshot the slot under read.
    let snapshot = {
        let s = app.state.read();
        s.cv_seq[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut depth = snapshot.depth;
    let mut target = snapshot.target;
    let mut step_values = snapshot.step_values;
    let mut changed = false;

    // ── Header row: ON/OFF + TARGET cycle button + DEPTH ──
    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("->")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let lbl = target_label(&target);
        if ui
            .button(
                egui::RichText::new(lbl)
                    .color(if matches!(target, crate::state::LfoTarget::None) {
                        theme::IRON
                    } else {
                        theme::CHALK
                    })
                    .monospace()
                    .size(9.5),
            )
            .clicked()
        {
            target = next_target(&target);
            changed = true;
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("DEPTH")
                .monospace()
                .size(7.5)
                .color(theme::IRON),
        );
        if ui
            .add(
                egui::DragValue::new(&mut depth)
                    .range(0.0..=1.0)
                    .speed(0.005)
                    .fixed_decimals(2),
            )
            .changed()
        {
            changed = true;
        }
    });
    let _ = ParamMode::UserOwned; // (kept for future migration to param_control)

    // ── 16 step bars ───────────────────────────────────────
    ui.add_space(4.0);
    let avail_w = ui.available_width();
    let preferred_w = STEP_BAR_W * 16.0 + STEP_BAR_GAP * 15.0;
    let bar_w = if avail_w < preferred_w {
        ((avail_w - STEP_BAR_GAP * 15.0) / 16.0).max(6.0)
    } else {
        STEP_BAR_W
    };

    let playhead = {
        let s = app.state.read();
        (s.sequencer.current_step % 16) as i32
    };

    ui.horizontal(|ui| {
        for (i, slot_v) in step_values.iter_mut().enumerate() {
            let (rect, resp) = ui
                .allocate_exact_size(egui::vec2(bar_w, STEP_BAR_H), egui::Sense::click_and_drag());
            // Background — beat markers brighter every 4 steps.
            let beat_lift = if i % 4 == 0 { 8 } else { 0 };
            let bg = egui::Color32::from_gray(28 + beat_lift);
            ui.painter().rect_filled(rect, 1.0, bg);
            // Step value bar.
            let v = slot_v.clamp(0.0, 1.0);
            let h = v * STEP_BAR_H;
            let val_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.bottom() - h),
                egui::vec2(rect.width(), h),
            );
            let bar_col = if i as i32 == playhead {
                theme::CHALK
            } else {
                egui::Color32::from_gray(150)
            };
            ui.painter().rect_filled(val_rect, 1.0, bar_col);
            // Playhead column highlight (top + bottom).
            if i as i32 == playhead {
                let stroke = egui::Stroke::new(1.0, theme::CHALK);
                ui.painter()
                    .line_segment([rect.left_top(), rect.right_top()], stroke);
                ui.painter()
                    .line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
            }
            // Click + drag → set value.  Map y into 0..1 of the
            // bar (top = 1.0, bottom = 0.0).
            if (resp.dragged() || resp.clicked())
                && let Some(p) = resp.interact_pointer_pos()
            {
                let frac = ((rect.bottom() - p.y) / STEP_BAR_H).clamp(0.0, 1.0);
                *slot_v = frac;
                changed = true;
            }
            if i < 15 {
                ui.add_space(STEP_BAR_GAP);
            }
        }
    });

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.cv_seq[slot_idx];
        slot.enabled = enabled;
        slot.target = target;
        slot.depth = depth.clamp(0.0, 1.0);
        slot.step_values = step_values;
    }
}
