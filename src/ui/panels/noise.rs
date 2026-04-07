// ─── ui/panels/noise.rs ──────────────────────────────────────────────────────
// Noise voice panel — minimal controls (volume, color, cutoff).

use crate::ui::{ImpulseApp, widgets};

pub fn draw_noise(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut vol, mut color, mut cutoff) = {
        let s = app.state.read();
        (
            s.noise_voice.volume,
            s.noise_voice.color,
            s.noise_voice.cutoff,
        )
    };
    let mut changed = false;
    if widgets::param_control(ui, "VOLUME", &mut vol, pm("noise_voice.volume"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "COLOR", &mut color, pm("noise_voice.color"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "CUTOFF", &mut cutoff, pm("noise_voice.cutoff"), ctrl).0 {
        changed = true;
    }
    if changed {
        {
            let mut s = app.state.write();
            s.noise_voice.volume = vol;
            s.noise_voice.color = color;
            s.noise_voice.cutoff = cutoff;
        }
        app.push_audio_params();
    }
}
