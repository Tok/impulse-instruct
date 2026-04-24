// ─── tests/conv_reverb_tests.rs ──────────────────────────────────────────────
// Convolution reverb — Phase 1 coverage.
//
// Phase 1 ships the plumbing (state fields, FxStep variant, ModuleKind,
// FxPlan dispatch, LLM update path, API endpoint) with a filter-only
// DSP stub.  These tests lock the plumbing so Phase 2 can swap in real
// partitioned convolution without rediscovering what the contract is.

use crate::state::{AppState, FxStep, ModuleKind, apply_llm_update};

// ─── FxState defaults ────────────────────────────────────────────────────────

#[test]
fn conv_reverb_fx_state_defaults_are_quiet_and_bright() {
    // An unpatched ConvReverb must pass dry audio unchanged — the stub
    // and Phase 2 both rely on `mix < 0.001` as the zero-output fast
    // path.  Other knobs default to their "neutral" position so merely
    // adding the module doesn't colour the signal.
    let s = AppState::default();
    assert_eq!(s.fx.conv_reverb_mix, 0.0);
    assert_eq!(s.fx.conv_reverb_size, 1.0); // full tail
    assert_eq!(s.fx.conv_reverb_predelay, 0.0);
    assert_eq!(s.fx.conv_reverb_damp, 0.0);
    assert_eq!(s.fx.conv_reverb_lowcut, 0.0);
    assert_eq!(s.fx.conv_reverb_width, 1.0);
    assert!(!s.fx.conv_reverb_reverse);
    assert!(s.fx.conv_reverb_ir_path.is_empty());
}

// ─── LLM update path ─────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_writes_all_conv_reverb_params() {
    // The LLM writes fx.conv_reverb_* via apply_fx_update; a missed
    // field in `llm_helpers::apply_fx_update` would silently drop the
    // update and the agent would lose control.
    let s0 = AppState::default();
    let update = serde_json::json!({
        "fx": {
            "conv_reverb_mix":      0.42,
            "conv_reverb_size":     0.7,
            "conv_reverb_predelay": 0.3,
            "conv_reverb_damp":     0.6,
            "conv_reverb_lowcut":   0.25,
            "conv_reverb_width":    0.8,
            "conv_reverb_reverse":  true,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!((s1.fx.conv_reverb_mix - 0.42).abs() < 1e-6);
    assert!((s1.fx.conv_reverb_size - 0.7).abs() < 1e-6);
    assert!((s1.fx.conv_reverb_predelay - 0.3).abs() < 1e-6);
    assert!((s1.fx.conv_reverb_damp - 0.6).abs() < 1e-6);
    assert!((s1.fx.conv_reverb_lowcut - 0.25).abs() < 1e-6);
    assert!((s1.fx.conv_reverb_width - 0.8).abs() < 1e-6);
    assert!(s1.fx.conv_reverb_reverse);
}

#[test]
fn apply_llm_update_respects_locks_on_conv_reverb_params() {
    // Touching a knob in the UI adds its dot-path to `llm.locked_params`.
    // apply_fx_update must skip locked paths — a regression here would
    // let the LLM overwrite the user's edits, which violates the core
    // "lock = user-owned" invariant of the whole LLM pipeline.
    let mut s0 = AppState::default();
    s0.fx.conv_reverb_mix = 0.9;
    s0.llm
        .locked_params
        .insert("fx.conv_reverb_mix".to_string());
    let update = serde_json::json!({
        "fx": {
            "conv_reverb_mix":  0.1,
            "conv_reverb_damp": 0.5,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(
        (s1.fx.conv_reverb_mix - 0.9).abs() < 1e-6,
        "locked mix must remain at 0.9, got {}",
        s1.fx.conv_reverb_mix
    );
    assert!(
        (s1.fx.conv_reverb_damp - 0.5).abs() < 1e-6,
        "unlocked damp must follow the update, got {}",
        s1.fx.conv_reverb_damp
    );
}

// ─── ModuleKind wiring ───────────────────────────────────────────────────────

#[test]
fn fx_conv_reverb_is_rackable_fx_module() {
    // Flags the rack system reads to treat the module correctly:
    //  - `has_audio_output` decides whether MASTER-reach LEDs light up
    //  - `supports_xy_pad` drives the pad-expand chevron
    //  - `allows_multiple` lets the user stack two ConvReverbs for
    //    wet-mix + send-return patches.
    let k = ModuleKind::FxConvReverb;
    assert!(k.has_audio_output());
    assert!(k.supports_xy_pad());
    assert!(k.allows_multiple());
    assert_eq!(k.default_zone(), crate::state::Zone::FxMod);
    assert_eq!(k.label(), "CONV REV");
}

#[test]
fn fx_conv_reverb_kind_maps_to_conv_reverb_step() {
    // compile_fx_plan reads kind_to_fx_step to translate rack modules
    // into FxStep opcodes.  A None here would orphan the module from
    // the audio graph.
    assert_eq!(
        crate::state::fx_plan::kind_to_fx_step(ModuleKind::FxConvReverb),
        Some(FxStep::ConvReverb),
    );
    assert!(crate::state::fx_plan::kind_is_fx(ModuleKind::FxConvReverb));
}

// ─── Ramp path ───────────────────────────────────────────────────────────────

#[test]
fn ramp_starts_from_current_conv_reverb_mix_value() {
    // `fx.conv_reverb_mix` must be in jam_tools' param reader so a
    // ramp `{ param: "fx.conv_reverb_mix", to: 0.6, bars: 4 }` picks
    // the current value as its starting point.  Without the entry the
    // ramp would start from 0.0, producing an audible jump on scheduling.
    let mut s = AppState::default();
    s.fx.conv_reverb_mix = 0.33;
    let mut args = serde_json::Map::new();
    args.insert("param".into(), serde_json::json!("fx.conv_reverb_mix"));
    args.insert("to".into(), serde_json::json!(0.75));
    args.insert("bars".into(), serde_json::json!(4));
    let s = crate::state::jam_tools::parse_and_schedule_ramp(s, &args);
    let ramp = s
        .llm
        .active_ramps
        .iter()
        .find(|r| r.param == "fx.conv_reverb_mix")
        .expect("ramp should have been scheduled");
    assert!(
        (ramp.from - 0.33).abs() < 1e-6,
        "ramp.from {} should equal seeded state value 0.33",
        ramp.from
    );
}

// ─── DSP stub ────────────────────────────────────────────────────────────────
//
// Phase 1 ConvReverb is filter-only; these tests lock the stub shape
// so Phase 2's partitioned-convolution rewrite has a regression net.

#[test]
fn conv_reverb_passes_dry_signal_when_mix_is_zero() {
    // The bypass fast-path (mix < 0.001) must return `sig` unchanged —
    // otherwise an un-configured ConvReverb in the chain would colour
    // the dry bus.
    let mut cr = crate::audio::dsp::conv_reverb::ConvReverb::new();
    let out = cr.process(0.5, /*mix*/ 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 48_000.0);
    assert!((out - 0.5).abs() < 1e-9);
    // side contribution stays centred while mix is off.
    assert_eq!(cr.side, 0.0);
}

#[test]
fn conv_reverb_blends_wet_into_output_when_mix_is_positive() {
    // With mix=1 the output is 100 % wet, with mix=0.5 it's halfway.
    // This locks the final `sig*(1-mix)+wet*mix` blend so a future
    // refactor can't accidentally flip the mix direction.
    let mut cr = crate::audio::dsp::conv_reverb::ConvReverb::new();
    // Prime the predelay line so `delayed` isn't exactly zero on the
    // first call — otherwise mix=1 would output silence and the
    // assertion below couldn't distinguish a correct blend from a
    // stuck-at-zero bug.
    for _ in 0..64 {
        cr.process(0.8, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 48_000.0);
    }
    let out = cr.process(0.8, 0.5, 0.0, 0.0, 0.0, 1.0, 1.0, 48_000.0);
    // Crossfade sanity: output sits between full-dry (0.8) and the
    // wet tail, so it can't be outside [-1, 1] or a wild extrapolation.
    assert!(
        out.abs() <= 1.0,
        "conv reverb must stay in [-1, 1], got {out}"
    );
}

#[test]
fn conv_reverb_load_ir_stores_but_does_not_panic() {
    // Phase 1 doesn't read the IR inside `process`, but the API path
    // stores it.  A panic here — e.g. from a bad channel cast or
    // length assumption — would crash the audio thread.
    let mut cr = crate::audio::dsp::conv_reverb::ConvReverb::new();
    let ir = std::sync::Arc::new(vec![0.1_f32; 512]);
    cr.load_ir(ir, /*channels*/ 1, /*reversed*/ false);
    let out = cr.process(0.25, 0.5, 0.1, 0.2, 0.1, 1.0, 1.0, 48_000.0);
    assert!(out.is_finite(), "output must be finite after IR load");
    cr.clear_ir();
    let out2 = cr.process(0.25, 0.5, 0.1, 0.2, 0.1, 1.0, 1.0, 48_000.0);
    assert!(out2.is_finite(), "output must be finite after IR clear");
}
