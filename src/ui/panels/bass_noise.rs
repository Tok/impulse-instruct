// ─── ui/panels/bass_noise.rs ──────────────────────────────────────────────────
// Noise voice section, extracted from bass.rs to stay under the line limit.

use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_noise_section(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    ui.add_space(8.0);
    widgets::section_header(ui, "NOISE VOICE");
    ui.spacing_mut().slider_width = 200.0;

    let (noise_enabled, mut noise_volume, mut noise_color, mut noise_cutoff) = {
        let s = app.state.read();
        (
            s.noise_voice.enabled,
            s.noise_voice.volume,
            s.noise_voice.color,
            s.noise_voice.cutoff,
        )
    };

    ui.horizontal(|ui| {
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
            .add_sized(
                [28.0, 18.0],
                egui::Button::new(
                    egui::RichText::new("ON")
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

        ui.label(
            egui::RichText::new("VOL")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
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

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("COLOR (WHITE→BROWN)")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
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

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("CUTOFF")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
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
}
