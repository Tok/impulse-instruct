// ─── ui/panels/comparator.rs ─────────────────────────────────────────────────
// Comparator CV utility panel — ON/OFF toggle + THRESHOLD knob.

use crate::state::ParamMode;
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_comparator(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::COMPARATOR_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.comparator[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut threshold = snapshot.threshold;
    let mut changed = false;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(8.0);
        if widgets::param_control(ui, "THRESH", &mut threshold, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("CV in → 1 if > thresh else 0 → CV out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.comparator[slot_idx];
        slot.enabled = enabled;
        slot.threshold = threshold.clamp(-1.0, 1.5);
    }
}
