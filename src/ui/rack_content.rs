// ─── ui/rack_content.rs ──────────────────────────────────────────────────────
// Content draw functions for each module kind, split from rack_canvas.rs to
// keep file sizes under the 1000-line limit.

use crate::state::ModuleKind;
use crate::ui::fx_dir::draw_fx_dir_button;
use crate::ui::rack_content_pad::{PAD_SECTION_TOP_GAP, render_three_pad, render_two_pad};
// Re-export so the drum kits (ui/panels/drums.rs) can pull the pad helpers
// from rack_content without reaching into the split sub-module directly.
pub(crate) use crate::ui::rack_content_pad::{render_three_pad_bare, render_two_pad_bare};
use crate::ui::{ImpulseApp, theme};

pub(super) fn draw_voice_content(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    kind: ModuleKind,
    module_id: u32,
) {
    match kind {
        ModuleKind::AcidBass => crate::ui::panels::draw_bass(app, ui),
        ModuleKind::DrumKit808 => crate::ui::panels::draw_kit_a(app, ui),
        ModuleKind::DrumKit909 => crate::ui::panels::draw_kit_b(app, ui),
        ModuleKind::HooverLead => crate::ui::panels::draw_hoover(app, ui),
        ModuleKind::PluckString => crate::ui::panels::draw_pluck(app, ui),
        ModuleKind::WavetableVoice => crate::ui::panels::draw_wavetable(app, ui),
        ModuleKind::SampleInstrument => crate::ui::panels::draw_sample_instrument(app, ui),
        ModuleKind::An1xVoice => crate::ui::panels::draw_an1x(app, ui),
        ModuleKind::AmenSampler => crate::ui::panels::draw_amen(app, ui),
        ModuleKind::NoiseVoice => crate::ui::panels::draw_noise(app, ui),
        ModuleKind::Theremin => crate::ui::panels::draw_theremin(app, ui),
        ModuleKind::Pendulum => crate::ui::panels::draw_pendulum(app, ui),
        ModuleKind::FmOpsVoice => crate::ui::panels::draw_fm_ops(app, ui),
        ModuleKind::AdditiveVoice => crate::ui::panels::draw_additive(app, ui),
        ModuleKind::ModalVoice => crate::ui::panels::draw_modal(app, ui),
        ModuleKind::ChiptuneVoice => crate::ui::panels::draw_chiptune(app, ui),
        ModuleKind::VocalVoice => crate::ui::panels::draw_vocal(app, ui),
        ModuleKind::GranularTexture => crate::ui::panels::draw_granular(app, ui),
        ModuleKind::GabberKick => crate::ui::panels::draw_gabber(app, ui),
        ModuleKind::NeuTts => crate::ui::panels::draw_tts(app, ui, module_id),
        _ => {}
    }
}

pub(super) fn draw_fx_content(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    kind: ModuleKind,
    module_id: u32,
) {
    use crate::ui::widgets;

    let scale: f32 = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("module_scale")))
        .unwrap_or(1.0);
    let ctrl = widgets::ControlPrefs::from_prefs_scaled(&app.state.read().ui_prefs, scale);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = crate::ui::panels::KNOB_SPACING;
    let user_owned = |path: &str| matches!(pm(path), crate::state::ParamMode::UserOwned);
    let (pad_expanded, initial_pad_pair) = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .find(|m| m.id == module_id)
            .map(|m| (m.pad_expanded, m.pad_pair))
            .unwrap_or((false, 0))
    };
    let mut pad_pair = initial_pad_pair;

    // Helper: horizontal row of knobs
    macro_rules! hk {
        ($ui:expr, $( ($label:expr, $val:expr, $pm:expr) ),+ $(,)?) => {
            widgets::centered_row($ui, |ui| {
                $( if widgets::param_control(ui, $label, $val, $pm, ctrl).0 { changed = true; } )+
            });
        }
    }

    match kind {
        ModuleKind::FxReverb => {
            let (mut rs, mut rd, mut rm, mut rdir, mut rq) = {
                let s = app.state.read();
                (
                    s.fx.reverb_size,
                    s.fx.reverb_damp,
                    s.fx.reverb_mix,
                    s.fx.reverb_dir,
                    s.fx.reverb_rev_quant,
                )
            };
            hk!(
                ui,
                ("SIZE", &mut rs, pm("fx.reverb_size")),
                ("DAMPING", &mut rd, pm("fx.reverb_damp")),
                ("MIX", &mut rm, pm("fx.reverb_mix"))
            );
            // Direction + reverse-quant share one row so the rev-quant
            // button doesn't get clipped onto a second row that would
            // overflow narrow Reverb cards.
            let (dir_changed, q_changed) = ui
                .horizontal(|ui| {
                    let d = draw_fx_dir_button(ui, &mut rdir, "Reverb direction");
                    let q = crate::ui::fx_dir::draw_fx_rev_quant_button(ui, &mut rq, "Reverb");
                    (d, q)
                })
                .inner;
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("reverb_xy_{module_id}"),
                    ["SIZE", "DAMP", "MIX"],
                    &mut pad_pair,
                    (&mut rs, &mut rd, &mut rm),
                    [
                        user_owned("fx.reverb_size"),
                        user_owned("fx.reverb_damp"),
                        user_owned("fx.reverb_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || rs != app.state.read().fx.reverb_size || dir_changed || q_changed {
                let mut s = app.state.write();
                s.fx.reverb_size = rs;
                s.fx.reverb_damp = rd;
                s.fx.reverb_mix = rm;
                s.fx.reverb_dir = rdir;
                s.fx.reverb_rev_quant = rq;
            }
        }
        ModuleKind::FxDelay => {
            let (mut dt, mut df, mut dm, mut ddir, mut dq, mut dfz, mut dhpf, mut dlpf) = {
                let s = app.state.read();
                (
                    s.fx.delay_time,
                    s.fx.delay_feedback,
                    s.fx.delay_mix,
                    s.fx.delay_dir,
                    s.fx.delay_rev_quant,
                    s.fx.delay_freeze,
                    s.fx.delay_hpf,
                    s.fx.delay_lpf,
                )
            };
            hk!(
                ui,
                ("TIME", &mut dt, pm("fx.delay_time")),
                ("FEEDBACK", &mut df, pm("fx.delay_feedback")),
                ("MIX", &mut dm, pm("fx.delay_mix"))
            );
            // Dub send/return row: HPF + LPF feedback filters + FREEZE toggle.
            hk!(
                ui,
                ("HPF", &mut dhpf, pm("fx.delay_hpf")),
                ("LPF", &mut dlpf, pm("fx.delay_lpf"))
            );
            let freeze_changed = ui
                .horizontal(|ui| {
                    let d = draw_fx_dir_button(ui, &mut ddir, "Delay direction");
                    let q = crate::ui::fx_dir::draw_fx_rev_quant_button(ui, &mut dq, "Delay");
                    let prev = dfz;
                    if widgets::toggle_button(ui, if dfz { "FRZ" } else { "frz" }, &mut dfz) {
                        // toggle_button only returns `true` when clicked; dfz
                        // already mutated via &mut.  Keep the existing dir/q
                        // change semantics separate so we don't trigger a
                        // write when the user hasn't touched freeze.
                    }
                    (d, q, dfz != prev)
                })
                .inner;
            let (dir_changed, q_changed, fz_changed) = freeze_changed;
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("delay_xy_{module_id}"),
                    ["TIME", "FBK", "MIX"],
                    &mut pad_pair,
                    (&mut dt, &mut df, &mut dm),
                    [
                        user_owned("fx.delay_time"),
                        user_owned("fx.delay_feedback"),
                        user_owned("fx.delay_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed
                || dt != app.state.read().fx.delay_time
                || dir_changed
                || q_changed
                || fz_changed
            {
                let mut s = app.state.write();
                s.fx.delay_time = dt;
                s.fx.delay_feedback = df;
                s.fx.delay_mix = dm;
                s.fx.delay_dir = ddir;
                s.fx.delay_rev_quant = dq;
                s.fx.delay_freeze = dfz;
                s.fx.delay_hpf = dhpf;
                s.fx.delay_lpf = dlpf;
            }
        }
        ModuleKind::FxChorus => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.chorus_rate, s.fx.chorus_depth, s.fx.chorus_mix)
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.chorus_rate")),
                ("DEPTH", &mut d, pm("fx.chorus_depth")),
                ("MIX", &mut m, pm("fx.chorus_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("chorus_xy_{module_id}"),
                    ["RATE", "DEPTH", "MIX"],
                    &mut pad_pair,
                    (&mut r, &mut d, &mut m),
                    [
                        user_owned("fx.chorus_rate"),
                        user_owned("fx.chorus_depth"),
                        user_owned("fx.chorus_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.chorus_rate {
                let mut s = app.state.write();
                s.fx.chorus_rate = r;
                s.fx.chorus_depth = d;
                s.fx.chorus_mix = m;
            }
        }
        ModuleKind::FxPhaser => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.phaser_rate, s.fx.phaser_depth, s.fx.phaser_mix)
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.phaser_rate")),
                ("DEPTH", &mut d, pm("fx.phaser_depth")),
                ("MIX", &mut m, pm("fx.phaser_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("phaser_xy_{module_id}"),
                    ["RATE", "DEPTH", "MIX"],
                    &mut pad_pair,
                    (&mut r, &mut d, &mut m),
                    [
                        user_owned("fx.phaser_rate"),
                        user_owned("fx.phaser_depth"),
                        user_owned("fx.phaser_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.phaser_rate {
                let mut s = app.state.write();
                s.fx.phaser_rate = r;
                s.fx.phaser_depth = d;
                s.fx.phaser_mix = m;
            }
        }
        ModuleKind::FxFlanger
        | ModuleKind::FxLimiter
        | ModuleKind::FxFilter
        | ModuleKind::FxComb
        | ModuleKind::FxTilt
        | ModuleKind::FxTransient
        | ModuleKind::FxExciter
        | ModuleKind::FxGate
        | ModuleKind::FxVocoder
        | ModuleKind::FxWiden
        | ModuleKind::FxFreqShift
        | ModuleKind::FxVinyl
        | ModuleKind::FxMultitap
        | ModuleKind::FxRevDelay
        | ModuleKind::FxTapeStop
        | ModuleKind::FxStutter
        | ModuleKind::FxFreeze
        | ModuleKind::FxDjFilter
        | ModuleKind::FxIsoEq => {
            // FX cards in `rack_content_fx_extras.rs` to keep this file
            // under the 1000-line cap.  Helper returns the new pad_pair.
            // Dispatch list MUST stay in lockstep with the accept list
            // at the top of `try_draw_fx_extras_content` — modules
            // listed there but missing here render empty (the
            // catch-all `_ => {}` below silently swallows them).
            if let Some(new_pair) = crate::ui::rack_content_fx_extras::try_draw_fx_extras_content(
                app,
                ui,
                kind,
                module_id,
                ctrl,
                &locked,
                &focused,
                pad_expanded,
                pad_pair,
            ) {
                pad_pair = new_pair;
            }
        }
        ModuleKind::FxTremolo
        | ModuleKind::FxVibrato
        | ModuleKind::FxDeEsser
        | ModuleKind::FxResBank
        | ModuleKind::FxTapeEcho
        | ModuleKind::FxMultibandComp
        | ModuleKind::FxGrainDelay
        | ModuleKind::FxSpectralGate
        | ModuleKind::FxPlate
        | ModuleKind::FxTranceGate
        | ModuleKind::FxWaveFolder => {
            // Internal-LFO modulation cluster lives in
            // `rack_content_fx_lfo.rs` — same split-for-LOC
            // pattern as the bigger fx_extras file.
            if let Some(new_pair) = crate::ui::rack_content_fx_lfo::try_draw_fx_lfo_content(
                app,
                ui,
                kind,
                module_id,
                ctrl,
                &locked,
                &focused,
                pad_expanded,
                pad_pair,
            ) {
                pad_pair = new_pair;
            }
        }
        ModuleKind::FxEq => {
            let (mut lo, mut mi, mut hi) = {
                let s = app.state.read();
                (s.fx.eq_low_gain, s.fx.eq_mid_gain, s.fx.eq_hi_gain)
            };
            hk!(
                ui,
                ("LOW", &mut lo, pm("fx.eq_low_gain")),
                ("MID", &mut mi, pm("fx.eq_mid_gain")),
                ("HIGH", &mut hi, pm("fx.eq_hi_gain"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("eq_xy_{module_id}"),
                    ["LOW", "MID", "HIGH"],
                    &mut pad_pair,
                    (&mut lo, &mut mi, &mut hi),
                    [
                        user_owned("fx.eq_low_gain"),
                        user_owned("fx.eq_mid_gain"),
                        user_owned("fx.eq_hi_gain"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || lo != app.state.read().fx.eq_low_gain {
                let mut s = app.state.write();
                s.fx.eq_low_gain = lo;
                s.fx.eq_mid_gain = mi;
                s.fx.eq_hi_gain = hi;
            }
        }
        ModuleKind::FxCompressor => {
            let (mut th, mut ra, mut mi, sc_on) = {
                let s = app.state.read();
                (
                    s.fx.compressor_threshold,
                    s.fx.compressor_ratio,
                    s.fx.compressor_mix,
                    s.fx.compressor_sidechain,
                )
            };
            hk!(
                ui,
                ("THRESH", &mut th, pm("fx.compressor_threshold")),
                ("RATIO", &mut ra, pm("fx.compressor_ratio")),
                ("MIX", &mut mi, pm("fx.compressor_mix"))
            );
            // Sidechain toggle.  Compact button below the knob row —
            // when on, the level detector reads the sidechain audio
            // input port (cable into PortKind::SidechainIn) instead of
            // the main signal.  Falls back gracefully to self-detection
            // when no cable is connected.
            ui.horizontal(|ui| {
                let label = if sc_on { "SC ON" } else { "SC" };
                let col = if sc_on {
                    crate::ui::theme::CHALK
                } else {
                    crate::ui::theme::IRON
                };
                if ui
                    .add_sized(
                        [44.0, 16.0],
                        egui::Button::new(
                            egui::RichText::new(label).monospace().size(8.0).color(col),
                        ),
                    )
                    .clicked()
                {
                    app.state.write().fx.compressor_sidechain = !sc_on;
                    changed = true;
                }
            });
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("comp_xy_{module_id}"),
                    ["THR", "RATIO", "MIX"],
                    &mut pad_pair,
                    (&mut th, &mut ra, &mut mi),
                    [
                        user_owned("fx.compressor_threshold"),
                        user_owned("fx.compressor_ratio"),
                        user_owned("fx.compressor_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || th != app.state.read().fx.compressor_threshold {
                let mut s = app.state.write();
                s.fx.compressor_threshold = th;
                s.fx.compressor_ratio = ra;
                s.fx.compressor_mix = mi;
            }
        }
        ModuleKind::FxTapeSat => {
            let (mut dr, mut fl, mut mi) = {
                let s = app.state.read();
                (s.fx.tape_drive, s.fx.tape_flutter, s.fx.tape_mix)
            };
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.tape_drive")),
                ("FLUTTER", &mut fl, pm("fx.tape_flutter")),
                ("MIX", &mut mi, pm("fx.tape_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("tape_xy_{module_id}"),
                    ["DRIVE", "FLUT", "MIX"],
                    &mut pad_pair,
                    (&mut dr, &mut fl, &mut mi),
                    [
                        user_owned("fx.tape_drive"),
                        user_owned("fx.tape_flutter"),
                        user_owned("fx.tape_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || dr != app.state.read().fx.tape_drive {
                let mut s = app.state.write();
                s.fx.tape_drive = dr;
                s.fx.tape_flutter = fl;
                s.fx.tape_mix = mi;
            }
        }
        ModuleKind::FxDrive => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.distortion_drive, s.fx.distortion_mix)
            };
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.distortion_drive")),
                ("MIX", &mut mi, pm("fx.distortion_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                if render_two_pad(
                    ui,
                    &format!("drive_xy_{module_id}"),
                    "DRIVE",
                    "MIX",
                    &mut dr,
                    &mut mi,
                    user_owned("fx.distortion_drive"),
                    user_owned("fx.distortion_mix"),
                ) {
                    changed = true;
                }
            }
            if changed {
                let mut s = app.state.write();
                s.fx.distortion_drive = dr;
                s.fx.distortion_mix = mi;
            }
        }
        ModuleKind::FxAutotune => {
            let (mut amt, mut mi) = {
                let s = app.state.read();
                (s.fx.autotune_amount, s.fx.autotune_mix)
            };
            hk!(
                ui,
                ("AMOUNT", &mut amt, pm("fx.autotune_amount")),
                ("MIX", &mut mi, pm("fx.autotune_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                if render_two_pad(
                    ui,
                    &format!("autotune_xy_{module_id}"),
                    "AMOUNT",
                    "MIX",
                    &mut amt,
                    &mut mi,
                    user_owned("fx.autotune_amount"),
                    user_owned("fx.autotune_mix"),
                ) {
                    changed = true;
                }
            }
            if changed {
                let mut s = app.state.write();
                s.fx.autotune_amount = amt;
                s.fx.autotune_mix = mi;
            }
        }
        ModuleKind::FxPan => {
            let (mut pos, mut width, mut rate) = {
                let s = app.state.read();
                (s.fx.fx_pan_pos, s.fx.fx_pan_width, s.fx.fx_pan_rate)
            };
            // POS control in the panel takes 0..1; map to -1..+1 under the
            // hood so the centre-click / drag feel matches other knobs.
            let mut pos_norm = (pos + 1.0) * 0.5;
            hk!(
                ui,
                ("POS", &mut pos_norm, pm("fx.fx_pan_pos")),
                ("WIDTH", &mut width, pm("fx.fx_pan_width")),
                ("RATE", &mut rate, pm("fx.fx_pan_rate"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("pan_xy_{module_id}"),
                    ["POS", "WIDTH", "RATE"],
                    &mut pad_pair,
                    (&mut pos_norm, &mut width, &mut rate),
                    [
                        user_owned("fx.fx_pan_pos"),
                        user_owned("fx.fx_pan_width"),
                        user_owned("fx.fx_pan_rate"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed {
                pos = (pos_norm * 2.0 - 1.0).clamp(-1.0, 1.0);
                let mut s = app.state.write();
                s.fx.fx_pan_pos = pos;
                s.fx.fx_pan_width = width;
                s.fx.fx_pan_rate = rate;
            }
        }
        ModuleKind::FxWaveshaper => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.waveshaper_drive, s.fx.waveshaper_mix)
            };
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.waveshaper_drive")),
                ("MIX", &mut mi, pm("fx.waveshaper_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                if render_two_pad(
                    ui,
                    &format!("waveshaper_xy_{module_id}"),
                    "DRIVE",
                    "MIX",
                    &mut dr,
                    &mut mi,
                    user_owned("fx.waveshaper_drive"),
                    user_owned("fx.waveshaper_mix"),
                ) {
                    changed = true;
                }
            }
            if changed {
                let mut s = app.state.write();
                s.fx.waveshaper_drive = dr;
                s.fx.waveshaper_mix = mi;
            }
        }
        ModuleKind::FxBitcrush => {
            let (mut bi, mut ra, mut mi) = {
                let s = app.state.read();
                (s.fx.bitcrush_bits, s.fx.bitcrush_rate, s.fx.bitcrush_mix)
            };
            hk!(
                ui,
                ("BITS", &mut bi, pm("fx.bitcrush_bits")),
                ("RATE", &mut ra, pm("fx.bitcrush_rate")),
                ("MIX", &mut mi, pm("fx.bitcrush_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("bitcrush_xy_{module_id}"),
                    ["BITS", "RATE", "MIX"],
                    &mut pad_pair,
                    (&mut bi, &mut ra, &mut mi),
                    [
                        user_owned("fx.bitcrush_bits"),
                        user_owned("fx.bitcrush_rate"),
                        user_owned("fx.bitcrush_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || bi != app.state.read().fx.bitcrush_bits {
                let mut s = app.state.write();
                s.fx.bitcrush_bits = bi;
                s.fx.bitcrush_rate = ra;
                s.fx.bitcrush_mix = mi;
            }
        }
        ModuleKind::FxConvReverb => {
            changed |= super::rack_content_conv_reverb::draw_conv_reverb(
                app,
                ui,
                module_id,
                pad_expanded,
                &mut pad_pair,
            );
        }
        ModuleKind::FxParamEq => {
            changed |= super::rack_content_param_eq::draw_param_eq(app, ui, module_id);
        }
        ModuleKind::FxPitchShift => {
            changed |= super::rack_content_pitch_shift::draw_pitch_shift(
                app,
                ui,
                module_id,
                pad_expanded,
                &mut pad_pair,
            );
        }
        ModuleKind::FxRingMod => {
            let (mut fr, mut mi) = {
                let s = app.state.read();
                (s.fx.ring_mod_freq, s.fx.ring_mod_mix)
            };
            hk!(
                ui,
                ("FREQ", &mut fr, pm("fx.ring_mod_freq")),
                ("MIX", &mut mi, pm("fx.ring_mod_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                if render_two_pad(
                    ui,
                    &format!("ringmod_xy_{module_id}"),
                    "FREQ",
                    "MIX",
                    &mut fr,
                    &mut mi,
                    user_owned("fx.ring_mod_freq"),
                    user_owned("fx.ring_mod_mix"),
                ) {
                    changed = true;
                }
            }
            if changed {
                let mut s = app.state.write();
                s.fx.ring_mod_freq = fr;
                s.fx.ring_mod_mix = mi;
            }
        }
        _ => {}
    }

    // Persist pair cycling (from cycle-chip clicks or right-click on pad)
    // back to the module so it survives save/restore and is API-addressable.
    if pad_pair != initial_pad_pair
        && let Some(m) = app
            .state
            .write()
            .rack
            .modules
            .iter_mut()
            .find(|m| m.id == module_id)
    {
        m.pad_pair = pad_pair;
    }

    if changed {
        app.push_audio_params();
        // Observe key FX params for style tracking
        let fx = &app.state.read().fx.clone();
        app.observe_edits(&[
            ("fx.reverb_mix", fx.reverb_mix),
            ("fx.delay_mix", fx.delay_mix),
            ("fx.delay_feedback", fx.delay_feedback),
            ("fx.chorus_mix", fx.chorus_mix),
            ("fx.compressor_threshold", fx.compressor_threshold),
            ("fx.distortion_drive", fx.distortion_drive),
            ("fx.master_volume", fx.master_volume),
            ("fx.stereo_width", fx.stereo_width),
        ]);
    }
}

pub(super) fn draw_lfo_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_lfo_slot(app, ui, slot);
}

pub(super) fn draw_cv_seq_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // Multiple CvSequencer instances share the four CV-seq slots.
    // Slot index = the instance's order in the rack, capped to
    // CV_SEQ_SLOTS - 1 so a 5th instance stacks on the last slot.
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::CvSequencer)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_cv_seq(app, ui, slot);
}

pub(super) fn draw_slew_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // Same per-instance slot mapping as the CV sequencer / LFO.
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::Slew)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_slew(app, ui, slot);
}

pub(super) fn draw_quantizer_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::Quantizer)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_quantizer(app, ui, slot);
}

pub(super) fn draw_comparator_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::Comparator)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_comparator(app, ui, slot);
}

pub(super) fn draw_sample_hold_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::SampleHold)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_sample_hold(app, ui, slot);
}

pub(super) fn draw_math_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::Math)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_math(app, ui, slot);
}

// Newer CV-utility content dispatchers (TriggerDiv / LogicGate /
// FunctionGen) live in `rack_content_util.rs` (sibling) since this
// file crossed the 1000-line cap during the FunctionGen ship.
// Re-exported via the module declaration in `ui::mod`.

pub(super) fn draw_master_content(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::ui::widgets;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let (
        mut master_vol,
        mut mid_gain,
        mut mid_tilt,
        mut mid_sat,
        mut side_gain,
        mut side_tilt,
        mut side_sat,
    ) = {
        let s = app.state.read();
        (
            s.fx.master_volume,
            s.fx.ms_mid_gain,
            s.fx.ms_mid_tilt,
            s.fx.ms_mid_sat,
            s.fx.ms_side_gain,
            s.fx.ms_side_tilt,
            s.fx.ms_side_sat,
        )
    };
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("MASTER")
                .monospace()
                .size(9.0)
                .color(theme::ASH),
        );
        if widgets::param_control(ui, "VOL", &mut master_vol, pm("fx.master_volume"), ctrl).0 {
            changed = true;
        }
        ui.separator();
        if widgets::param_control(ui, "MID G", &mut mid_gain, pm("fx.ms_mid_gain"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "MID T", &mut mid_tilt, pm("fx.ms_mid_tilt"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "MID S", &mut mid_sat, pm("fx.ms_mid_sat"), ctrl).0 {
            changed = true;
        }
        ui.separator();
        if widgets::param_control(ui, "SIDE G", &mut side_gain, pm("fx.ms_side_gain"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "SIDE T", &mut side_tilt, pm("fx.ms_side_tilt"), ctrl).0 {
            changed = true;
        }
        if widgets::param_control(ui, "SIDE S", &mut side_sat, pm("fx.ms_side_sat"), ctrl).0 {
            changed = true;
        }
        ui.separator();
        let rack = app.state.read();
        let voice_kinds = [
            (crate::state::ModuleKind::AcidBass, "BASS"),
            (crate::state::ModuleKind::DrumKit808, "808"),
            (crate::state::ModuleKind::DrumKit909, "909"),
            (crate::state::ModuleKind::HooverLead, "HVVR"),
            (crate::state::ModuleKind::An1xVoice, "AN1X"),
            (crate::state::ModuleKind::AmenSampler, "AMEN"),
        ];
        for (kind, label) in &voice_kinds {
            let present = rack
                .rack
                .modules
                .iter()
                .any(|m| m.kind == *kind && m.enabled);
            let col = if present {
                egui::Color32::from_gray(160)
            } else {
                theme::PIT
            };
            ui.label(egui::RichText::new(*label).monospace().size(8.0).color(col));
        }
    });

    if changed {
        let mut s = app.state.write();
        s.fx.master_volume = master_vol;
        s.fx.ms_mid_gain = mid_gain;
        s.fx.ms_mid_tilt = mid_tilt;
        s.fx.ms_mid_sat = mid_sat;
        s.fx.ms_side_gain = side_gain;
        s.fx.ms_side_tilt = side_tilt;
        s.fx.ms_side_sat = side_sat;
        drop(s);
        app.push_audio_params();
    }
}

pub(super) use super::agent_card::draw_llm_agent_content;

// Cable-drag + module-drag helpers live in `rack_content_drag.rs`
// (extracted to keep this file under the 1000-line limit).
pub(super) use super::rack_content_drag::{
    handle_cable_drag, handle_title_drag, reorder_module_by_drop,
};
