// ─── ui/prefs_controls.rs ─────────────────────────────────────────────────────
// Preferences → Controls tab — extracted from windows.rs to stay under 1000 lines.

use crate::ui::{ImpulseApp, theme, widgets};

impl ImpulseApp {
    pub(super) fn draw_controls_tab(&mut self, ui: &mut egui::Ui) {
        // Compact helper: label on left, toggle button on right.
        let toggle_row = |ui: &mut egui::Ui,
                          label: &str,
                          on_label: &str,
                          off_label: &str,
                          val: &mut bool|
         -> bool {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(label)
                        .monospace()
                        .size(9.5)
                        .color(theme::FOG),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::toggle_button(ui, if *val { on_label } else { off_label }, val)
                })
                .inner
            })
            .inner
        };

        let mut prefs = self.state.read().ui_prefs.clone();
        let mut dirty = false;

        // UI scale
        widgets::section_header(ui, "UI SCALE");
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::DragValue::new(&mut prefs.ui_scale)
                        .range(0.5..=3.0)
                        .speed(0.01)
                        .fixed_decimals(2),
                )
                .changed()
            {
                dirty = true;
            }
            ui.label(
                egui::RichText::new("× (0.5 – 3.0, takes effect immediately)")
                    .monospace()
                    .size(9.0)
                    .color(theme::SLATE),
            );
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("reset")
                            .monospace()
                            .size(9.0)
                            .color(theme::ASH),
                    )
                    .fill(theme::PIT)
                    .min_size(egui::vec2(36.0, 16.0)),
                )
                .clicked()
            {
                prefs.ui_scale = 1.0;
                dirty = true;
            }
        });

        ui.add_space(4.0);
        widgets::section_header(ui, "KEYBOARD");
        if toggle_row(
            ui,
            "WASD as arrow keys",
            "ON",
            "OFF",
            &mut prefs.wasd_as_arrows,
        ) {
            dirty = true;
        }

        if dirty {
            self.state.write().ui_prefs = prefs;
            self.session_dirty = true;
        }
        ui.add_space(8.0);
        widgets::section_header(ui, "LOCK BEHAVIOUR");
        {
            let mut alt = self.state.read().llm.auto_lock_on_touch;
            if toggle_row(ui, "Auto-lock on touch", "ON", "OFF", &mut alt) {
                self.state.write().llm.auto_lock_on_touch = alt;
            }
        }
        ui.label(
            egui::RichText::new("  Off: knobs are free — click knob to toggle lock")
                .monospace()
                .size(8.0)
                .color(theme::IRON),
        );
    }
}
