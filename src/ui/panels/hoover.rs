// ─── ui/panels/hoover.rs ─────────────────────────────────────────────────────
// Hoover lead voice panel.

use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_hoover(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().hoover.enabled;
        let btn_text = if enabled { "ON" } else { "OFF" };
        let btn_color = if enabled { theme::CHALK } else { theme::IRON };
        let btn_fill = if enabled {
            egui::Color32::from_gray(55)
        } else {
            egui::Color32::from_gray(22)
        };
        if ui
            .add_sized(
                [36.0, 20.0],
                egui::Button::new(
                    egui::RichText::new(btn_text)
                        .monospace()
                        .size(8.5)
                        .color(btn_color),
                )
                .fill(btn_fill),
            )
            .clicked()
        {
            app.state.write().hoover.enabled = !enabled;
            app.push_audio_params();
        }

        ui.label(
            egui::RichText::new("supersaw → HP sweep → resonance")
                .color(theme::PIT)
                .monospace()
                .size(7.5),
        );
    });

    ui.add_space(2.0);

    // ── Filter row ────────────────────────────────────────────────────────────
    widgets::section_header(ui, "FILTER  (HP sweeps down on trigger)");
    ui.horizontal(|ui| {
        {
            let mut v = app.state.read().hoover.filter_start;
            let (ch, _) = widgets::param_control(ui, "START", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.filter_start = v;
                app.push_audio_params();
            }
        }
        {
            // sweep_time: 0.1–4.0 s, normalized to 0–1 for knob
            let raw = app.state.read().hoover.sweep_time;
            let mut v = (raw - 0.1) / (4.0 - 0.1);
            let (ch, _) = widgets::param_control(ui, "SWEEP", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.sweep_time = v * (4.0 - 0.1) + 0.1;
                app.push_audio_params();
            }
        }
        {
            let mut v = app.state.read().hoover.resonance;
            let (ch, _) = widgets::param_control(ui, "RESO", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.resonance = v;
                app.push_audio_params();
            }
        }
    });

    ui.add_space(4.0);

    // ── Oscillator row ────────────────────────────────────────────────────────
    widgets::section_header(ui, "OSCILLATOR");
    ui.horizontal(|ui| {
        {
            let mut v = app.state.read().hoover.detune;
            let (ch, _) = widgets::param_control(ui, "DETUNE", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.detune = v;
                app.push_audio_params();
            }
        }

        // Voice count stepper
        ui.vertical(|ui| {
            let voices = app.state.read().hoover.voices;
            ui.label(
                egui::RichText::new(format!("VOICES: {}", voices))
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.5),
            );
            ui.horizontal(|ui| {
                if ui.small_button("-").clicked() && voices > 2 {
                    app.state.write().hoover.voices = voices - 1;
                    app.push_audio_params();
                }
                if ui.small_button("+").clicked() && voices < 7 {
                    app.state.write().hoover.voices = voices + 1;
                    app.push_audio_params();
                }
            });
        });

        {
            let mut v = app.state.read().hoover.volume;
            let (ch, _) = widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.volume = v;
                app.push_audio_params();
            }
        }
    });

    ui.add_space(4.0);

    // ── LFO row ───────────────────────────────────────────────────────────────
    widgets::section_header(ui, "PITCH LFO  (wail)");
    ui.horizontal(|ui| {
        {
            // pitch_lfo_rate: 0–8 Hz, normalized to 0–1
            let raw = app.state.read().hoover.pitch_lfo_rate;
            let mut v = raw / 8.0;
            let (ch, _) = widgets::param_control(ui, "RATE", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.pitch_lfo_rate = v * 8.0;
                app.push_audio_params();
            }
        }
        {
            // pitch_lfo_depth: 0–2 semitones, normalized to 0–1
            let raw = app.state.read().hoover.pitch_lfo_depth;
            let mut v = raw / 2.0;
            let (ch, _) = widgets::param_control(ui, "DEPTH", &mut v, ParamMode::Free, ctrl);
            if ch {
                app.state.write().hoover.pitch_lfo_depth = v * 2.0;
                app.push_audio_params();
            }
        }
    });

    ui.add_space(2.0);

    // ── Preset button ─────────────────────────────────────────────────────────
    if ui
        .add_sized(
            [110.0, 18.0],
            egui::Button::new(
                egui::RichText::new("RESET PRESET")
                    .monospace()
                    .size(7.5)
                    .color(theme::SMOKE),
            )
            .fill(egui::Color32::from_gray(28)),
        )
        .on_hover_text("Apply classic Hoover lead settings and enable voice")
        .clicked()
    {
        let s = app.state.read().clone();
        *app.state.write() = crate::state::apply_hoover_preset(s);
        app.push_audio_params();
    }
}
