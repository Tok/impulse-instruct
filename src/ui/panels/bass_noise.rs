// ─── ui/panels/bass_noise.rs ──────────────────────────────────────────────────
// Noise voice section, extracted from bass.rs to stay under the line limit.

use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_noise_section(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    // Force left alignment — the card's centered layout would center the header
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        widgets::section_header(ui, "NOISE VOICE");

        let (noise_enabled, mut noise_volume, mut noise_color, mut noise_cutoff) = {
            let s = app.state.read();
            (
                s.noise_voice.enabled,
                s.noise_voice.volume,
                s.noise_voice.color,
                s.noise_voice.cutoff,
            )
        };

        let label_w = 50.0;
        ui.spacing_mut().slider_width = 200.0;

        // ON toggle — own row
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_w, 18.0],
                egui::Label::new(
                    egui::RichText::new("POWER")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(9.0),
                ),
            );
            let on_color = if noise_enabled {
                theme::CHALK
            } else {
                theme::IRON
            };
            let on_fill = if noise_enabled {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(if noise_enabled { "ON" } else { "OFF" })
                            .monospace()
                            .size(9.0)
                            .color(on_color),
                    )
                    .fill(on_fill),
                )
                .clicked()
            {
                app.state.write().noise_voice.enabled = !noise_enabled;
                app.push_audio_params();
            }
        });

        // VOL slider
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_w, 18.0],
                egui::Label::new(
                    egui::RichText::new("VOL")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(9.0),
                ),
            );
            if ui
                .add(
                    egui::Slider::new(&mut noise_volume, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                app.state.write().noise_voice.volume = noise_volume;
                app.push_audio_params();
            }
        });

        // COLOR slider
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_w, 18.0],
                egui::Label::new(
                    egui::RichText::new("COLOR")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(9.0),
                ),
            );
            if ui
                .add(
                    egui::Slider::new(&mut noise_color, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                app.state.write().noise_voice.color = noise_color;
                app.push_audio_params();
            }
            let color_label = if noise_color < 0.25 {
                "WHITE"
            } else if noise_color < 0.75 {
                "PINK"
            } else {
                "BROWN"
            };
            ui.label(
                egui::RichText::new(color_label)
                    .color(theme::FOG)
                    .monospace()
                    .size(9.0),
            );
        });

        // CUTOFF slider
        ui.horizontal(|ui| {
            ui.add_sized(
                [label_w, 18.0],
                egui::Label::new(
                    egui::RichText::new("CUT")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(9.0),
                ),
            );
            if ui
                .add(
                    egui::Slider::new(&mut noise_cutoff, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                )
                .changed()
            {
                app.state.write().noise_voice.cutoff = noise_cutoff;
                app.push_audio_params();
            }
        });
    }); // end left-aligned layout
}
