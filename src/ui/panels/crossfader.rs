// ─── ui/panels/crossfader.rs ─────────────────────────────────────────────────
// Crossfader CV utility panel — ON/OFF toggle + MIX knob.

use crate::state::ParamMode;
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_crossfader(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::CROSSFADER_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.crossfader[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut mix = snapshot.mix;
    let mut changed = false;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(8.0);
        if widgets::param_control(ui, "MIX", &mut mix, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("CV A · CV B → lerp(A, B, MIX) → CV out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.crossfader[slot_idx];
        slot.enabled = enabled;
        slot.mix = mix.clamp(0.0, 1.0);
    }
}
