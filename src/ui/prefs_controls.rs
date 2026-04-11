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

        widgets::section_header(ui, "KNOB LAYOUT");
        let mut prefs = self.state.read().ui_prefs.clone();
        let mut dirty = false;
        if toggle_row(
            ui,
            "Control style",
            "SLIDERS",
            "KNOBS",
            &mut prefs.use_sliders,
        ) {
            dirty = true;
        }

        if !prefs.use_sliders {
            use crate::state::KnobStyle;
            let mut chrome = prefs.knob_style == KnobStyle::Chrome;
            if toggle_row(ui, "Knob style", "CHROME", "FLAT", &mut chrome) {
                prefs.knob_style = if chrome {
                    KnobStyle::Chrome
                } else {
                    KnobStyle::Flat
                };
                dirty = true;
            }
        }

        // Knob size — fibonacci steps + optional custom px
        ui.add_space(4.0);
        widgets::section_header(ui, "KNOB SIZE");
        ui.horizontal(|ui| {
            use crate::state::KnobSize;
            for (label, size) in [
                ("S", KnobSize::Small),
                ("M", KnobSize::Normal),
                ("L", KnobSize::Large),
                ("XL", KnobSize::XL),
            ] {
                let active = prefs.knob_size == size && prefs.custom_knob_px.is_none();
                let col = if active { theme::CHALK } else { theme::ASH };
                let fill = if active { theme::IRON } else { theme::PIT };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(label).monospace().size(9.5).color(col),
                        )
                        .fill(fill)
                        .min_size(egui::vec2(28.0, 18.0)),
                    )
                    .on_hover_text(format!("{:.0} px", size.body_px()))
                    .clicked()
                {
                    prefs.knob_size = size;
                    prefs.custom_knob_px = None;
                    dirty = true;
                }
            }
            let mut custom_knob = prefs.custom_knob_px.unwrap_or(prefs.effective_knob_px());
            if ui
                .add(
                    egui::DragValue::new(&mut custom_knob)
                        .range(12.0..=200.0)
                        .speed(1.0)
                        .suffix(" px"),
                )
                .changed()
            {
                prefs.custom_knob_px = Some(custom_knob);
                dirty = true;
            }
            if prefs.custom_knob_px.is_some()
                && ui
                    .small_button(egui::RichText::new("↺").color(theme::ASH))
                    .clicked()
            {
                prefs.custom_knob_px = None;
                dirty = true;
            }
        });

        // Sequencer step size
        ui.add_space(4.0);
        widgets::section_header(ui, "SEQ STEP SIZE");
        ui.horizontal(|ui| {
            use crate::state::PadSize;
            for (label, size) in [
                ("S", PadSize::Small),
                ("M", PadSize::Normal),
                ("L", PadSize::Large),
                ("XL", PadSize::XL),
            ] {
                let active = prefs.pad_size == size && prefs.custom_pad_px.is_none();
                let col = if active { theme::CHALK } else { theme::ASH };
                let fill = if active { theme::IRON } else { theme::PIT };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(label).monospace().size(9.5).color(col),
                        )
                        .fill(fill)
                        .min_size(egui::vec2(28.0, 18.0)),
                    )
                    .on_hover_text(format!("{:.0} px", size.px()))
                    .clicked()
                {
                    prefs.pad_size = size;
                    prefs.custom_pad_px = None;
                    dirty = true;
                }
            }
            let mut custom_pad = prefs.custom_pad_px.unwrap_or(prefs.effective_pad_px());
            if ui
                .add(
                    egui::DragValue::new(&mut custom_pad)
                        .range(10.0..=150.0)
                        .speed(1.0)
                        .suffix(" px"),
                )
                .changed()
            {
                prefs.custom_pad_px = Some(custom_pad);
                dirty = true;
            }
            if prefs.custom_pad_px.is_some()
                && ui
                    .small_button(egui::RichText::new("↺").color(theme::ASH))
                    .clicked()
            {
                prefs.custom_pad_px = None;
                dirty = true;
            }
        });

        // XY control pad size
        ui.add_space(4.0);
        widgets::section_header(ui, "XY PAD SIZE");
        ui.horizontal(|ui| {
            let mut custom_xy = prefs.custom_xy_px.unwrap_or(prefs.effective_xy_px());
            if ui
                .add(
                    egui::DragValue::new(&mut custom_xy)
                        .range(40.0..=400.0)
                        .speed(1.0)
                        .suffix(" px"),
                )
                .changed()
            {
                prefs.custom_xy_px = Some(custom_xy);
                dirty = true;
            }
            if prefs.custom_xy_px.is_some()
                && ui
                    .small_button(egui::RichText::new("↺").color(theme::ASH))
                    .clicked()
            {
                prefs.custom_xy_px = None;
                dirty = true;
            }
            ui.label(
                egui::RichText::new("(↺ = auto from step size)")
                    .color(theme::IRON)
                    .monospace()
                    .size(8.0),
            );
        });

        // Envelope/ADSR display height
        ui.add_space(4.0);
        widgets::section_header(ui, "ENV HEIGHT");
        ui.horizontal(|ui| {
            let mut custom_env = prefs.custom_env_h.unwrap_or(prefs.effective_env_h());
            if ui
                .add(
                    egui::DragValue::new(&mut custom_env)
                        .range(16.0..=200.0)
                        .speed(1.0)
                        .suffix(" px"),
                )
                .changed()
            {
                prefs.custom_env_h = Some(custom_env);
                dirty = true;
            }
            if prefs.custom_env_h.is_some()
                && ui
                    .small_button(egui::RichText::new("↺").color(theme::ASH))
                    .clicked()
            {
                prefs.custom_env_h = None;
                dirty = true;
            }
            ui.label(
                egui::RichText::new("(↺ = auto from XY size)")
                    .color(theme::IRON)
                    .monospace()
                    .size(8.0),
            );
        });

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
