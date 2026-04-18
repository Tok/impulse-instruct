// ─── ui/panels/drums.rs ───────────────────────────────────────────────────────
// Drum kit panels: Kit A (808-style) and Kit B (909-style).
// (AmenSampler panel moved to ui/panels/amen.rs.)

use super::PAN_SLIDER_W;
use crate::state::ParamMode;
use crate::ui::rack_content::{render_three_pad_bare, render_two_pad_bare};
use crate::ui::{ImpulseApp, theme, widgets};

/// Vertical gap that stacks the voice label on top of the knobs + pad row
/// inside each per-voice glass group.
const DRUM_VOICE_HEADER_H: f32 = 12.0;
/// Gap between the knob column and the pad column inside a voice group.
const DRUM_KNOB_PAD_GAP: f32 = 8.0;

/// Compute a pad size that uses the voice's XY preference but never
/// steals so much width that the knob column can't fit its own knobs.
/// `avail` is the card width, `knob_budget` is how many pixels the knob
/// column should keep as a minimum.
fn drum_pad_size(avail: f32, xy_pref: f32, knob_budget: f32) -> f32 {
    // glass_group has 14 px of horizontal chrome; leave that plus the gap +
    // ~40 px for the side cycle chip (or ~8 px for a 2-knob pad).  The caller
    // passes the side-chip cost inside `knob_budget` so this function just
    // balances knobs vs pad from whatever's left.  Drums cap lower than the
    // FX pads so all three voice groups fit the card's grid envelope.
    let inner = (avail - 14.0).max(100.0);
    let for_pad = (inner - knob_budget - DRUM_KNOB_PAD_GAP).max(50.0);
    xy_pref.min(for_pad).clamp(50.0, 80.0)
}

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

    // Honour per-module scale (set via Ctrl+scroll) — ControlPrefs needs the
    // scale so knob sizes match the card's outer scaling, otherwise the panel
    // overflows a shrunk-down card and the pad's hit region drifts off its
    // visible position via the ScrollArea fallback in module_card_sized.
    let scale: f32 = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("module_scale")))
        .unwrap_or(1.0);
    let ctrl = widgets::ControlPrefs::from_prefs_scaled(&app.state.read().ui_prefs, scale);
    let xy_size = app.state.read().ui_prefs.effective_xy_px() * scale;
    let avail = ui.available_width();

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

    // ── KICK voice: knobs (left, 2 rows) + PITCH × DECAY × PUNCH pad (right)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("KICK")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        ui.add_space(DRUM_VOICE_HEADER_H - 10.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
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
            ui.add_space(DRUM_KNOB_PAD_GAP);
            // Pad + side cycle chip — 3-pair (PITCH × DECAY, × PUNCH combos)
            let pad_size = drum_pad_size(avail, xy_size, ctrl.knob_size * 4.0 + 50.0);
            if render_three_pad_bare(
                ui,
                "kit_a_kick_xy",
                ["PITCH", "DECAY", "PUNCH"],
                (&mut kp, &mut kd, &mut kpu),
                [false, false, false],
                pad_size,
            ) {
                changed = true;
            }
        });
    });

    ui.add_space(super::GLASS_GAP);

    // ── SNARE voice: knobs (left) + TONE × SNAPPY × DECAY pad (right)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("SNARE")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
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
            ui.add_space(DRUM_KNOB_PAD_GAP);
            let pad_size = drum_pad_size(avail, xy_size, ctrl.knob_size * 4.0 + 50.0);
            if render_three_pad_bare(
                ui,
                "kit_a_snare_xy",
                ["TONE", "SNAPPY", "DECAY"],
                (&mut st, &mut ssn, &mut sd),
                [false, false, false],
                pad_size,
            ) {
                changed = true;
            }
        });
    });

    ui.add_space(super::GLASS_GAP);

    // ── HIHAT voice: knobs (left) + CLOSED × OPEN 2-pair pad (right)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("HIHAT")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                widgets::centered_row(ui, |ui| {
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
            ui.add_space(DRUM_KNOB_PAD_GAP);
            let pad_size = drum_pad_size(avail, xy_size, ctrl.knob_size * 3.0 + 10.0);
            if render_two_pad_bare(
                ui,
                "kit_a_hihat_xy",
                "CLOSED",
                "OPEN",
                &mut hcd,
                &mut hod,
                false,
                false,
                pad_size,
            ) {
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

    let scale: f32 = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("module_scale")))
        .unwrap_or(1.0);
    let ctrl = widgets::ControlPrefs::from_prefs_scaled(&app.state.read().ui_prefs, scale);
    let xy_size = app.state.read().ui_prefs.effective_xy_px() * scale;
    let avail = ui.available_width();

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

    // ── KICK voice: knobs (left, 2 rows) + PITCH × DECAY × PUNCH pad (right)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("KICK")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
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
            ui.add_space(DRUM_KNOB_PAD_GAP);
            let pad_size = drum_pad_size(avail, xy_size, ctrl.knob_size * 4.0 + 50.0);
            if render_three_pad_bare(
                ui,
                "kit_b_kick_xy",
                ["PITCH", "DECAY", "PUNCH"],
                (&mut kp, &mut kd, &mut kpu),
                [false, false, false],
                pad_size,
            ) {
                changed = true;
            }
        });
    });

    ui.add_space(super::GLASS_GAP);

    // ── SNARE voice: knobs (left) + TONE × SNAPPY × DECAY pad (right)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("SNARE")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
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
            ui.add_space(DRUM_KNOB_PAD_GAP);
            let pad_size = drum_pad_size(avail, xy_size, ctrl.knob_size * 4.0 + 50.0);
            if render_three_pad_bare(
                ui,
                "kit_b_snare_xy",
                ["TONE", "SNAPPY", "DECAY"],
                (&mut st, &mut ssn, &mut sd),
                [false, false, false],
                pad_size,
            ) {
                changed = true;
            }
        });
    });

    ui.add_space(super::GLASS_GAP);

    // ── CLAP / RIM voice: knobs (left) + DECAY × LEVEL 2-pair pad (right)
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
        ui.label(
            egui::RichText::new("CLAP / RIM")
                .color(theme::FOG)
                .monospace()
                .size(9.5),
        );
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                widgets::centered_row(ui, |ui| {
                    if widgets::param_control(ui, "DECAY", &mut cd, ParamMode::Free, ctrl).0 {
                        changed = true;
                    }
                    if widgets::param_control(ui, "LEVEL", &mut cv, ParamMode::Free, ctrl).0 {
                        changed = true;
                    }
                });
            });
            ui.add_space(DRUM_KNOB_PAD_GAP);
            let pad_size = drum_pad_size(avail, xy_size, ctrl.knob_size * 2.0 + 10.0);
            if render_two_pad_bare(
                ui,
                "kit_b_clap_xy",
                "DECAY",
                "LEVEL",
                &mut cd,
                &mut cv,
                false,
                false,
                pad_size,
            ) {
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
