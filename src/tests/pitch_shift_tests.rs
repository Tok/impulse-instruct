// ─── tests/pitch_shift_tests.rs ──────────────────────────────────────────────
// Standalone pitch-shift FX coverage.  The interesting test here is
// the ratio assertion — feeding a sine through +12 st must come out
// one octave up, measured by counting zero-crossings across a steady-
// state window of the wet signal.

use crate::audio::dsp::pitch_shift::PitchShift;
use crate::state::{AppState, ModuleKind, apply_llm_update};

// ─── Defaults + LLM apply ────────────────────────────────────────────────────

#[test]
fn pitch_shift_defaults_are_bypass() {
    let s = AppState::default();
    assert_eq!(s.fx.pitch_shift_semi, 0.0);
    assert_eq!(s.fx.pitch_shift_fine, 0.0);
    assert_eq!(s.fx.pitch_shift_mix, 0.0);
    assert_eq!(s.fx.pitch_shift_fbk, 0.0);
}

#[test]
fn apply_llm_update_writes_all_pitch_shift_params() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "fx": {
            "pitch_shift_semi": 7.0,
            "pitch_shift_fine": -15.0,
            "pitch_shift_mix":  0.8,
            "pitch_shift_fbk":  0.6,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!((s1.fx.pitch_shift_semi - 7.0).abs() < 1e-6);
    assert!((s1.fx.pitch_shift_fine - -15.0).abs() < 1e-6);
    assert!((s1.fx.pitch_shift_mix - 0.8).abs() < 1e-6);
    assert!((s1.fx.pitch_shift_fbk - 0.6).abs() < 1e-6);
}

#[test]
fn apply_llm_update_respects_locks_on_pitch_shift() {
    let mut s0 = AppState::default();
    s0.fx.pitch_shift_semi = 5.0;
    s0.llm
        .locked_params
        .insert("fx.pitch_shift_semi".to_string());
    let update = serde_json::json!({
        "fx": {
            "pitch_shift_semi": -3.0,
            "pitch_shift_mix":  0.7,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(
        (s1.fx.pitch_shift_semi - 5.0).abs() < 1e-6,
        "locked semi kept"
    );
    assert!(
        (s1.fx.pitch_shift_mix - 0.7).abs() < 1e-6,
        "unlocked mix applied"
    );
}

// ─── Module kind wiring ──────────────────────────────────────────────────────

#[test]
fn fx_pitch_shift_is_rackable_and_supports_xy_pad() {
    let k = ModuleKind::FxPitchShift;
    assert!(k.has_audio_output());
    assert!(k.supports_xy_pad());
    assert!(k.allows_multiple());
    assert_eq!(k.default_zone(), crate::state::Zone::FxMod);
    assert_eq!(k.label(), "PITCH");
    assert_eq!(
        crate::state::fx_plan::kind_to_fx_step(ModuleKind::FxPitchShift),
        Some(crate::state::FxStep::PitchShift),
    );
}

// ─── DSP behaviour ───────────────────────────────────────────────────────────

#[test]
fn pitch_shift_bypasses_when_semi_and_mix_are_zero() {
    // mix=0 short-circuits the wet path — output must match input
    // bit-for-bit so neutral ConvReverb patches don't colour the dry.
    let mut ps = PitchShift::new();
    let out = ps.process(0.5, 0.0, 0.0, 0.0, 0.0);
    assert!((out - 0.5).abs() < 1e-9);
}

#[test]
fn pitch_shift_bypasses_when_total_offset_is_zero_regardless_of_mix() {
    // semi=0 + cents=0 → ratio 1.0 — even with mix=1 the DSP takes
    // the bypass fast path, so callers that drop a PitchShift in
    // line with a non-shifting preset still hear the dry signal.
    let mut ps = PitchShift::new();
    // Feed a longish tail so the ring buffer is populated.
    for i in 0..2048 {
        let x = (i as f32 * 0.01).sin();
        let out = ps.process(x, 0.0, 0.0, 1.0, 0.0);
        assert!(
            (out - x).abs() < 1e-6,
            "identity shift at mix=1 should pass dry unchanged, got {out} vs {x}",
        );
    }
}

/// Count zero-crossings across `samples`.  Used to approximate the
/// fundamental frequency of a sinusoid: crossings/2 = cycles.
fn zero_crossings(samples: &[f32]) -> usize {
    let mut count = 0usize;
    for w in samples.windows(2) {
        let a = w[0];
        let b = w[1];
        // Skip zero-valued transitions to stop tiny sample drift from
        // registering as a crossing.
        if (a > 1e-6 && b < -1e-6) || (a < -1e-6 && b > 1e-6) {
            count += 1;
        }
    }
    count
}

#[test]
fn pitch_shift_plus_twelve_st_doubles_fundamental_frequency() {
    // A 1 kHz sine through +12 st must come out at ~2 kHz.  Count
    // zero-crossings over 4096 samples of steady-state output after
    // a settle window so grain envelopes have latched.
    let mut ps = PitchShift::new();
    let sr = 48_000.0_f32;
    let freq = 1_000.0_f32;
    let settle = 4096;
    let measure = 4096;
    let mut wet_out = Vec::with_capacity(measure);
    for i in 0..(settle + measure) {
        let t = i as f32 / sr;
        let x = (std::f32::consts::TAU * freq * t).sin();
        let y = ps.process(x, /*semi*/ 12.0, 0.0, /*mix*/ 1.0, 0.0);
        if i >= settle {
            wet_out.push(y);
        }
    }
    let crossings = zero_crossings(&wet_out);
    // Expected: 2 kHz × (measure / sr) cycles × 2 crossings/cycle.
    let expected = (2.0 * freq * measure as f32 / sr * 2.0) as usize;
    let diff = (crossings as isize - expected as isize).abs();
    // PSOLA + linear-interp read introduces grain transition noise
    // that can add or drop a handful of zero-crossings; 10 % is a
    // generous-but-tight window.
    assert!(
        diff < (expected / 10) as isize,
        "crossings {} far from doubled-freq target {} ({} diff)",
        crossings,
        expected,
        diff,
    );
}

#[test]
fn pitch_shift_minus_twelve_st_halves_fundamental_frequency() {
    // Symmetric downshift — 1 kHz sine through -12 st should come
    // out at ~500 Hz.
    let mut ps = PitchShift::new();
    let sr = 48_000.0_f32;
    let freq = 1_000.0_f32;
    let settle = 4096;
    let measure = 4096;
    let mut wet_out = Vec::with_capacity(measure);
    for i in 0..(settle + measure) {
        let t = i as f32 / sr;
        let x = (std::f32::consts::TAU * freq * t).sin();
        let y = ps.process(x, -12.0, 0.0, 1.0, 0.0);
        if i >= settle {
            wet_out.push(y);
        }
    }
    let crossings = zero_crossings(&wet_out);
    let expected = (0.5 * freq * measure as f32 / sr * 2.0) as usize;
    let diff = (crossings as isize - expected as isize).abs();
    assert!(
        diff < ((expected / 10) + 2) as isize,
        "crossings {} far from halved-freq target {} ({} diff)",
        crossings,
        expected,
        diff,
    );
}
