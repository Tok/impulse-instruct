// ─── ui/panels/fm_ops.rs ──────────────────────────────────────────────────────
// FM operator synth voice panel.  4-op DX7-flavoured voice — header
// row (ON/OFF + algorithm picker + volume / pan / feedback) plus
// four operator rows (RATIO / LEVEL / ATTACK / DECAY / SUSTAIN /
// RELEASE).  Each op row is glass-grouped so the four ops read as
// distinct units; algorithm picker is a row of four chips with
// labels describing the routing.

use crate::state::{FM_ALGORITHM_COUNT, ParamMode};
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_fm_ops(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // ── Header row: ON/OFF + algorithm picker + global volume / pan / feedback
    ui.horizontal(|ui| {
        let enabled = app.state.read().fm_ops.enabled;
        let btn_text = if enabled { "ON" } else { "OFF" };
        let btn_color = if enabled { theme::CHALK } else { theme::IRON };
        let btn_fill = if enabled {
            egui::Color32::from_gray(55)
        } else {
            egui::Color32::from_gray(22)
        };
        if ui
            .add_sized(
                [36.0, 20.0],
                egui::Button::new(
                    egui::RichText::new(btn_text)
                        .monospace()
                        .size(8.5)
                        .color(btn_color),
                )
                .fill(btn_fill),
            )
            .clicked()
        {
            app.state.write().fm_ops.enabled = !enabled;
            app.push_audio_params();
        }

        // Algorithm picker — 4 chips with terse routing labels so
        // the user can read the topology at a glance.
        let cur_alg = app
            .state
            .read()
            .fm_ops
            .algorithm
            .min(FM_ALGORITHM_COUNT - 1);
        let labels = ["4→3→2→1", "4,3,2 → 1", "4→3 / 2→1", "1+2+3+4"];
        for (i, label) in labels.iter().enumerate() {
            let active = cur_alg == i as u8;
            let col = if active { theme::CHALK } else { theme::ASH };
            let fill = if active {
                egui::Color32::from_gray(55)
            } else {
                egui::Color32::from_gray(22)
            };
            if ui
                .add_sized(
                    [62.0, 20.0],
                    egui::Button::new(egui::RichText::new(*label).monospace().size(8.0).color(col))
                        .fill(fill),
                )
                .clicked()
            {
                app.state.write().fm_ops.algorithm = i as u8;
                app.push_audio_params();
            }
        }

        // Global VOLUME, PAN, FEEDBACK.
        let mut vol = app.state.read().fm_ops.volume;
        if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
            app.state.write().fm_ops.volume = vol.clamp(0.0, 1.5);
            app.push_audio_params();
        }
        let raw_pan = app.state.read().fm_ops.pan;
        let mut pan = (raw_pan + 1.0) * 0.5;
        if widgets::param_control(ui, "PAN", &mut pan, ParamMode::Free, ctrl).0 {
            app.state.write().fm_ops.pan = (pan * 2.0 - 1.0).clamp(-1.0, 1.0);
            app.push_audio_params();
        }
        let mut fbk = app.state.read().fm_ops.feedback;
        if widgets::param_control(ui, "FEEDBACK", &mut fbk, ParamMode::Free, ctrl).0 {
            app.state.write().fm_ops.feedback = fbk.clamp(0.0, 1.0);
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

    // ── 4 op rows.  Each row is one glass group containing the
    // op's six knobs (RATIO / LEVEL / ATTACK / DECAY / SUSTAIN /
    // RELEASE).  RATIO + LEVEL get φ-bigger because they're the
    // FM character knobs the user reaches for first; ADSR sits at
    // default size as the shaping detail.
    let big = ctrl.phi_bigger();
    let avail = ui.available_width();
    let group_h = widgets::glass_group_height(ctrl, 35.0);
    for op_idx in 0..4 {
        widgets::glass_group_fill(ui, avail, avail, |ui| {
            ui.set_min_height(group_h);
            let label = match op_idx {
                0 => "OP 1",
                1 => "OP 2",
                2 => "OP 3",
                _ => "OP 4",
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .color(theme::FOG)
                        .monospace()
                        .size(10.0),
                );
                let (mut ratio, mut level, mut a, mut d, mut sus, mut r) = {
                    let s = app.state.read();
                    let op = match op_idx {
                        0 => &s.fm_ops.op1,
                        1 => &s.fm_ops.op2,
                        2 => &s.fm_ops.op3,
                        _ => &s.fm_ops.op4,
                    };
                    (
                        op.ratio, op.level, op.attack, op.decay, op.sustain, op.release,
                    )
                };
                let mut changed = false;
                if widgets::param_control(ui, "RATIO", &mut ratio, ParamMode::Free, big).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut level, ParamMode::Free, big).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "ATTACK", &mut a, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "DECAY", &mut d, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SUSTAIN", &mut sus, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "RELEASE", &mut r, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if changed {
                    let mut s = app.state.write();
                    let op = match op_idx {
                        0 => &mut s.fm_ops.op1,
                        1 => &mut s.fm_ops.op2,
                        2 => &mut s.fm_ops.op3,
                        _ => &mut s.fm_ops.op4,
                    };
                    op.ratio = ratio.clamp(0.0, 1.0);
                    op.level = level.clamp(0.0, 1.0);
                    op.attack = a.clamp(0.0, 1.0);
                    op.decay = d.clamp(0.0, 1.0);
                    op.sustain = sus.clamp(0.0, 1.0);
                    op.release = r.clamp(0.0, 1.0);
                    drop(s);
                    app.push_audio_params();
                }
            });
        });
        ui.add_space(2.0);
    }
}
