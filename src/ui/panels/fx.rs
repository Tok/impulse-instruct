// ─── ui/panels/fx.rs ──────────────────────────────────────────────────────────
// FX chain panel — all groups in a horizontal_wrapped flow inside a ScrollArea.

use crate::state::ParamMode;
use crate::state::rack::{ModuleKind, PortDir, PortKind, PortRef};
use crate::ui::{ImpulseApp, theme, widgets};

// Voice sources and FX destinations shown in the send matrix.
const VOICE_KINDS: &[ModuleKind] = &[
    ModuleKind::AcidBass,
    ModuleKind::DrumKit808,
    ModuleKind::DrumKit909,
    ModuleKind::HooverLead,
    ModuleKind::An1xVoice,
    ModuleKind::AmenSampler,
    ModuleKind::NoiseVoice,
];
const VOICE_LABELS: &[&str] = &["BASS", "808", "909", "HOV", "AN1X", "AMEN", "NOISE"];
const FX_KINDS: &[ModuleKind] = &[
    ModuleKind::FxReverb,
    ModuleKind::FxDelay,
    ModuleKind::FxChorus,
    ModuleKind::FxPhaser,
    ModuleKind::FxWaveshaper,
    ModuleKind::FxBitcrush,
    ModuleKind::FxEq,
    ModuleKind::FxCompressor,
    ModuleKind::FxTapeSat,
    ModuleKind::FxDrive,
    ModuleKind::FxRingMod,
    ModuleKind::FxAutotune,
];
const FX_LABELS: &[&str] = &[
    "REV", "DLY", "CHR", "PHS", "WVS", "BIT", "EQ", "CMP", "TAPE", "DRV", "RING", "AUTO",
];

fn draw_send_matrix(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let rack = app.state.read().rack.clone();

    // Build a lookup: ModuleKind → module_id (first match)
    let id_of = |kind: ModuleKind| -> Option<u32> {
        rack.modules.iter().find(|m| m.kind == kind).map(|m| m.id)
    };

    // Check if a voice→fx cable exists
    let has_cable = |v_id: u32, fx_id: u32| -> bool {
        rack.cables.iter().any(|c| {
            c.from.module_id == v_id
                && c.from.dir == PortDir::Out
                && c.to.module_id == fx_id
                && c.to.dir == PortDir::In
        })
    };

    ui.add_space(2.0);

    // Header row: FX column labels
    egui::Grid::new("fx_send_matrix")
        .spacing([1.0, 1.0])
        .show(ui, |ui| {
            // Top-left corner cell
            ui.label(
                egui::RichText::new("")
                    .monospace()
                    .size(7.0)
                    .color(theme::PIT),
            );
            for &fx_label in FX_LABELS {
                ui.label(
                    egui::RichText::new(fx_label)
                        .monospace()
                        .size(7.0)
                        .color(theme::PIT),
                );
            }
            ui.end_row();

            // One row per voice
            let mut toggle: Option<(u32, u32, bool)> = None; // (v_id, fx_id, currently_connected)
            for (vi, &vkind) in VOICE_KINDS.iter().enumerate() {
                let Some(v_id) = id_of(vkind) else {
                    ui.end_row();
                    continue;
                };
                ui.label(
                    egui::RichText::new(VOICE_LABELS[vi])
                        .monospace()
                        .size(7.0)
                        .color(theme::SMOKE),
                );
                for &fxkind in FX_KINDS {
                    let Some(fx_id) = id_of(fxkind) else {
                        ui.label("");
                        continue;
                    };
                    let connected = has_cable(v_id, fx_id);
                    let dot = if connected { "◼" } else { "◻" };
                    let col = if connected { theme::FOG } else { theme::PIT };
                    let resp = ui.add(
                        egui::Label::new(egui::RichText::new(dot).monospace().size(9.0).color(col))
                            .sense(egui::Sense::click()),
                    );
                    if resp.clicked() {
                        toggle = Some((v_id, fx_id, connected));
                    }
                }
                ui.end_row();
            }

            // Apply toggle after the immutable borrow of rack is done
            if let Some((v_id, fx_id, was_connected)) = toggle {
                let mut s = app.state.write();
                if was_connected {
                    let from = PortRef {
                        module_id: v_id,
                        dir: PortDir::Out,
                        kind: PortKind::Audio,
                        index: 0,
                    };
                    let to = PortRef {
                        module_id: fx_id,
                        dir: PortDir::In,
                        kind: PortKind::Audio,
                        index: 0,
                    };
                    s.rack.disconnect(&from, &to);
                } else {
                    let from = PortRef {
                        module_id: v_id,
                        dir: PortDir::Out,
                        kind: PortKind::Audio,
                        index: 0,
                    };
                    let to = PortRef {
                        module_id: fx_id,
                        dir: PortDir::In,
                        kind: PortKind::Audio,
                        index: 0,
                    };
                    s.rack.connect(from, to);
                }
                drop(s);
                app.push_fx_plan();
            }
        });

    ui.add_space(2.0);
    ui.separator();
    ui.add_space(2.0);
}

pub fn draw_fx(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    widgets::section_header(ui, "FX CHAIN");
    draw_send_matrix(app, ui);

    // Snapshot all FX values before any widget call
    let (
        mut rs,
        mut rd,
        mut rm,
        mut rgt,
        mut dt,
        mut df,
        mut dm,
        mut dd,
        mut dx,
        mut mv,
        mut ch_rate,
        mut ch_depth,
        mut ch_mix,
        mut ph_rate,
        mut ph_depth,
        mut ph_mix,
        mut ws_drive,
        mut ws_mix,
        mut rm_freq,
        mut rm_mix,
        mut eq_low,
        mut eq_mid,
        mut eq_hi,
        mut comp_thresh,
        mut comp_ratio,
        mut comp_mix,
        mut tape_drive,
        mut tape_mix,
        mut tape_flutter,
        mut master_pitch,
        mut delay_wow,
        mut delay_sat,
        mut sc_amount,
        mut sc_attack,
        mut sc_release,
        mut comp_mb,
        mut stereo_w,
        mut rev_freeze,
        mut comp_reverse,
        locked,
    ) = {
        let s = app.state.read();
        (
            s.fx.reverb_size,
            s.fx.reverb_damp,
            s.fx.reverb_mix,
            s.fx.reverb_gate_time,
            s.fx.delay_time,
            s.fx.delay_feedback,
            s.fx.delay_mix,
            s.fx.distortion_drive,
            s.fx.distortion_mix,
            s.fx.master_volume,
            s.fx.chorus_rate,
            s.fx.chorus_depth,
            s.fx.chorus_mix,
            s.fx.phaser_rate,
            s.fx.phaser_depth,
            s.fx.phaser_mix,
            s.fx.waveshaper_drive,
            s.fx.waveshaper_mix,
            s.fx.ring_mod_freq,
            s.fx.ring_mod_mix,
            s.fx.eq_low_gain,
            s.fx.eq_mid_gain,
            s.fx.eq_hi_gain,
            s.fx.compressor_threshold,
            s.fx.compressor_ratio,
            s.fx.compressor_mix,
            s.fx.tape_drive,
            s.fx.tape_mix,
            s.fx.tape_flutter,
            s.fx.master_pitch_st,
            s.fx.delay_wow_flutter,
            s.fx.delay_saturation,
            s.fx.sidechain_amount,
            s.fx.sidechain_attack,
            s.fx.sidechain_release,
            s.fx.compressor_multiband,
            s.fx.stereo_width,
            s.fx.reverb_freeze,
            s.fx.compressor_reverse,
            s.llm.locked_params.clone(),
        )
    };

    let mut changed = false;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // Helper: derive ParamMode from locked set
    let pm = |path: &str| {
        if locked.contains(path) {
            ParamMode::UserOwned
        } else {
            ParamMode::Free
        }
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        // 9 FX groups — compute equal width so they fill the panel without dead space.
        // Use at most 4 columns so groups don't grow excessively wide on large screens.
        let n_cols = ((ui.available_width() / ctrl.group_max_width()) as usize).clamp(1, 4);
        let gw = widgets::even_group_width(ui, n_cols);
        ui.horizontal_wrapped(|ui| {
            // Macro: horizontal row of knobs inside a group
            macro_rules! hknobs {
                ($ui:expr, $( ($label:expr, $val:expr, $pm:expr) ),+ $(,)?) => {
                    widgets::centered_row($ui, |ui| {
                        $( if widgets::param_control(ui, $label, $val, $pm, ctrl).0 { changed = true; } )+
                    });
                }
            }

            // ── REVERB (3+2 rows) ──────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("REVERB").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("MIX", &mut rm, pm("fx.reverb_mix")), ("SIZE", &mut rs, pm("fx.reverb_size")), ("DAMPING", &mut rd, ParamMode::Free));
                ui.horizontal(|ui| {
                    let mut rgt_norm = rgt / 2.0;
                    if widgets::param_control(ui, "GATE", &mut rgt_norm, pm("fx.reverb_gate_time"), ctrl).0 {
                        rgt = rgt_norm * 2.0; changed = true;
                    }
                    if ui.add(egui::SelectableLabel::new(rev_freeze, "FREEZE")).clicked() {
                        rev_freeze = !rev_freeze; changed = true;
                    }
                });
            });

            // ── DELAY (3+2 rows) ───────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("DELAY").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("MIX", &mut dm, pm("fx.delay_mix")), ("TIME", &mut dt, ParamMode::Free), ("FEEDBACK", &mut df, pm("fx.delay_feedback")));
                hknobs!(ui, ("WOW", &mut delay_wow, pm("fx.delay_wow_flutter")), ("SAT", &mut delay_sat, pm("fx.delay_saturation")));
            });

            // ── CHORUS (1 row) ─────────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("CHORUS").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("MIX", &mut ch_mix, ParamMode::Free), ("RATE", &mut ch_rate, ParamMode::Free), ("DEPTH", &mut ch_depth, ParamMode::Free));
            });

            // ── PHASER (1 row) ─────────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("PHASER").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("MIX", &mut ph_mix, ParamMode::Free), ("RATE", &mut ph_rate, ParamMode::Free), ("DEPTH", &mut ph_depth, ParamMode::Free));
            });

            // ── SHAPE: waveshaper + ring mod (2×2) ─────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("SHAPE").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("WS MIX", &mut ws_mix, ParamMode::Free), ("WS DRV", &mut ws_drive, ParamMode::Free));
                hknobs!(ui, ("RM MIX", &mut rm_mix, ParamMode::Free), ("RM FRQ", &mut rm_freq, ParamMode::Free));
            });

            // ── EQ (1 row) ─────────────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("EQ").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("LOW", &mut eq_low, ParamMode::Free), ("MID", &mut eq_mid, ParamMode::Free), ("HIGH", &mut eq_hi, ParamMode::Free));
            });

            // ── COMPRESSOR (2×2 + REVERSE toggle) ──────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("COMP").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("MIX", &mut comp_mix, ParamMode::Free), ("THRESH", &mut comp_thresh, ParamMode::Free));
                hknobs!(ui, ("RATIO", &mut comp_ratio, ParamMode::Free), ("MULTI", &mut comp_mb, pm("fx.compressor_multiband")));
                // Attack/release swap — the envelope misses the initial
                // transient (slow attack) and releases fast, giving a
                // swell-into-hit feel.  Third FX with a reversal toggle
                // alongside reverb (reverb_dir) and delay (delay_dir).
                if ui
                    .add(egui::SelectableLabel::new(comp_reverse, "REVERSE"))
                    .on_hover_text(
                        "Swap attack/release so the envelope misses the \
                         initial transient and clamps the sustain — classic \
                         reverse-comp swell-into-hit shape."
                    )
                    .clicked()
                {
                    comp_reverse = !comp_reverse;
                    changed = true;
                }
            });

            // ── SIDECHAIN (1 row) ──────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("SIDECHAIN").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("AMOUNT", &mut sc_amount, pm("fx.sidechain_amount")), ("ATTACK", &mut sc_attack, pm("fx.sidechain_attack")), ("RELEASE", &mut sc_release, pm("fx.sidechain_release")));
            });

            // ── TAPE (1 row) ───────────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("TAPE").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("MIX", &mut tape_mix, ParamMode::Free), ("DRIVE", &mut tape_drive, ParamMode::Free), ("FLUTTER", &mut tape_flutter, ParamMode::Free));
            });

            // ── MASTER (3+2+tuning) ────────────────────────────────────────────
            widgets::glass_group_fill(ui, gw, gw, |ui| {
                ui.label(egui::RichText::new("MASTER").color(theme::FOG).monospace().size(9.5));
                hknobs!(ui, ("DRIVE", &mut dd, ParamMode::Free), ("MIX", &mut dx, ParamMode::Free), ("VOLUME", &mut mv, ParamMode::Free));
                ui.horizontal(|ui| {
                    let mut mp_norm = (master_pitch + 12.0) / 24.0;
                    if widgets::param_control(ui, "PITCH", &mut mp_norm, pm("fx.master_pitch_st"), ctrl).0 {
                        master_pitch = mp_norm * 24.0 - 12.0; changed = true;
                    }
                    if widgets::param_control(ui, "WIDTH", &mut stereo_w, pm("fx.stereo_width"), ctrl).0 { changed = true; }
                });
                let tuning_names = ["12-TET", "Just", "Slendro", "Pelog"];
                let mut tuning_idx = app.state.read().fx.tuning as usize;
                if tuning_idx >= tuning_names.len() { tuning_idx = 0; }
                egui::ComboBox::from_id_source("tuning_sel")
                    .width(60.0)
                    .selected_text(tuning_names[tuning_idx])
                    .show_ui(ui, |ui| {
                        for (i, name) in tuning_names.iter().enumerate() {
                            if ui.selectable_label(tuning_idx == i, *name).clicked() {
                                app.state.write().fx.tuning = i as u8; changed = true;
                            }
                        }
                    });
            });
        });
    });

    if changed {
        let mut s = app.state.write();
        s.fx.reverb_size = rs;
        s.fx.reverb_damp = rd;
        s.fx.reverb_mix = rm;
        s.fx.reverb_gate_time = rgt;
        s.fx.delay_time = dt;
        s.fx.delay_feedback = df;
        s.fx.delay_mix = dm;
        s.fx.distortion_drive = dd;
        s.fx.distortion_mix = dx;
        s.fx.master_volume = mv;
        s.fx.chorus_rate = ch_rate;
        s.fx.chorus_depth = ch_depth;
        s.fx.chorus_mix = ch_mix;
        s.fx.phaser_rate = ph_rate;
        s.fx.phaser_depth = ph_depth;
        s.fx.phaser_mix = ph_mix;
        s.fx.waveshaper_drive = ws_drive;
        s.fx.waveshaper_mix = ws_mix;
        s.fx.ring_mod_freq = rm_freq;
        s.fx.ring_mod_mix = rm_mix;
        s.fx.eq_low_gain = eq_low;
        s.fx.eq_mid_gain = eq_mid;
        s.fx.eq_hi_gain = eq_hi;
        s.fx.compressor_threshold = comp_thresh;
        s.fx.compressor_ratio = comp_ratio;
        s.fx.compressor_mix = comp_mix;
        s.fx.tape_drive = tape_drive;
        s.fx.tape_mix = tape_mix;
        s.fx.tape_flutter = tape_flutter;
        s.fx.master_pitch_st = master_pitch;
        s.fx.delay_wow_flutter = delay_wow;
        s.fx.delay_saturation = delay_sat;
        s.fx.sidechain_amount = sc_amount;
        s.fx.sidechain_attack = sc_attack;
        s.fx.sidechain_release = sc_release;
        s.fx.compressor_multiband = comp_mb;
        s.fx.stereo_width = stereo_w;
        s.fx.reverb_freeze = rev_freeze;
        s.fx.compressor_reverse = comp_reverse;
        drop(s);
        app.push_audio_params();
        app.observe_edits(&[
            ("fx.reverb_mix", rm),
            ("fx.delay_mix", dm),
            ("fx.delay_feedback", df),
            ("fx.chorus_mix", ch_mix),
            ("fx.compressor_threshold", comp_thresh),
            ("fx.distortion_drive", dd),
            ("fx.master_volume", mv),
            ("fx.stereo_width", stereo_w),
        ]);
    }
}
