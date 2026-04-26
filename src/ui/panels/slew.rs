// ─── ui/panels/slew.rs ────────────────────────────────────────────────────────
// Slew CV utility panel — two knobs (ATTACK + RELEASE) plus an
// ON/OFF toggle.  The Mod-In and Cv-Out jacks are drawn by the
// rack canvas's port renderer; this panel just exposes the
// slot's knob controls.
//
// Multiple `Slew` rack instances share the four backing slots in
// `AppState.slew[]` — each instance maps to the slot matching its
// rack-order position (same idiom as `LfoModule`).  Instance 5+
// stacks on slot 4.

use crate::state::ParamMode;
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_slew(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::SLEW_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.slew[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut attack = snapshot.attack;
    let mut release = snapshot.release;
    let mut changed = false;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(8.0);
        if widgets::param_control(ui, "ATTACK", &mut attack, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "RELEASE", &mut release, ParamMode::UserOwned, ctrl).0 {
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("CV in → smooths to → CV out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.slew[slot_idx];
        slot.enabled = enabled;
        slot.attack = attack.clamp(0.0, 1.0);
        slot.release = release.clamp(0.0, 1.0);
    }
}
