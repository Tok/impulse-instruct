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
        | ModuleKind::FxVocoder
        | ModuleKind::FxWiden
        | ModuleKind::FxFreqShift
        | ModuleKind::FxVinyl
        | ModuleKind::FxDjFilter
        | ModuleKind::FxIsoEq => {}
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
            // Glass-grouped 4-knob bank.  THRESHOLD + CEILING are
            // the primary controls (φ-bigger); RELEASE + LOOKAHEAD
            // sit at the default (medium) size — they were
            // φ-smaller in V1 but read too cramped, so user
            // feedback bumped them back up.
            let big = ctrl.phi_bigger();
            let avail = ui.available_width();
            widgets::glass_group_fill(ui, avail, avail, |ui| {
                widgets::centered_row(ui, |ui| {
                    if widgets::param_control(
                        ui,
                        "THRESHOLD",
                        &mut th,
                        pm("fx.limiter_threshold"),
                        big,
                    )
                    .0
                    {
                        changed = true;
                    }
                    if widgets::param_control(ui, "CEILING", &mut ce, pm("fx.limiter_ceiling"), big)
                        .0
                    {
                        changed = true;
                    }
                });
                widgets::centered_row(ui, |ui| {
                    if widgets::param_control(
                        ui,
                        "RELEASE",
                        &mut rl,
                        pm("fx.limiter_release"),
                        ctrl,
                    )
                    .0
                    {
                        changed = true;
                    }
                    if widgets::param_control(
                        ui,
                        "LOOKAHEAD",
                        &mut la,
                        pm("fx.limiter_lookahead"),
                        ctrl,
                    )
                    .0
                    {
                        changed = true;
                    }
                });
            });
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
            // All knobs φ-bigger per user feedback — Multitap is a
            // performance / dub FX where every parameter is in
            // active play, so they share the same visual weight.
            let big = ctrl.phi_bigger();
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "TIME", &mut t, pm("fx.multitap_time"), big).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SPREAD", &mut sp, pm("fx.multitap_spread"), big).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "FEEDBACK", &mut fb, pm("fx.multitap_feedback"), big)
                    .0
                {
                    changed = true;
                }
                if widgets::param_control(ui, "MIX", &mut m, pm("fx.multitap_mix"), big).0 {
                    changed = true;
                }
            });
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
            // TIME + FEEDBACK are the character controls — the
            // delay's whole identity comes from how those two
            // interact, so both go φ-bigger.  MIX stays at the
            // default size as the wet/dry trim.
            let big = ctrl.phi_bigger();
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "TIME", &mut t, pm("fx.revdelay_time"), big).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "FEEDBACK", &mut fb, pm("fx.revdelay_feedback"), big)
                    .0
                {
                    changed = true;
                }
            });
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
            // All knobs φ-bigger — Stutter is hands-on performance
            // chrome where every parameter is grabbed in the moment.
            let big = ctrl.phi_bigger();
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "RATE", &mut r, pm("fx.stutter_rate"), big).0 {
                    changed = true;
                }
                if widgets::param_control(ui, "SLICE", &mut sl, pm("fx.stutter_slice"), big).0 {
                    changed = true;
                }
            });
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(ui, "MIX", &mut m, pm("fx.stutter_mix"), big).0 {
                    changed = true;
                }
            });
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
        ModuleKind::FxFreqShift => {
            let (mut a, mut f, mut m) = {
                let st = app.state.read();
                (
                    st.fx.freq_shift_amount,
                    st.fx.freq_shift_feedback,
                    st.fx.freq_shift_mix,
                )
            };
            // SHIFT + FEEDBACK glass-grouped, MIX at default
            // (medium) size beside it — single row so the card fits
            // 2×1.  MIX was φ-bigger in V1; user feedback dropped
            // it to medium because the primary visual emphasis
            // belongs to the SHIFT/FEEDBACK pair, not the wet/dry.
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                widgets::glass_group_fill(ui, avail * 0.7, avail * 0.7, |ui| {
                    widgets::centered_row(ui, |ui| {
                        if widgets::param_control(
                            ui,
                            "SHIFT",
                            &mut a,
                            pm("fx.freq_shift_amount"),
                            ctrl,
                        )
                        .0
                        {
                            changed = true;
                        }
                        if widgets::param_control(
                            ui,
                            "FEEDBACK",
                            &mut f,
                            pm("fx.freq_shift_feedback"),
                            ctrl,
                        )
                        .0
                        {
                            changed = true;
                        }
                    });
                });
                if widgets::param_control(ui, "MIX", &mut m, pm("fx.freq_shift_mix"), ctrl).0 {
                    changed = true;
                }
            });
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("freq_shift_xy_{module_id}"),
                    ["SHIFT", "FBK", "MIX"],
                    &mut pad_pair,
                    (&mut a, &mut f, &mut m),
                    [
                        user_owned("fx.freq_shift_amount"),
                        user_owned("fx.freq_shift_feedback"),
                        user_owned("fx.freq_shift_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || a != app.state.read().fx.freq_shift_amount {
                let mut st = app.state.write();
                st.fx.freq_shift_amount = a;
                st.fx.freq_shift_feedback = f;
                st.fx.freq_shift_mix = m;
            }
        }
        ModuleKind::FxVinyl => {
            // NOISE / WEAR / MIX (V1) + TRANSIENT (V2 follow-up).
            // The XY pad still hosts the original 3 knobs; transient
            // gets its own row to keep the V2 follow-up visually
            // distinct from the steady-state colour controls.
            let (mut n, mut w, mut m, mut t) = {
                let st = app.state.read();
                (
                    st.fx.vinyl_noise,
                    st.fx.vinyl_wear,
                    st.fx.vinyl_mix,
                    st.fx.vinyl_transient,
                )
            };
            hk!(
                ui,
                ("NOISE", &mut n, pm("fx.vinyl_noise")),
                ("WEAR", &mut w, pm("fx.vinyl_wear"))
            );
            hk!(
                ui,
                ("MIX", &mut m, pm("fx.vinyl_mix")),
                ("TRANSIENT", &mut t, pm("fx.vinyl_transient"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("vinyl_xy_{module_id}"),
                    ["NOISE", "WEAR", "MIX"],
                    &mut pad_pair,
                    (&mut n, &mut w, &mut m),
                    [
                        user_owned("fx.vinyl_noise"),
                        user_owned("fx.vinyl_wear"),
                        user_owned("fx.vinyl_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || n != app.state.read().fx.vinyl_noise {
                let mut st = app.state.write();
                st.fx.vinyl_noise = n;
                st.fx.vinyl_wear = w;
                st.fx.vinyl_mix = m;
                st.fx.vinyl_transient = t;
            }
        }
        ModuleKind::FxWiden => {
            let (mut h, mut s, mut m) = {
                let st = app.state.read();
                (st.fx.widen_haas, st.fx.widen_side, st.fx.widen_mix)
            };
            hk!(
                ui,
                ("HAAS", &mut h, pm("fx.widen_haas")),
                ("SIDE", &mut s, pm("fx.widen_side")),
                ("MIX", &mut m, pm("fx.widen_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("widen_xy_{module_id}"),
                    ["HAAS", "SIDE", "MIX"],
                    &mut pad_pair,
                    (&mut h, &mut s, &mut m),
                    [
                        user_owned("fx.widen_haas"),
                        user_owned("fx.widen_side"),
                        user_owned("fx.widen_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || h != app.state.read().fx.widen_haas {
                let mut st = app.state.write();
                st.fx.widen_haas = h;
                st.fx.widen_side = s;
                st.fx.widen_mix = m;
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
            // All four controls in one row so the card fits 2×1 like
            // the rest of the Tier-1 FX strip.
            hk!(
                ui,
                ("BANDS", &mut bd, pm("fx.vocoder_bands")),
                ("CARRIER", &mut cm, pm("fx.vocoder_carrier_mix")),
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
        ModuleKind::FxDjFilter => {
            // Single-knob morph FX — MORPH is the entire identity
            // of the card, so it's φ-bigger and glass-grouped on
            // its own; RESONANCE + MIX support roles get default
            // size beside it.  Bipolar feel comes from the morph
            // sweep itself (LP at 0, BP at 0.5, HP at 1) — no
            // detent label needed since the resonance peak
            // crossing the centre is the audible signpost.
            let (mut morph, mut res, mut mix) = {
                let st = app.state.read();
                (
                    st.fx.dj_filter_morph,
                    st.fx.dj_filter_resonance,
                    st.fx.dj_filter_mix,
                )
            };
            let big = ctrl.phi_bigger();
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                widgets::glass_group_fill(ui, avail * 0.5, avail * 0.5, |ui| {
                    widgets::centered_row(ui, |ui| {
                        if widgets::param_control(
                            ui,
                            "MORPH",
                            &mut morph,
                            pm("fx.dj_filter_morph"),
                            big,
                        )
                        .0
                        {
                            changed = true;
                        }
                    });
                });
                if widgets::param_control(
                    ui,
                    "RESONANCE",
                    &mut res,
                    pm("fx.dj_filter_resonance"),
                    ctrl,
                )
                .0
                {
                    changed = true;
                }
                if widgets::param_control(ui, "MIX", &mut mix, pm("fx.dj_filter_mix"), ctrl).0 {
                    changed = true;
                }
            });
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("dj_filter_xy_{module_id}"),
                    ["MORPH", "RES", "MIX"],
                    &mut pad_pair,
                    (&mut morph, &mut res, &mut mix),
                    [
                        user_owned("fx.dj_filter_morph"),
                        user_owned("fx.dj_filter_resonance"),
                        user_owned("fx.dj_filter_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || morph != app.state.read().fx.dj_filter_morph {
                let mut st = app.state.write();
                st.fx.dj_filter_morph = morph;
                st.fx.dj_filter_resonance = res;
                st.fx.dj_filter_mix = mix;
            }
        }
        ModuleKind::FxIsoEq => {
            // LOW / MID / HIGH / MIX — DJ-style 4-knob layout.
            // Defaults park each band at unity so engaging the
            // FX (mix > 0) is a no-op until the user starts
            // killing bands.
            let (mut l, mut m, mut h, mut mx) = {
                let st = app.state.read();
                (st.fx.iso_low, st.fx.iso_mid, st.fx.iso_high, st.fx.iso_mix)
            };
            hk!(
                ui,
                ("LOW", &mut l, pm("fx.iso_low")),
                ("MID", &mut m, pm("fx.iso_mid")),
                ("HIGH", &mut h, pm("fx.iso_high")),
                ("MIX", &mut mx, pm("fx.iso_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("iso_eq_xy_{module_id}"),
                    ["LOW", "MID", "HIGH"],
                    &mut pad_pair,
                    (&mut l, &mut m, &mut h),
                    [
                        user_owned("fx.iso_low"),
                        user_owned("fx.iso_mid"),
                        user_owned("fx.iso_high"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || l != app.state.read().fx.iso_low {
                let mut st = app.state.write();
                st.fx.iso_low = l;
                st.fx.iso_mid = m;
                st.fx.iso_high = h;
                st.fx.iso_mix = mx;
            }
        }
        _ => unreachable!("guarded by the early return above"),
    }
    Some(pad_pair)
}
