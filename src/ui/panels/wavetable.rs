// ─── ui/panels/wavetable.rs ──────────────────────────────────────────────────
// Wavetable voice panel.

use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_wavetable(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().wavetable.enabled;
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
            app.state.write().wavetable.enabled = !enabled;
            app.push_audio_params();
        }

        // LOAD WAV button + filename label.  Mirror of the ConvReverb
        // picker — uses the existing portal-backed picker so the UI
        // thread doesn't try to drive rfd's sync API directly.
        if ui
            .add_sized([56.0, 20.0], egui::Button::new("LOAD WAV"))
            .clicked()
            && let Some(p) = crate::ui::header_menu::pick_file_via_portal("WAV", &["wav", "WAV"])
        {
            let ps = p.to_string_lossy().to_string();
            if let Some(data) = load_wav_to_44100(&ps) {
                let _ = app.audio_tx.push(AudioCommand::LoadWavetable(data));
                app.state.write().wavetable.wave_path = ps.clone();
                app.last_wavetable_path = ps;
            }
        }
        let path = app.state.read().wavetable.wave_path.clone();
        let name = if path.is_empty() {
            "(no table)".to_string()
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

        // Poll for API-driven wave_path changes — mirror of the
        // ConvReverb pattern.
        if !path.is_empty() && app.last_wavetable_path != path {
            if let Some(data) = load_wav_to_44100(&path) {
                let _ = app.audio_tx.push(AudioCommand::LoadWavetable(data));
            }
            app.last_wavetable_path = path;
        }
    });

    ui.add_space(2.0);

    let gw = widgets::even_group_width(ui, 2);
    let group_h = widgets::glass_group_height(ctrl, 60.0);
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            widgets::group_header(ui, "SCAN");
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().wavetable.position;
                    if widgets::param_control(ui, "POS", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().wavetable.position = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().wavetable.phase_offset;
                    if widgets::param_control(ui, "PHASE", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().wavetable.phase_offset = v;
                        app.push_audio_params();
                    }
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            widgets::group_header(ui, "MIX");
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().wavetable.volume;
                    if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().wavetable.volume = v;
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().wavetable.pan;
                    let mut v = (raw + 1.0) * 0.5;
                    if widgets::param_control(ui, "PAN", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().wavetable.pan = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().wavetable.pitch_offset_semi;
                    let mut v = (raw / 48.0) + 0.5;
                    if widgets::param_control(ui, "PITCH", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().wavetable.pitch_offset_semi =
                            ((v - 0.5) * 48.0).clamp(-24.0, 24.0);
                        app.push_audio_params();
                    }
                }
            });
        });
    });
}
