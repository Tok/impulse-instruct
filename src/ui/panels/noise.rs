// ─── ui/panels/noise.rs ──────────────────────────────────────────────────────
// Noise voice panel — minimal controls (volume, color, cutoff).

use super::PAN_SLIDER_W;
use crate::ui::{ImpulseApp, widgets};

pub fn draw_noise(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut vol, mut color, mut cutoff, mut pan) = {
        let s = app.state.read();
        (
            s.noise_voice.volume,
            s.noise_voice.color,
            s.noise_voice.cutoff,
            s.noise_voice.pan,
        )
    };
    let mut changed = false;
    // PAN slider — right-justified
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pan_slider(ui, &mut pan, PAN_SLIDER_W) {
                changed = true;
            }
            ui.label(
                egui::RichText::new("PAN")
                    .color(crate::ui::theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
        });
    });
    widgets::centered_row(ui, |ui| {
        if widgets::param_control(ui, "VOL", &mut vol, pm("noise_voice.volume"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "COLOR", &mut color, pm("noise_voice.color"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "CUT", &mut cutoff, pm("noise_voice.cutoff"), ctrl).0 {
            changed = true;
        }
    });
    if changed {
        {
            let mut s = app.state.write();
            s.noise_voice.volume = vol;
            s.noise_voice.color = color;
            s.noise_voice.cutoff = cutoff;
            s.noise_voice.pan = pan;
        }
        app.push_audio_params();
        app.observe_edits(&[
            ("noise_voice.volume", vol),
            ("noise_voice.color", color),
            ("noise_voice.cutoff", cutoff),
        ]);
    }
}
