// ─── ui/panels/pendulum.rs ────────────────────────────────────────────────────
// Pendulum voice panel — knob bank + live beat-rate readout.  Drone
// voice; no sequencer trigger.  Layout:
//   Row 1: ON/OFF toggle + beat-rate readout ("BEAT 3.0 Hz")
//   Row 2: PITCH / DETUNE / MIX / VOL / PAN knobs

use crate::audio::dsp::pendulum::beat_rate_hz;
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_pendulum(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // Row 1: ON/OFF + live beat-rate readout.  The detune knob's
    // 0..1 range maps to 0..60 Hz of beat (`beat_rate_hz` is the
    // shared helper the DSP uses), so the readout is bit-accurate.
    ui.horizontal(|ui| {
        let enabled = app.state.read().pendulum.enabled;
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
            app.state.write().pendulum.enabled = !enabled;
            app.push_audio_params();
        }
        let detune = app.state.read().pendulum.detune_hz;
        let beat = beat_rate_hz(detune);
        let beat_label = if beat < 1.0 {
            format!("BEAT  {:.2} Hz", beat)
        } else {
            format!("BEAT  {:.1} Hz", beat)
        };
        ui.label(
            egui::RichText::new(beat_label)
                .monospace()
                .size(8.5)
                .color(theme::ASH),
        );
    });

    ui.add_space(2.0);

    // Row 2: PITCH / DETUNE / MIX / VOL / PAN.  Glass-grouped to
    // match the rest of the voice cards.
    let gw = ui.available_width().max(120.0);
    widgets::glass_group_fill(ui, gw, gw, |ui| {
        widgets::centered_row(ui, |ui| {
            {
                let mut v = app.state.read().pendulum.base_pitch;
                if widgets::param_control(ui, "PITCH", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().pendulum.base_pitch = v.clamp(0.0, 1.0);
                    app.push_audio_params();
                }
            }
            {
                let mut v = app.state.read().pendulum.detune_hz;
                if widgets::param_control(ui, "DETUNE", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().pendulum.detune_hz = v.clamp(0.0, 1.0);
                    app.push_audio_params();
                }
            }
            {
                let mut v = app.state.read().pendulum.mix;
                if widgets::param_control(ui, "MIX", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().pendulum.mix = v.clamp(0.0, 1.0);
                    app.push_audio_params();
                }
            }
            {
                let mut v = app.state.read().pendulum.volume;
                if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().pendulum.volume = v.clamp(0.0, 1.5);
                    app.push_audio_params();
                }
            }
            {
                let raw = app.state.read().pendulum.pan;
                let mut v = (raw + 1.0) * 0.5;
                if widgets::param_control(ui, "PAN", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().pendulum.pan = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                    app.push_audio_params();
                }
            }
        });
    });
}
