// ─── ui/panels/quantizer.rs ──────────────────────────────────────────────────
// Quantizer CV utility panel — ON/OFF toggle + ROOT cycle button
// + SCALE cycle button.  Same per-instance slot mapping idiom as
// the LFO / CV-seq / Slew panels.

use crate::state::{ROOT_NAMES, Scale};
use crate::ui::widgets;
use crate::ui::{ImpulseApp, theme};

pub fn draw_quantizer(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    let slot_idx = slot_idx.min(crate::state::QUANTIZER_SLOTS - 1);
    let snapshot = {
        let s = app.state.read();
        s.quantizer[slot_idx].clone()
    };
    let mut enabled = snapshot.enabled;
    let mut root = snapshot.root.min(11);
    let mut scale = snapshot.scale;
    let mut changed = false;

    ui.horizontal(|ui| {
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            changed = true;
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("ROOT")
                .monospace()
                .size(7.5)
                .color(theme::IRON),
        );
        let root_label = ROOT_NAMES[root as usize];
        if ui
            .button(
                egui::RichText::new(root_label)
                    .color(theme::CHALK)
                    .monospace()
                    .size(9.5),
            )
            .clicked()
        {
            root = (root + 1) % 12;
            changed = true;
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("SCALE")
                .monospace()
                .size(7.5)
                .color(theme::IRON),
        );
        if ui
            .button(
                egui::RichText::new(scale.name())
                    .color(theme::CHALK)
                    .monospace()
                    .size(9.5),
            )
            .clicked()
        {
            scale = next_scale(scale);
            changed = true;
        }
    });
    ui.label(
        egui::RichText::new("CV in → snap → CV out")
            .monospace()
            .size(7.5)
            .color(theme::IRON),
    );

    if changed {
        let mut s = app.state.write();
        let slot = &mut s.quantizer[slot_idx];
        slot.enabled = enabled;
        slot.root = root;
        slot.scale = scale;
    }
}

fn next_scale(s: Scale) -> Scale {
    let all = Scale::all();
    let idx = all.iter().position(|x| *x == s).unwrap_or(0);
    all[(idx + 1) % all.len()]
}
