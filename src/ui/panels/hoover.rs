// ─── ui/panels/hoover.rs ─────────────────────────────────────────────────────
// Hoover lead voice panel.

use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_hoover(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs).flat();

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

    // ── Three equal-width glass groups ────────────────────────────────────────
    let gw = widgets::even_group_width(ui, 3);
    ui.horizontal(|ui| {
        // FILTER group: START, SWEEP, RESO (horizontal)
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.label(
                egui::RichText::new("FILTER")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().hoover.filter_start;
                    if widgets::param_control(ui, "START", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.filter_start = v;
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().hoover.sweep_time;
                    let mut v = (raw - 0.1) / (4.0 - 0.1);
                    if widgets::param_control(ui, "SWEEP", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.sweep_time = v * (4.0 - 0.1) + 0.1;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().hoover.resonance;
                    if widgets::param_control(ui, "RESO", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.resonance = v;
                        app.push_audio_params();
                    }
                }
            });
        });
        // OSC group: DETUNE + VOL horizontal, VOICES stepper
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.label(
                egui::RichText::new("OSC")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().hoover.detune;
                    if widgets::param_control(ui, "DETUNE", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.detune = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().hoover.volume;
                    if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.volume = v;
                        app.push_audio_params();
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut pan = app.state.read().hoover.pan;
                    if widgets::pan_slider(ui, &mut pan, super::PAN_SLIDER_W) {
                        app.state.write().hoover.pan = pan;
                        app.push_audio_params();
                    }
                    ui.label(
                        egui::RichText::new("PAN")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(7.5),
                    );
                });
            });
            widgets::centered_row(ui, |ui| {
                let voices = app.state.read().hoover.voices;
                ui.label(
                    egui::RichText::new(format!("V:{}", voices))
                        .color(theme::SMOKE)
                        .monospace()
                        .size(8.5),
                );
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
        // LFO group: RATE + DEPTH horizontal
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.label(
                egui::RichText::new("PITCH LFO")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let raw = app.state.read().hoover.pitch_lfo_rate;
                    let mut v = raw / 8.0;
                    if widgets::param_control(ui, "RATE", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.pitch_lfo_rate = v * 8.0;
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().hoover.pitch_lfo_depth;
                    let mut v = raw / 2.0;
                    if widgets::param_control(ui, "DEPTH", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().hoover.pitch_lfo_depth = v * 2.0;
                        app.push_audio_params();
                    }
                }
            });
        });
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
    // Observe edits for style tracking (snapshot then observe)
    let (fs, res, det, vol) = {
        let h = &app.state.read().hoover;
        (h.filter_start, h.resonance, h.detune, h.volume)
    };
    app.observe_edits(&[
        ("hoover.filter_start", fs),
        ("hoover.resonance", res),
        ("hoover.detune", det),
        ("hoover.volume", vol),
    ]);
}
