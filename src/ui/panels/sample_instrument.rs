// ─── ui/panels/sample_instrument.rs ──────────────────────────────────────────
// Sample-Instrument voice panel.  Mirrors the WavetableVoice panel
// shape (ON/OFF + LOAD WAV + filename label, then a knob row) but the
// knobs are different: ROOT (source-recording note), VOL, PAN, PITCH
// trim in cents.
//
// V1 keeps it lean — a future iteration adds an "auto-detect root"
// button that runs `detect_pitch_hz` on the loaded buffer and writes
// the nearest MIDI note back into state.

use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_sample_instrument(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().sample_instrument.enabled;
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
            app.state.write().sample_instrument.enabled = !enabled;
            app.push_audio_params();
        }

        // LOAD WAV button + filename — same picker pattern as Wavetable.
        // V1.1: also runs auto-detect-root via `detect_pitch_hz` on the
        // loaded buffer; if confidence is decent (>= 0.5) we set the
        // root note to the detected pitch so users don't have to know
        // their sample's source pitch.  Manual root knob still wins —
        // the detect only fires when a fresh file is loaded.
        if ui
            .add_sized([56.0, 20.0], egui::Button::new("LOAD WAV"))
            .clicked()
            && let Some(p) = crate::ui::header_menu::pick_file_via_portal("WAV", &["wav", "WAV"])
        {
            let ps = p.to_string_lossy().to_string();
            if let Some(data) = load_wav_to_44100(&ps) {
                if let Some((hz, conf)) =
                    crate::audio::analysis::detect_pitch_hz(&data, crate::audio::SAMPLE_RATE)
                    && conf >= 0.5
                {
                    let midi = crate::audio::dsp::hz_to_midi(hz).round().clamp(0.0, 127.0) as u8;
                    app.state.write().sample_instrument.root_note = midi;
                }
                let _ = app.audio_tx.push(AudioCommand::LoadSampleInstrument(data));
                app.state.write().sample_instrument.sample_path = ps.clone();
                app.last_sample_instrument_path = ps;
            }
        }
        let path = app.state.read().sample_instrument.sample_path.clone();
        let name = if path.is_empty() {
            "(no sample)".to_string()
        } else {
            std::path::Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        ui.label(
            egui::RichText::new(name)
                .monospace()
                .size(8.0)
                .color(theme::ASH),
        );

        // Poll for API-driven sample_path changes.
        if !path.is_empty() && app.last_sample_instrument_path != path {
            if let Some(data) = load_wav_to_44100(&path) {
                let _ = app.audio_tx.push(AudioCommand::LoadSampleInstrument(data));
            }
            app.last_sample_instrument_path = path;
        }
    });

    ui.add_space(2.0);

    let gw = widgets::even_group_width(ui, 2);
    let group_h = widgets::glass_group_height(ctrl, 60.0);
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("ROOT")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                let raw = app.state.read().sample_instrument.root_note;
                let mut v = raw as f32 / 127.0;
                if widgets::param_control(ui, "NOTE", &mut v, ParamMode::Free, ctrl).0 {
                    let n = (v * 127.0).round().clamp(0.0, 127.0) as u8;
                    app.state.write().sample_instrument.root_note = n;
                    app.push_audio_params();
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("MIX")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.volume;
                    if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.volume = v;
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().sample_instrument.pan;
                    let mut v = (raw + 1.0) * 0.5;
                    if widgets::param_control(ui, "PAN", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.pan = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().sample_instrument.pitch_offset_cents;
                    let mut v = (raw / 200.0) + 0.5;
                    if widgets::param_control(ui, "TRIM", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.pitch_offset_cents =
                            ((v - 0.5) * 200.0).clamp(-100.0, 100.0);
                        app.push_audio_params();
                    }
                }
            });
        });
    });

    ui.add_space(2.0);

    // Row 2: ADSR + loop window.  Two glass groups again.
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("ADSR")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.attack;
                    if widgets::param_control(ui, "A", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.attack = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.decay;
                    if widgets::param_control(ui, "D", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.decay = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.sustain;
                    if widgets::param_control(ui, "S", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.sustain = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.release;
                    if widgets::param_control(ui, "R", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.release = v;
                        app.push_audio_params();
                    }
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("LOOP")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().sample_instrument.loop_start;
                    if widgets::param_control(ui, "STR", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.loop_start = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().sample_instrument.loop_end;
                    if widgets::param_control(ui, "END", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().sample_instrument.loop_end = v.clamp(0.0, 1.0);
                        app.push_audio_params();
                    }
                }
                let on = app.state.read().sample_instrument.loop_enabled;
                let label = if on { "LP" } else { "1×" };
                let col = if on { theme::CHALK } else { theme::IRON };
                if ui
                    .add_sized(
                        [28.0, 18.0],
                        egui::Button::new(
                            egui::RichText::new(label).monospace().size(8.0).color(col),
                        ),
                    )
                    .clicked()
                {
                    app.state.write().sample_instrument.loop_enabled = !on;
                    app.push_audio_params();
                }
            });
        });
    });
}
