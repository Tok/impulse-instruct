// ─── ui/panels/drums.rs ───────────────────────────────────────────────────────
// Drum kit panels: Kit A (808-style), Kit B (909-style), and Amen sampler.

use crate::audio::{AudioCommand, load_wav_to_44100};
use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_kit_a(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Snapshot all values before any widget rendering
    let (
        mut kp,
        mut kd,
        mut kpu,
        mut kv,
        mut kped,
        mut kpet,
        mut kclip,
        mut st,
        mut ssn,
        mut sd,
        mut sv,
        mut hcd,
        mut hod,
        mut hv,
    ) = {
        let s = app.state.read();
        (
            s.kit_a.kick.pitch,
            s.kit_a.kick.decay,
            s.kit_a.kick.punch,
            s.kit_a.kick.volume,
            s.kit_a.kick.pitch_env_depth,
            s.kit_a.kick.pitch_env_time,
            s.kit_a.kick.clip,
            s.kit_a.snare.tone,
            s.kit_a.snare.snappy,
            s.kit_a.snare.decay,
            s.kit_a.snare.volume,
            s.kit_a.hihat_closed.decay,
            s.kit_a.hihat_open.decay,
            s.kit_a.hihat_closed.volume,
        )
    };
    let mut changed = false;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let xy_size = app.state.read().ui_prefs.pad_size.px() * (88.0 / 26.0);
    ui.horizontal_wrapped(|ui| {
        widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
            ui.label(
                egui::RichText::new("KICK")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "PITCH", &mut kp, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut kd, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "PUNCH", &mut kpu, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut kv, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "P.DEPTH", &mut kped, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "P.TIME", &mut kpet, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "CLIP", &mut kclip, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
        ui.add_space(4.0);
        widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
            ui.label(
                egui::RichText::new("SNARE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "TONE", &mut st, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "SNAPPY", &mut ssn, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut sd, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut sv, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
        ui.add_space(4.0);
        widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
            ui.label(
                egui::RichText::new("HIHAT")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "CLOSED", &mut hcd, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "OPEN", &mut hod, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut hv, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
    });

    // Single brief write with all changes
    if changed {
        let mut s = app.state.write();
        s.kit_a.kick.pitch = kp;
        s.kit_a.kick.decay = kd;
        s.kit_a.kick.punch = kpu;
        s.kit_a.kick.volume = kv;
        s.kit_a.kick.pitch_env_depth = kped;
        s.kit_a.kick.pitch_env_time = kpet;
        s.kit_a.kick.clip = kclip;
        s.kit_a.snare.tone = st;
        s.kit_a.snare.snappy = ssn;
        s.kit_a.snare.decay = sd;
        s.kit_a.snare.volume = sv;
        s.kit_a.hihat_closed.decay = hcd;
        s.kit_a.hihat_open.decay = hod;
        s.kit_a.hihat_closed.volume = hv;
        s.kit_a.hihat_open.volume = hv;
        drop(s);
        app.push_audio_params();
    }

    // PITCH × DECAY XY pad for quick kick shaping
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new("KICK: PITCH × DECAY")
            .color(theme::SMOKE)
            .monospace()
            .size(9.0),
    );
    ui.horizontal(|ui| {
        if widgets::xy_pad(ui, "PIT", "DEC", &mut kp, &mut kd, xy_size, false) {
            let mut s = app.state.write();
            s.kit_a.kick.pitch = kp;
            s.kit_a.kick.decay = kd;
            drop(s);
            app.push_audio_params();
        }
    });
}

pub fn draw_kit_b(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (
        mut kp,
        mut kd,
        mut kpu,
        mut kv,
        mut kped,
        mut kpet,
        mut kclip,
        mut st,
        mut ssn,
        mut sd,
        mut sv,
        mut cd,
        mut cv,
    ) = {
        let s = app.state.read();
        (
            s.kit_b.kick.pitch,
            s.kit_b.kick.decay,
            s.kit_b.kick.punch,
            s.kit_b.kick.volume,
            s.kit_b.kick.pitch_env_depth,
            s.kit_b.kick.pitch_env_time,
            s.kit_b.kick.clip,
            s.kit_b.snare.tone,
            s.kit_b.snare.snappy,
            s.kit_b.snare.decay,
            s.kit_b.snare.volume,
            s.kit_b.clap.decay,
            s.kit_b.clap.volume,
        )
    };
    let mut changed = false;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    ui.horizontal_wrapped(|ui| {
        widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
            ui.label(
                egui::RichText::new("KICK")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "PITCH", &mut kp, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut kd, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "PUNCH", &mut kpu, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut kv, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "P.DEPTH", &mut kped, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "P.TIME", &mut kpet, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "CLIP", &mut kclip, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
        ui.add_space(4.0);
        widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
            ui.label(
                egui::RichText::new("SNARE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "TONE", &mut st, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "SNAPPY", &mut ssn, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut sd, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut sv, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
        ui.add_space(4.0);
        widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
            ui.label(
                egui::RichText::new("CLAP / RIM")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            if widgets::param_control(ui, "CLAP DEC", &mut cd, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "CLAP LVL", &mut cv, ParamMode::Free, ctrl).0 {
                changed = true;
            }
        });
    });

    if changed {
        let mut s = app.state.write();
        s.kit_b.kick.pitch = kp;
        s.kit_b.kick.decay = kd;
        s.kit_b.kick.punch = kpu;
        s.kit_b.kick.volume = kv;
        s.kit_b.kick.pitch_env_depth = kped;
        s.kit_b.kick.pitch_env_time = kpet;
        s.kit_b.kick.clip = kclip;
        s.kit_b.snare.tone = st;
        s.kit_b.snare.snappy = ssn;
        s.kit_b.snare.decay = sd;
        s.kit_b.snare.volume = sv;
        s.kit_b.clap.decay = cd;
        s.kit_b.clap.volume = cv;
        drop(s);
        app.push_audio_params();
    }
}

// ─── Amen / WAV sampler panel ─────────────────────────────────────────────────

pub fn draw_amen(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // ── File path + Load button ───────────────────────────────────────────────
    let mut path = app.state.read().amen.path.clone();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("WAV file")
                .monospace()
                .size(9.5)
                .color(theme::FOG),
        );
    });
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut path)
                .hint_text("path/to/amen.wav")
                .desired_width(ui.available_width() - 60.0),
        );
        if resp.changed() {
            app.state.write().amen.path = path.clone();
        }
        if ui.button("Load").clicked() {
            match load_wav_to_44100(&path) {
                Some(data) => {
                    let _ = app.audio_tx.push(AudioCommand::LoadSampler(data));
                    log::info!("Amen sampler: loaded '{}'", path);
                }
                None => {
                    log::warn!("Amen sampler: could not load '{}'", path);
                }
            }
        }
    });

    ui.add_space(4.0);

    // ── Volume + Pitch knobs ──────────────────────────────────────────────────
    let (mut vol, mut pitch, mut loop_mode) = {
        let s = app.state.read();
        (s.amen.volume, s.amen.pitch, s.amen.loop_mode)
    };
    let mut changed = false;

    widgets::glass_group(ui, ctrl.group_max_width(), |ui| {
        ui.horizontal_wrapped(|ui| {
            if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
                changed = true;
            }
            let mut pitch_norm = (pitch + 24.0) / 48.0; // -24..+24 → 0..1
            if widgets::param_control(ui, "PITCH", &mut pitch_norm, ParamMode::Free, ctrl).0 {
                pitch = pitch_norm * 48.0 - 24.0;
                changed = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Loop")
                    .monospace()
                    .size(9.5)
                    .color(theme::FOG),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::toggle_button(ui, if loop_mode { "ON" } else { "OFF" }, &mut loop_mode)
                {
                    changed = true;
                }
            });
        });
    });

    if changed {
        let mut s = app.state.write();
        s.amen.volume = vol;
        s.amen.pitch = pitch;
        s.amen.loop_mode = loop_mode;
        drop(s);
        app.push_audio_params();
    }
}
