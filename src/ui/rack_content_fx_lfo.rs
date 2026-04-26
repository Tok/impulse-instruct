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
        ModuleKind::FxTremolo | ModuleKind::FxVibrato | ModuleKind::FxDeEsser => {}
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
        _ => unreachable!("guarded by the early return above"),
    }
    Some(pad_pair)
}
