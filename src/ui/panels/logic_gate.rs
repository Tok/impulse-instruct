// ─── ui/panels/logic_gate.rs ─────────────────────────────────────────────────
// Logic-gate CV utility panel — ON/OFF toggle + AND/OR/XOR cycle button.

use crate::state::LogicOp;
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_logic_gate(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::LOGIC_GATE_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.logic_gate[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut op = snapshot.op;
    let mut changed = false;

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(8.0);
        let label = op.name();
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
            op = match op {
                LogicOp::And => LogicOp::Or,
                LogicOp::Or => LogicOp::Xor,
                LogicOp::Xor => LogicOp::And,
            };
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("Gate A · Gate B → bool → Gate out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.logic_gate[slot_idx];
        slot.enabled = enabled;
        slot.op = op;
    }
}
