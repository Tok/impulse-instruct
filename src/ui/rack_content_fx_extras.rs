// ─── ui/rack_content_fx_extras.rs ────────────────────────────────────────────
// Per-card render functions for the Tier-1 FX modules (Limiter, Filter,
// Comb, Tilt, Transient, Exciter, plus Flanger).  Split from rack_content.rs
// to keep that file under the 1000-line cap; called via
// `try_draw_fx_extras_content` which returns true on a match so the caller
// can short-circuit its big `match kind` block.
//
// Each card recreates the same `pm` / `user_owned` / `hk!` boilerplate the
// outer `draw_fx_content` uses — the duplication is the cost of avoiding a
// large `DrawCtx` struct that would have to thread `&mut app` and
// `&mut ui` through every helper.

use std::collections::HashSet;

use crate::state::ModuleKind;
use crate::ui::ImpulseApp;
use crate::ui::rack_content_pad::{PAD_SECTION_TOP_GAP, render_three_pad};
use crate::ui::widgets::{self, ControlPrefs};

/// Render a Tier-1 FX card if `kind` is one of the new variants.  Returns
/// `Some(pad_pair)` on hit (the outer scope writes it back to the rack).
/// Returns `None` if `kind` isn't handled here, so the caller can fall
/// through to the existing match.
pub(super) fn try_draw_fx_extras_content(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    kind: ModuleKind,
    module_id: u32,
    ctrl: ControlPrefs,
    locked: &HashSet<String>,
    focused: &HashSet<String>,
    pad_expanded: bool,
    pad_pair_in: u8,
) -> Option<u8> {
    match kind {
        ModuleKind::FxFlanger
        | ModuleKind::FxLimiter
        | ModuleKind::FxFilter
        | ModuleKind::FxComb
        | ModuleKind::FxTilt
        | ModuleKind::FxTransient
        | ModuleKind::FxExciter
        | ModuleKind::FxMultitap
        | ModuleKind::FxRevDelay
        | ModuleKind::FxTapeStop
        | ModuleKind::FxStutter
        | ModuleKind::FxFreeze
        | ModuleKind::FxGate
        | ModuleKind::FxVocoder => {}
        _ => return None,
    }

    let pm = |path: &str| crate::state::param_mode(path, locked, focused);
    let user_owned = |path: &str| matches!(pm(path), crate::state::ParamMode::UserOwned);
    let mut changed = false;
    let mut pad_pair = pad_pair_in;

    macro_rules! hk {
        ($ui:expr, $( ($label:expr, $val:expr, $pm:expr) ),+ $(,)?) => {
            widgets::centered_row($ui, |ui| {
                $( if widgets::param_control(ui, $label, $val, $pm, ctrl).0 { changed = true; } )+
            });
        }
    }

    match kind {
        ModuleKind::FxFlanger => {
            let (mut r, mut d, mut fb, mut m) = {
                let s = app.state.read();
                (
                    s.fx.flanger_rate,
                    s.fx.flanger_depth,
                    s.fx.flanger_feedback,
                    s.fx.flanger_mix,
                )
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.flanger_rate")),
                ("DEPTH", &mut d, pm("fx.flanger_depth"))
            );
            hk!(
                ui,
                ("FBK", &mut fb, pm("fx.flanger_feedback")),
                ("MIX", &mut m, pm("fx.flanger_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("flanger_xy_{module_id}"),
                    ["RATE", "DEPTH", "MIX"],
                    &mut pad_pair,
                    (&mut r, &mut d, &mut m),
                    [
                        user_owned("fx.flanger_rate"),
                        user_owned("fx.flanger_depth"),
                        user_owned("fx.flanger_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.flanger_rate {
                let mut s = app.state.write();
                s.fx.flanger_rate = r;
                s.fx.flanger_depth = d;
                s.fx.flanger_feedback = fb;
                s.fx.flanger_mix = m;
            }
        }
        ModuleKind::FxLimiter => {
            let (mut th, mut ce, mut rl, mut la) = {
                let s = app.state.read();
                (
                    s.fx.limiter_threshold,
                    s.fx.limiter_ceiling,
                    s.fx.limiter_release,
                    s.fx.limiter_lookahead,
                )
            };
            hk!(
                ui,
                ("THRESH", &mut th, pm("fx.limiter_threshold")),
                ("CEIL", &mut ce, pm("fx.limiter_ceiling"))
            );
            hk!(
                ui,
                ("REL", &mut rl, pm("fx.limiter_release")),
                ("LOOK", &mut la, pm("fx.limiter_lookahead"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("limiter_xy_{module_id}"),
                    ["THR", "CEIL", "REL"],
                    &mut pad_pair,
                    (&mut th, &mut ce, &mut rl),
                    [
                        user_owned("fx.limiter_threshold"),
                        user_owned("fx.limiter_ceiling"),
                        user_owned("fx.limiter_release"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || th != app.state.read().fx.limiter_threshold {
                let mut s = app.state.write();
                s.fx.limiter_threshold = th;
                s.fx.limiter_ceiling = ce;
                s.fx.limiter_release = rl;
                s.fx.limiter_lookahead = la;
            }
        }
        ModuleKind::FxFilter => {
            let (mut cu, mut re, mut dr, mut mi, mut mode) = {
                let s = app.state.read();
                (
                    s.fx.svf_cutoff,
                    s.fx.svf_resonance,
                    s.fx.svf_drive,
                    s.fx.svf_mix,
                    s.fx.svf_mode,
                )
            };
            hk!(
                ui,
                ("CUT", &mut cu, pm("fx.svf_cutoff")),
                ("RES", &mut re, pm("fx.svf_resonance"))
            );
            hk!(
                ui,
                ("DRV", &mut dr, pm("fx.svf_drive")),
                ("MIX", &mut mi, pm("fx.svf_mix"))
            );
            // Mode selector — single row of 4 small radio-style buttons.
            let mode_changed = ui
                .horizontal(|ui| {
                    let labels = ["LP", "BP", "HP", "NCH"];
                    let mut any_changed = false;
                    for (i, lbl) in labels.iter().enumerate() {
                        let sel = mode == i as u8;
                        if ui.add(egui::SelectableLabel::new(sel, *lbl)).clicked() {
                            mode = i as u8;
                            any_changed = true;
                        }
                    }
                    any_changed
                })
                .inner;
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("svf_xy_{module_id}"),
                    ["CUT", "RES", "MIX"],
                    &mut pad_pair,
                    (&mut cu, &mut re, &mut mi),
                    [
                        user_owned("fx.svf_cutoff"),
                        user_owned("fx.svf_resonance"),
                        user_owned("fx.svf_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || mode_changed || cu != app.state.read().fx.svf_cutoff {
                let mut s = app.state.write();
                s.fx.svf_cutoff = cu;
                s.fx.svf_resonance = re;
                s.fx.svf_drive = dr;
                s.fx.svf_mix = mi;
                s.fx.svf_mode = mode;
            }
        }
        ModuleKind::FxComb => {
            let (mut p, mut fb, mut dp, mut mi) = {
                let s = app.state.read();
                (
                    s.fx.comb_pitch,
                    s.fx.comb_feedback,
                    s.fx.comb_damp,
                    s.fx.comb_mix,
                )
            };
            hk!(
                ui,
                ("PITCH", &mut p, pm("fx.comb_pitch")),
                ("FBK", &mut fb, pm("fx.comb_feedback"))
            );
            hk!(
                ui,
                ("DAMP", &mut dp, pm("fx.comb_damp")),
                ("MIX", &mut mi, pm("fx.comb_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("comb_xy_{module_id}"),
                    ["PIT", "FBK", "MIX"],
                    &mut pad_pair,
                    (&mut p, &mut fb, &mut mi),
                    [
                        user_owned("fx.comb_pitch"),
                        user_owned("fx.comb_feedback"),
                        user_owned("fx.comb_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || p != app.state.read().fx.comb_pitch {
                let mut s = app.state.write();
                s.fx.comb_pitch = p;
                s.fx.comb_feedback = fb;
                s.fx.comb_damp = dp;
                s.fx.comb_mix = mi;
            }
        }
        ModuleKind::FxTilt => {
            let (mut t, mut p, mut m) = {
                let s = app.state.read();
                (s.fx.tilt_tilt, s.fx.tilt_pivot, s.fx.tilt_mix)
            };
            hk!(
                ui,
                ("TILT", &mut t, pm("fx.tilt_tilt")),
                ("PIVOT", &mut p, pm("fx.tilt_pivot")),
                ("MIX", &mut m, pm("fx.tilt_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("tilt_xy_{module_id}"),
                    ["TILT", "PIV", "MIX"],
                    &mut pad_pair,
                    (&mut t, &mut p, &mut m),
                    [
                        user_owned("fx.tilt_tilt"),
                        user_owned("fx.tilt_pivot"),
                        user_owned("fx.tilt_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || t != app.state.read().fx.tilt_tilt {
                let mut s = app.state.write();
                s.fx.tilt_tilt = t;
                s.fx.tilt_pivot = p;
                s.fx.tilt_mix = m;
            }
        }
        ModuleKind::FxTransient => {
            let (mut a, mut su, mut m) = {
                let s = app.state.read();
                (
                    s.fx.transient_attack,
                    s.fx.transient_sustain,
                    s.fx.transient_mix,
                )
            };
            hk!(
                ui,
                ("ATTACK", &mut a, pm("fx.transient_attack")),
                ("SUSTAIN", &mut su, pm("fx.transient_sustain")),
                ("MIX", &mut m, pm("fx.transient_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("transient_xy_{module_id}"),
                    ["ATT", "SUS", "MIX"],
                    &mut pad_pair,
                    (&mut a, &mut su, &mut m),
                    [
                        user_owned("fx.transient_attack"),
                        user_owned("fx.transient_sustain"),
                        user_owned("fx.transient_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || a != app.state.read().fx.transient_attack {
                let mut s = app.state.write();
                s.fx.transient_attack = a;
                s.fx.transient_sustain = su;
                s.fx.transient_mix = m;
            }
        }
        ModuleKind::FxExciter => {
            let (mut a, mut f, mut m) = {
                let s = app.state.read();
                (s.fx.exciter_amount, s.fx.exciter_freq, s.fx.exciter_mix)
            };
            hk!(
                ui,
                ("AMT", &mut a, pm("fx.exciter_amount")),
                ("FREQ", &mut f, pm("fx.exciter_freq")),
                ("MIX", &mut m, pm("fx.exciter_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("exciter_xy_{module_id}"),
                    ["AMT", "FRQ", "MIX"],
                    &mut pad_pair,
                    (&mut a, &mut f, &mut m),
                    [
                        user_owned("fx.exciter_amount"),
                        user_owned("fx.exciter_freq"),
                        user_owned("fx.exciter_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || a != app.state.read().fx.exciter_amount {
                let mut s = app.state.write();
                s.fx.exciter_amount = a;
                s.fx.exciter_freq = f;
                s.fx.exciter_mix = m;
            }
        }
        ModuleKind::FxMultitap => {
            let (mut t, mut sp, mut fb, mut m) = {
                let s = app.state.read();
                (
                    s.fx.multitap_time,
                    s.fx.multitap_spread,
                    s.fx.multitap_feedback,
                    s.fx.multitap_mix,
                )
            };
            hk!(
                ui,
                ("TIME", &mut t, pm("fx.multitap_time")),
                ("SPREAD", &mut sp, pm("fx.multitap_spread"))
            );
            hk!(
                ui,
                ("FBK", &mut fb, pm("fx.multitap_feedback")),
                ("MIX", &mut m, pm("fx.multitap_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("multitap_xy_{module_id}"),
                    ["TIME", "SPREAD", "MIX"],
                    &mut pad_pair,
                    (&mut t, &mut sp, &mut m),
                    [
                        user_owned("fx.multitap_time"),
                        user_owned("fx.multitap_spread"),
                        user_owned("fx.multitap_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || t != app.state.read().fx.multitap_time {
                let mut s = app.state.write();
                s.fx.multitap_time = t;
                s.fx.multitap_spread = sp;
                s.fx.multitap_feedback = fb;
                s.fx.multitap_mix = m;
            }
        }
        ModuleKind::FxRevDelay => {
            let (mut t, mut fb, mut m) = {
                let s = app.state.read();
                (
                    s.fx.revdelay_time,
                    s.fx.revdelay_feedback,
                    s.fx.revdelay_mix,
                )
            };
            hk!(
                ui,
                ("TIME", &mut t, pm("fx.revdelay_time")),
                ("FBK", &mut fb, pm("fx.revdelay_feedback"))
            );
            hk!(ui, ("MIX", &mut m, pm("fx.revdelay_mix")));
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("revdelay_xy_{module_id}"),
                    ["TIME", "FBK", "MIX"],
                    &mut pad_pair,
                    (&mut t, &mut fb, &mut m),
                    [
                        user_owned("fx.revdelay_time"),
                        user_owned("fx.revdelay_feedback"),
                        user_owned("fx.revdelay_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || t != app.state.read().fx.revdelay_time {
                let mut s = app.state.write();
                s.fx.revdelay_time = t;
                s.fx.revdelay_feedback = fb;
                s.fx.revdelay_mix = m;
            }
        }
        ModuleKind::FxTapeStop => {
            let (mut m, mut t) = {
                let s = app.state.read();
                (s.fx.tapestop_mix, s.fx.tapestop_time)
            };
            hk!(
                ui,
                ("STOP", &mut m, pm("fx.tapestop_mix")),
                ("TIME", &mut t, pm("fx.tapestop_time"))
            );
            if changed || m != app.state.read().fx.tapestop_mix {
                let mut s = app.state.write();
                s.fx.tapestop_mix = m;
                s.fx.tapestop_time = t;
            }
        }
        ModuleKind::FxStutter => {
            let (mut r, mut sl, mut m) = {
                let s = app.state.read();
                (s.fx.stutter_rate, s.fx.stutter_slice, s.fx.stutter_mix)
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.stutter_rate")),
                ("SLICE", &mut sl, pm("fx.stutter_slice"))
            );
            hk!(ui, ("MIX", &mut m, pm("fx.stutter_mix")));
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("stutter_xy_{module_id}"),
                    ["RATE", "SLICE", "MIX"],
                    &mut pad_pair,
                    (&mut r, &mut sl, &mut m),
                    [
                        user_owned("fx.stutter_rate"),
                        user_owned("fx.stutter_slice"),
                        user_owned("fx.stutter_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.stutter_rate {
                let mut s = app.state.write();
                s.fx.stutter_rate = r;
                s.fx.stutter_slice = sl;
                s.fx.stutter_mix = m;
            }
        }
        ModuleKind::FxFreeze => {
            let mut m = app.state.read().fx.freeze_mix;
            hk!(ui, ("FREEZE", &mut m, pm("fx.freeze_mix")));
            if changed || m != app.state.read().fx.freeze_mix {
                app.state.write().fx.freeze_mix = m;
            }
        }
        ModuleKind::FxGate => {
            let (mut th, mut at, mut rl, mut dp, mut m) = {
                let s = app.state.read();
                (
                    s.fx.gate_threshold,
                    s.fx.gate_attack,
                    s.fx.gate_release,
                    s.fx.gate_depth,
                    s.fx.gate_mix,
                )
            };
            hk!(
                ui,
                ("THR", &mut th, pm("fx.gate_threshold")),
                ("ATK", &mut at, pm("fx.gate_attack"))
            );
            hk!(
                ui,
                ("REL", &mut rl, pm("fx.gate_release")),
                ("DEPTH", &mut dp, pm("fx.gate_depth"))
            );
            hk!(ui, ("MIX", &mut m, pm("fx.gate_mix")));
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("gate_xy_{module_id}"),
                    ["THR", "DEPTH", "MIX"],
                    &mut pad_pair,
                    (&mut th, &mut dp, &mut m),
                    [
                        user_owned("fx.gate_threshold"),
                        user_owned("fx.gate_depth"),
                        user_owned("fx.gate_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || th != app.state.read().fx.gate_threshold {
                let mut s = app.state.write();
                s.fx.gate_threshold = th;
                s.fx.gate_attack = at;
                s.fx.gate_release = rl;
                s.fx.gate_depth = dp;
                s.fx.gate_mix = m;
            }
        }
        ModuleKind::FxVocoder => {
            let (mut bd, mut cm, mut sn, mut m) = {
                let s = app.state.read();
                (
                    s.fx.vocoder_bands,
                    s.fx.vocoder_carrier_mix,
                    s.fx.vocoder_sense,
                    s.fx.vocoder_mix,
                )
            };
            hk!(
                ui,
                ("BANDS", &mut bd, pm("fx.vocoder_bands")),
                ("CRR.MX", &mut cm, pm("fx.vocoder_carrier_mix"))
            );
            hk!(
                ui,
                ("SENSE", &mut sn, pm("fx.vocoder_sense")),
                ("MIX", &mut m, pm("fx.vocoder_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("vocoder_xy_{module_id}"),
                    ["BANDS", "CARR", "MIX"],
                    &mut pad_pair,
                    (&mut bd, &mut cm, &mut m),
                    [
                        user_owned("fx.vocoder_bands"),
                        user_owned("fx.vocoder_carrier_mix"),
                        user_owned("fx.vocoder_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || bd != app.state.read().fx.vocoder_bands {
                let mut s = app.state.write();
                s.fx.vocoder_bands = bd;
                s.fx.vocoder_carrier_mix = cm;
                s.fx.vocoder_sense = sn;
                s.fx.vocoder_mix = m;
            }
        }
        _ => unreachable!("guarded by the early return above"),
    }
    Some(pad_pair)
}
