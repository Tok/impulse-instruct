// ─── ui/rack_content_fx_lfo.rs ───────────────────────────────────────────────
// Per-card render functions for FX cards that didn't fit in
// `rack_content_fx_extras.rs` (which ran into the 1000-line cap
// during the LFO-modulation cluster ship).  Originally for
// Tremolo + Vibrato; now also hosts De-esser since the same
// 4-knob shape applies.  Same `pm` / `user_owned` / `hk!`
// boilerplate as its siblings — the duplication is the cost of
// avoiding a fat `DrawCtx` struct that would have to thread
// `&mut app` and `&mut ui` through every helper.

use std::collections::HashSet;

use crate::state::ModuleKind;
use crate::ui::ImpulseApp;
use crate::ui::rack_content_pad::{PAD_SECTION_TOP_GAP, render_three_pad};
use crate::ui::widgets::{self, ControlPrefs};

/// Render an LFO-modulation FX card if `kind` is one of the
/// LFO-driven movement variants (Tremolo, Vibrato).  Returns
/// `Some(pad_pair)` on hit so the caller writes it back.
/// Returns `None` if `kind` isn't handled here.
pub(super) fn try_draw_fx_lfo_content(
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
        ModuleKind::FxTremolo
        | ModuleKind::FxVibrato
        | ModuleKind::FxDeEsser
        | ModuleKind::FxResBank
        | ModuleKind::FxTapeEcho
        | ModuleKind::FxMultibandComp
        | ModuleKind::FxGrainDelay => {}
        _ => return None,
    }

    let pm = |path: &str| crate::state::param_mode(path, locked, focused);
    let user_owned = |path: &str| matches!(pm(path), crate::state::ParamMode::UserOwned);
    let mut changed = false;
    let mut pad_pair = pad_pair_in;

    macro_rules! hk {
        ($ui:expr, $( ($label:expr, $val:expr, $pm:expr) ),+ $(,)?) => {
            widgets::centered_row($ui, |ui| {
                $(
                    if widgets::param_control(ui, $label, $val, $pm, ctrl).0 {
                        changed = true;
                    }
                )+
            });
        };
    }

    match kind {
        ModuleKind::FxTremolo => {
            // RATE / DEPTH / SHAPE / MIX in a single row.  Every
            // knob is unipolar and audibly immediate — no detent
            // labels needed; users dial by ear.
            let (mut r, mut d, mut sh, mut m) = {
                let st = app.state.read();
                (
                    st.fx.tremolo_rate,
                    st.fx.tremolo_depth,
                    st.fx.tremolo_shape,
                    st.fx.tremolo_mix,
                )
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.tremolo_rate")),
                ("DEPTH", &mut d, pm("fx.tremolo_depth")),
                ("SHAPE", &mut sh, pm("fx.tremolo_shape")),
                ("MIX", &mut m, pm("fx.tremolo_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("tremolo_xy_{module_id}"),
                    ["RATE", "DEPTH", "MIX"],
                    &mut pad_pair,
                    (&mut r, &mut d, &mut m),
                    [
                        user_owned("fx.tremolo_rate"),
                        user_owned("fx.tremolo_depth"),
                        user_owned("fx.tremolo_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.tremolo_rate {
                let mut st = app.state.write();
                st.fx.tremolo_rate = r;
                st.fx.tremolo_depth = d;
                st.fx.tremolo_shape = sh;
                st.fx.tremolo_mix = m;
            }
        }
        ModuleKind::FxVibrato => {
            // Same 4-knob layout as Tremolo so users get a
            // consistent visual story for the LFO-modulation
            // cluster.  Vibrato's depth knob tops out at ±5 ms
            // delay-time swing (≈ ±50 cents pitch at 5 Hz);
            // Tremolo's tops out at full silence/double.
            let (mut r, mut d, mut sh, mut m) = {
                let st = app.state.read();
                (
                    st.fx.vibrato_rate,
                    st.fx.vibrato_depth,
                    st.fx.vibrato_shape,
                    st.fx.vibrato_mix,
                )
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.vibrato_rate")),
                ("DEPTH", &mut d, pm("fx.vibrato_depth")),
                ("SHAPE", &mut sh, pm("fx.vibrato_shape")),
                ("MIX", &mut m, pm("fx.vibrato_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("vibrato_xy_{module_id}"),
                    ["RATE", "DEPTH", "MIX"],
                    &mut pad_pair,
                    (&mut r, &mut d, &mut m),
                    [
                        user_owned("fx.vibrato_rate"),
                        user_owned("fx.vibrato_depth"),
                        user_owned("fx.vibrato_mix"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.vibrato_rate {
                let mut st = app.state.write();
                st.fx.vibrato_rate = r;
                st.fx.vibrato_depth = d;
                st.fx.vibrato_shape = sh;
                st.fx.vibrato_mix = m;
            }
        }
        ModuleKind::FxDeEsser => {
            // FREQ / THRESHOLD / AMOUNT / MIX — same 4-knob 2×1
            // grid as the other compact FX.  Defaults engage the
            // ducker the moment the user dials mix > 0; below the
            // threshold the FX is transparent.
            let (mut f, mut t, mut a, mut m) = {
                let st = app.state.read();
                (
                    st.fx.deess_freq,
                    st.fx.deess_threshold,
                    st.fx.deess_amount,
                    st.fx.deess_mix,
                )
            };
            hk!(
                ui,
                ("FREQ", &mut f, pm("fx.deess_freq")),
                ("THRESH", &mut t, pm("fx.deess_threshold")),
                ("AMOUNT", &mut a, pm("fx.deess_amount")),
                ("MIX", &mut m, pm("fx.deess_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("deesser_xy_{module_id}"),
                    ["FREQ", "THRESH", "AMOUNT"],
                    &mut pad_pair,
                    (&mut f, &mut t, &mut a),
                    [
                        user_owned("fx.deess_freq"),
                        user_owned("fx.deess_threshold"),
                        user_owned("fx.deess_amount"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || f != app.state.read().fx.deess_freq {
                let mut st = app.state.write();
                st.fx.deess_freq = f;
                st.fx.deess_threshold = t;
                st.fx.deess_amount = a;
                st.fx.deess_mix = m;
            }
        }
        ModuleKind::FxResBank => {
            // ROOT / CHORD / RES / MIX — chord knob quantises into
            // 6 preset interval sets inside the DSP, so the user
            // hears discrete chord changes as they sweep across
            // the knob's range.
            let (mut r, mut c, mut q, mut m) = {
                let st = app.state.read();
                (
                    st.fx.resbank_root,
                    st.fx.resbank_chord,
                    st.fx.resbank_resonance,
                    st.fx.resbank_mix,
                )
            };
            hk!(
                ui,
                ("ROOT", &mut r, pm("fx.resbank_root")),
                ("CHORD", &mut c, pm("fx.resbank_chord")),
                ("RES", &mut q, pm("fx.resbank_resonance")),
                ("MIX", &mut m, pm("fx.resbank_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("resbank_xy_{module_id}"),
                    ["ROOT", "CHORD", "RES"],
                    &mut pad_pair,
                    (&mut r, &mut c, &mut q),
                    [
                        user_owned("fx.resbank_root"),
                        user_owned("fx.resbank_chord"),
                        user_owned("fx.resbank_resonance"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || r != app.state.read().fx.resbank_root {
                let mut st = app.state.write();
                st.fx.resbank_root = r;
                st.fx.resbank_chord = c;
                st.fx.resbank_resonance = q;
                st.fx.resbank_mix = m;
            }
        }
        ModuleKind::FxTapeEcho => {
            // TIME / FEEDBACK / AGE / MIX — the AGE knob folds
            // wow / flutter / saturation / HF rolloff together so
            // the user dials character with one gesture.  Distinct
            // surface from FxDelay's separate-knobs layout.
            let (mut t, mut fb, mut a, mut m) = {
                let st = app.state.read();
                (
                    st.fx.tape_echo_time,
                    st.fx.tape_echo_feedback,
                    st.fx.tape_echo_age,
                    st.fx.tape_echo_mix,
                )
            };
            hk!(
                ui,
                ("TIME", &mut t, pm("fx.tape_echo_time")),
                ("FEEDBACK", &mut fb, pm("fx.tape_echo_feedback")),
                ("AGE", &mut a, pm("fx.tape_echo_age")),
                ("MIX", &mut m, pm("fx.tape_echo_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("tape_echo_xy_{module_id}"),
                    ["TIME", "FEEDBACK", "AGE"],
                    &mut pad_pair,
                    (&mut t, &mut fb, &mut a),
                    [
                        user_owned("fx.tape_echo_time"),
                        user_owned("fx.tape_echo_feedback"),
                        user_owned("fx.tape_echo_age"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || t != app.state.read().fx.tape_echo_time {
                let mut st = app.state.write();
                st.fx.tape_echo_time = t;
                st.fx.tape_echo_feedback = fb;
                st.fx.tape_echo_age = a;
                st.fx.tape_echo_mix = m;
            }
        }
        ModuleKind::FxMultibandComp => {
            // LOW / MID / HIGH thresholds + MIX — fixed shared
            // ratio (≈4:1) inside the DSP keeps the surface to
            // four knobs.  Defaults are 1.0 for every threshold,
            // so engaging the FX with default knobs is no-op
            // until the user dials a band's threshold below the
            // signal peak.
            let (mut l, mut m, mut h, mut mx) = {
                let st = app.state.read();
                (
                    st.fx.mb_low_thresh,
                    st.fx.mb_mid_thresh,
                    st.fx.mb_high_thresh,
                    st.fx.mb_mix,
                )
            };
            hk!(
                ui,
                ("LOW", &mut l, pm("fx.mb_low_thresh")),
                ("MID", &mut m, pm("fx.mb_mid_thresh")),
                ("HIGH", &mut h, pm("fx.mb_high_thresh")),
                ("MIX", &mut mx, pm("fx.mb_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("mb_comp_xy_{module_id}"),
                    ["LOW", "MID", "HIGH"],
                    &mut pad_pair,
                    (&mut l, &mut m, &mut h),
                    [
                        user_owned("fx.mb_low_thresh"),
                        user_owned("fx.mb_mid_thresh"),
                        user_owned("fx.mb_high_thresh"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || l != app.state.read().fx.mb_low_thresh {
                let mut st = app.state.write();
                st.fx.mb_low_thresh = l;
                st.fx.mb_mid_thresh = m;
                st.fx.mb_high_thresh = h;
                st.fx.mb_mix = mx;
            }
        }
        ModuleKind::FxGrainDelay => {
            // DELAY / SIZE / SCATTER / MIX — same compact 4-knob
            // shape as the other delay-line FX.  At scatter=0 the
            // four grains lock to a unison chorus around the
            // baseline; cranking scatter scrambles their pitch
            // and position into a granular cloud.
            let (mut d, mut g, mut s, mut m) = {
                let st = app.state.read();
                (
                    st.fx.grain_delay,
                    st.fx.grain_size,
                    st.fx.grain_scatter,
                    st.fx.grain_mix,
                )
            };
            hk!(
                ui,
                ("DELAY", &mut d, pm("fx.grain_delay")),
                ("SIZE", &mut g, pm("fx.grain_size")),
                ("SCATTER", &mut s, pm("fx.grain_scatter")),
                ("MIX", &mut m, pm("fx.grain_mix"))
            );
            if pad_expanded {
                ui.add_space(PAD_SECTION_TOP_GAP);
                let (vc, _) = render_three_pad(
                    ui,
                    &format!("grain_delay_xy_{module_id}"),
                    ["DELAY", "SIZE", "SCATTER"],
                    &mut pad_pair,
                    (&mut d, &mut g, &mut s),
                    [
                        user_owned("fx.grain_delay"),
                        user_owned("fx.grain_size"),
                        user_owned("fx.grain_scatter"),
                    ],
                );
                if vc {
                    changed = true;
                }
            }
            if changed || d != app.state.read().fx.grain_delay {
                let mut st = app.state.write();
                st.fx.grain_delay = d;
                st.fx.grain_size = g;
                st.fx.grain_scatter = s;
                st.fx.grain_mix = m;
            }
        }
        _ => unreachable!("guarded by the early return above"),
    }
    Some(pad_pair)
}
