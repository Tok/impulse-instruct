// ─── ui/panels/sample_hold.rs ────────────────────────────────────────────────
// S&H CV utility panel — just an ON/OFF toggle.  The latch is
// automatic on every sequencer step transition; no per-slot
// knobs needed.

use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_sample_hold(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::SAMPLE_HOLD_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.sample_hold[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut changed = false;

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("CV in → latch on step → CV out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.sample_hold[slot_idx];
        slot.enabled = enabled;
    }
}
