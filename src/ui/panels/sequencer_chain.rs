// ─── ui/panels/sequencer_chain.rs ────────────────────────────────────────────
// Pattern bank selector and chain editor for the sequencer panel.

use crate::state::{
    bank_load, bank_write, chain_pop, chain_push, set_chain_enabled, set_pattern_edit,
};
use crate::ui::{ImpulseApp, theme};

const SLOT_NAMES: [&str; 8] = ["A", "B", "C", "D", "E", "F", "G", "H"];

pub fn draw_pattern_chain(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (pattern_edit, chain, chain_enabled, chain_pos) = {
        let s = app.state.read();
        (
            s.pattern_edit,
            s.chain.clone(),
            s.chain_enabled,
            s.chain_pos,
        )
    };

    // ── Bank row ──────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("BANK")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        for (slot, &name) in SLOT_NAMES.iter().enumerate() {
            let is_edit = slot == pattern_edit;
            let col = if is_edit { theme::CHALK } else { theme::PIT };
            let fill = if is_edit {
                egui::Color32::from_gray(45)
            } else {
                egui::Color32::TRANSPARENT
            };
            let resp = ui.add_sized(
                [20.0, 16.0],
                egui::Button::new(egui::RichText::new(name).monospace().size(9.0).color(col))
                    .fill(fill),
            );
            if resp.clicked() {
                let s = app.state.read().clone();
                *app.state.write() = set_pattern_edit(s, slot);
            }
            if resp.secondary_clicked() {
                let s = app.state.read().clone();
                *app.state.write() = bank_write(s, slot);
            }
            if resp.middle_clicked() {
                let s = app.state.read().clone();
                *app.state.write() = bank_load(s, slot, true);
            }
        }
        ui.label(
            egui::RichText::new("  r-click=save  m-click=load")
                .color(theme::IRON)
                .monospace()
                .size(7.0),
        );
    });

    // ── Chain row ─────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("CHAIN")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        for pos in 0..8usize {
            let label = chain.get(pos).map(|&s| SLOT_NAMES[s % 8]).unwrap_or("·");
            let cursor_here = chain_enabled && !chain.is_empty() && chain_pos % chain.len() == pos;
            let fill = if cursor_here {
                egui::Color32::from_gray(70)
            } else {
                egui::Color32::TRANSPARENT
            };
            let col = if chain.get(pos).is_some() {
                theme::FOG
            } else {
                theme::IRON
            };
            ui.add_sized(
                [20.0, 16.0],
                egui::Button::new(egui::RichText::new(label).monospace().size(9.0).color(col))
                    .fill(fill),
            );
        }
        let small_btn = |ui: &mut egui::Ui, label: &str| {
            ui.add_sized(
                [16.0, 16.0],
                egui::Button::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(9.0)
                        .color(theme::FOG),
                ),
            )
            .clicked()
        };
        if small_btn(ui, "+") {
            let s = app.state.read().clone();
            *app.state.write() = chain_push(s, pattern_edit);
        }
        if small_btn(ui, "−") {
            let s = app.state.read().clone();
            *app.state.write() = chain_pop(s);
        }
        let on_col = if chain_enabled {
            theme::CHALK
        } else {
            theme::IRON
        };
        let on_fill = if chain_enabled {
            egui::Color32::from_gray(50)
        } else {
            egui::Color32::TRANSPARENT
        };
        if ui
            .add_sized(
                [28.0, 16.0],
                egui::Button::new(
                    egui::RichText::new(if chain_enabled { "ON" } else { "OFF" })
                        .monospace()
                        .size(9.0)
                        .color(on_col),
                )
                .fill(on_fill),
            )
            .clicked()
        {
            let s = app.state.read().clone();
            *app.state.write() = set_chain_enabled(s, !chain_enabled);
        }
    });
}
