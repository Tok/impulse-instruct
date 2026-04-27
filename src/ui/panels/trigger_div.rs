// ─── ui/panels/trigger_div.rs ────────────────────────────────────────────────
// TriggerDiv CV utility panel — ON/OFF toggle + ratio cycle button.
// Ratios cycle through `crate::state::TRIGGER_DIV_RATIOS` — `/2 /3
// /4 /5 /7`, the most common polyrhythm divisors.

use crate::state::trigger_div::nearest_trigger_div_ratio;
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_trigger_div(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::TRIGGER_DIV_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.trigger_div[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut ratio = nearest_trigger_div_ratio(snapshot.ratio);
    let mut changed = false;

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(8.0);
        // Cycle button — clicking advances to the next ratio in the
        // table, wrapping at the end.  Compact rendering: just shows
        // `÷N` so the user reads the current divisor at a glance.
        let label = format!("÷{ratio}");
        let btn = egui::Button::new(
            egui::RichText::new(label)
                .size(10.0)
                .monospace()
                .color(theme::CHALK),
        )
        .fill(theme::IRON)
        .stroke(egui::Stroke::new(1.0, theme::ASH))
        .min_size(egui::Vec2::new(48.0, 18.0));
        if ui.add(btn).clicked() {
            let cur = crate::state::TRIGGER_DIV_RATIOS
                .iter()
                .position(|&r| r == ratio)
                .unwrap_or(0);
            let next = (cur + 1) % crate::state::TRIGGER_DIV_RATIOS.len();
            ratio = crate::state::TRIGGER_DIV_RATIOS[next];
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("Gate in → 1 every Nth pulse → Gate out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.trigger_div[slot_idx];
        slot.enabled = enabled;
        slot.ratio = ratio;
    }
}
