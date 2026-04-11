// ─── ui/panels/drums.rs ───────────────────────────────────────────────────────
// Drum kit panels: Kit A (808-style), Kit B (909-style), and Amen sampler.

use super::PAN_SLIDER_W;
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
        mut kpan_a,
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
            s.kit_a.kick.pan,
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
    let xy_size = app.state.read().ui_prefs.effective_xy_px();
    let avail = ui.available_width();
    let gw = ((avail - super::GLASS_GAP) / 2.0).floor(); // 2-column layout
    let group_h = ctrl.knob_size * 2.0 + 50.0;

    // PAN slider — right-justified
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pan_slider(ui, &mut kpan_a, PAN_SLIDER_W) {
                changed = true;
            }
            ui.label(
                egui::RichText::new("PAN")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
        });
    });

    // Row 1: KICK (left) + KICK XY PAD (right)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("KICK")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
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
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "P.DPT", &mut kped, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "P.TIM", &mut kpet, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "CLIP", &mut kclip, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
        // KICK XY PAD (right column, padded)
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.set_min_height(group_h);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("KICK: PITCH × DECAY")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
            // Constrain pad to fit within the cell with padding
            let pad_size = (gw - 40.0).min(xy_size).min(group_h - 30.0);
            if widgets::xy_pad(
                ui,
                "drums_kick_xy",
                "PIT",
                "DEC",
                &mut kp,
                &mut kd,
                pad_size,
                false,
                1,
            )
            .0
            {
                let mut s = app.state.write();
                s.kit_a.kick.pitch = kp;
                s.kit_a.kick.decay = kd;
                drop(s);
                app.push_audio_params();
            }
        });
    });

    // Row 2: SNARE (left) + HIHAT (right)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("SNARE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "TONE", &mut st, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SNAPPY", &mut ssn, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "DECAY", &mut sd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut sv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("HIHAT")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "CLOSED", &mut hcd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "OPEN", &mut hod, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "LEVEL", &mut hv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
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
        s.kit_a.kick.pan = kpan_a;
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
        app.observe_edits(&[
            ("kit_a.kick.pitch", kp),
            ("kit_a.kick.decay", kd),
            ("kit_a.kick.punch", kpu),
            ("kit_a.kick.volume", kv),
            ("kit_a.snare.tone", st),
            ("kit_a.snare.snappy", ssn),
            ("kit_a.snare.decay", sd),
            ("kit_a.snare.volume", sv),
            ("kit_a.hihat_closed.decay", hcd),
            ("kit_a.hihat_closed.volume", hv),
        ]);
    }
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
        mut kpan_b,
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
            s.kit_b.kick.pan,
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
    let ctrl_big = ctrl.phi_bigger(); // larger knobs for the important KICK params
    let avail = ui.available_width();
    let gw_half = ((avail - super::GLASS_GAP) / 2.0).floor();

    // PAN slider — right-justified
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::pan_slider(ui, &mut kpan_b, PAN_SLIDER_W) {
                changed = true;
            }
            ui.label(
                egui::RichText::new("PAN")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            );
        });
    });

    // Row 1: KICK — full width, bigger knobs (most important for 909)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("KICK")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        widgets::centered_row(ui, |ui| {
            if widgets::param_control(ui, "PITCH", &mut kp, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut kd, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
            if widgets::param_control(ui, "PUNCH", &mut kpu, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
            if widgets::param_control(ui, "LEVEL", &mut kv, ParamMode::Free, ctrl_big).0 {
                changed = true;
            }
        });
        widgets::centered_row(ui, |ui| {
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
    });

    // Row 2: SNARE (left) + CLAP/RIM (right) — all knobs single row each
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = super::GLASS_GAP;
        widgets::glass_group_fill(ui, gw_half, gw_half, |ui| {
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("SNARE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
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
        });
        widgets::glass_group_fill(ui, gw_half, gw_half, |ui| {
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            ui.label(
                egui::RichText::new("CLAP / RIM")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "DECAY", &mut cd, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "LEVEL", &mut cv, ParamMode::Free, ctrl).0 {
                    changed = true;
                }
            });
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
        s.kit_b.kick.pan = kpan_b;
        s.kit_b.snare.tone = st;
        s.kit_b.snare.snappy = ssn;
        s.kit_b.snare.decay = sd;
        s.kit_b.snare.volume = sv;
        s.kit_b.clap.decay = cd;
        s.kit_b.clap.volume = cv;
        drop(s);
        app.push_audio_params();
        app.observe_edits(&[
            ("kit_b.kick.pitch", kp),
            ("kit_b.kick.decay", kd),
            ("kit_b.kick.punch", kpu),
            ("kit_b.kick.volume", kv),
            ("kit_b.snare.tone", st),
            ("kit_b.snare.snappy", ssn),
            ("kit_b.snare.decay", sd),
            ("kit_b.snare.volume", sv),
            ("kit_b.clap.decay", cd),
            ("kit_b.clap.volume", cv),
        ]);
    }
}

// ─── Amen / WAV sampler panel ─────────────────────────────────────────────────

pub fn draw_amen(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let mut path = app.state.read().amen.path.clone();
    let (mut vol, mut pitch, mut loop_mode) = {
        let s = app.state.read();
        (s.amen.volume, s.amen.pitch, s.amen.loop_mode)
    };
    let mut changed = false;

    // Row 1: file input + load button (full width)
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut path)
                .hint_text("amen.wav")
                .desired_width(ui.available_width() - 30.0)
                .font(egui::FontId::monospace(8.0)),
        );
        if resp.changed() {
            app.state.write().amen.path = path.clone();
        }
        if ui
            .small_button(egui::RichText::new("LD").monospace().size(7.0))
            .clicked()
            && let Some(data) = load_wav_to_44100(&path)
        {
            let _ = app.audio_tx.push(AudioCommand::LoadSampler(data));
        }
    });
    // Row 2: knobs
    ui.horizontal(|ui| {
        if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
            changed = true;
        }
        let mut pitch_norm = (pitch + 24.0) / 48.0;
        if widgets::param_control(ui, "PITCH", &mut pitch_norm, ParamMode::Free, ctrl).0 {
            pitch = pitch_norm * 48.0 - 24.0;
            changed = true;
        }
        if widgets::toggle_button(ui, if loop_mode { "LOOP" } else { "ONE" }, &mut loop_mode) {
            changed = true;
        }
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
