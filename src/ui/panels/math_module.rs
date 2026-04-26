// ─── ui/panels/math_module.rs ────────────────────────────────────────────────
// Math CV utility panel — ON/OFF toggle + OP cycle button +
// BLEND knob (only relevant when op = Blend, but kept visible
// always for surface consistency).

use crate::state::{MathOp, ParamMode};
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_math(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::MATH_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.math[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut op = snapshot.op;
    let mut blend = snapshot.blend;
    let mut changed = false;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(6.0);
        if ui
            .button(
                egui::RichText::new(op.name())
                    .color(theme::CHALK)
                    .monospace()
                    .size(9.5),
            )
            .clicked()
        {
            op = op.next();
            changed = true;
        }
        ui.add_space(6.0);
        if widgets::param_control(ui, "BLEND", &mut blend, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new(match op {
            MathOp::Add => "A + B → CV out",
            MathOp::Multiply => "A * B → CV out",
            MathOp::Blend => "lerp(A, B, blend) → CV out",
            MathOp::Max => "max(A, B) → CV out",
            MathOp::Min => "min(A, B) → CV out",
        })
        .monospace()
        .size(7.5)
        .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.math[slot_idx];
        slot.enabled = enabled;
        slot.op = op;
        slot.blend = blend.clamp(0.0, 1.0);
    }
}
