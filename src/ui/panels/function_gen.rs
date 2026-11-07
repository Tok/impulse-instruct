// ─── ui/panels/function_gen.rs ───────────────────────────────────────────────
// Function-generator CV utility panel — ON/OFF + ATTACK / RELEASE
// / CURVE knobs.  Curve knob centre (0.5) = linear; <0.5 = log /
// concave, >0.5 = exp / convex.

use crate::state::ParamMode;
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_function_gen(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::FUNCTION_GEN_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.function_gen[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut attack = snapshot.attack;
    let mut release = snapshot.release;
    let mut curve = snapshot.curve;
    let mut changed = false;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
    });
    widgets::centered_row(ui, |ui| {
        if widgets::param_control(ui, "ATK", &mut attack, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "REL", &mut release, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "CURVE", &mut curve, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("Gate in → AR envelope out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.function_gen[slot_idx];
        slot.enabled = enabled;
        slot.attack = attack.clamp(0.0, 1.0);
        slot.release = release.clamp(0.0, 1.0);
        slot.curve = curve.clamp(0.0, 1.0);
    }
}
