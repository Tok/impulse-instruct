// ─── ui/panels/gabber.rs ─────────────────────────────────────────────────────
// Gabber-kick voice panel — a dedicated hardcore kick distinct from 808/909.
// Controls: pitch / decay / pitch-env / clip / transient / volume / pan.

use super::PAN_SLIDER_W;
use crate::ui::{ImpulseApp, widgets};

pub fn draw_gabber(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut pitch, mut decay, mut ped, mut pet, mut clip, mut trans, mut vol, mut pan) = {
        let s = app.state.read();
        (
            s.gabber_kick.pitch,
            s.gabber_kick.decay,
            s.gabber_kick.pitch_env_depth,
            s.gabber_kick.pitch_env_time,
            s.gabber_kick.clip,
            s.gabber_kick.transient,
            s.gabber_kick.volume,
            s.gabber_kick.pan,
        )
    };
    let mut changed = false;

    // PAN slider — right-justified on its own row.
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

    // Row 1: oscillator body — pitch / decay / pitch env depth / pitch env time.
    widgets::centered_row(ui, |ui| {
        if widgets::param_control(ui, "PITCH", &mut pitch, pm("gabber_kick.pitch"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "DECAY", &mut decay, pm("gabber_kick.decay"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(
            ui,
            "P.DEPTH",
            &mut ped,
            pm("gabber_kick.pitch_env_depth"),
            ctrl,
        )
        .0
        {
            changed = true;
        }
        if widgets::param_control(
            ui,
            "P.TIME",
            &mut pet,
            pm("gabber_kick.pitch_env_time"),
            ctrl,
        )
        .0
        {
            changed = true;
        }
    });

    // Row 2: distortion + layering + output — clip / transient / volume.
    widgets::centered_row(ui, |ui| {
        if widgets::param_control(ui, "CLIP", &mut clip, pm("gabber_kick.clip"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "TRANS", &mut trans, pm("gabber_kick.transient"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "VOLUME", &mut vol, pm("gabber_kick.volume"), ctrl).0 {
            changed = true;
        }
    });

    if changed {
        {
            let mut s = app.state.write();
            s.gabber_kick.pitch = pitch;
            s.gabber_kick.decay = decay;
            s.gabber_kick.pitch_env_depth = ped;
            s.gabber_kick.pitch_env_time = pet;
            s.gabber_kick.clip = clip;
            s.gabber_kick.transient = trans;
            s.gabber_kick.volume = vol;
            s.gabber_kick.pan = pan;
        }
        app.push_audio_params();
        app.observe_edits(&[
            ("gabber_kick.pitch", pitch),
            ("gabber_kick.decay", decay),
            ("gabber_kick.pitch_env_depth", ped),
            ("gabber_kick.pitch_env_time", pet),
            ("gabber_kick.clip", clip),
            ("gabber_kick.transient", trans),
            ("gabber_kick.volume", vol),
        ]);
    }
}
