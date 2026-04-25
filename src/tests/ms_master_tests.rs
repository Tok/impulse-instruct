// ─── tests/ms_master_tests.rs ────────────────────────────────────────────────
// Mid/side master-bus coverage.  Locks the "flat defaults are
// transparent" invariant the master stage relies on, plus the
// correctness of the gain / tilt / saturation mapping.

use crate::audio::dsp::ms_master::{MsMaster, MsMasterParams, soft_clip};
use crate::state::{AppState, apply_llm_update};

// ─── Defaults ────────────────────────────────────────────────────────────────

#[test]
fn fx_state_defaults_leave_mid_side_transparent() {
    let s = AppState::default();
    assert_eq!(s.fx.ms_mid_gain, 0.5);
    assert_eq!(s.fx.ms_mid_tilt, 0.5);
    assert_eq!(s.fx.ms_mid_sat, 0.0);
    assert_eq!(s.fx.ms_side_gain, 0.5);
    assert_eq!(s.fx.ms_side_tilt, 0.5);
    assert_eq!(s.fx.ms_side_sat, 0.0);
}

#[test]
fn ms_master_passes_mid_side_unchanged_at_flat_defaults() {
    // At unity gain, flat tilt, no saturation every biquad takes
    // its 0 dB fast-bypass branch.  Output must track input exactly
    // so dropping the module in line doesn't ghost the master bus.
    let mut ms = MsMaster::new();
    let (mid, side) = ms.process(0.6, -0.25, 48_000.0, MsMasterParams::flat());
    assert!((mid - 0.6).abs() < 1e-6);
    assert!((side - -0.25).abs() < 1e-6);
}

// ─── Gain mapping ────────────────────────────────────────────────────────────

#[test]
fn ms_mid_gain_maxed_approximately_quadruples_mid_level() {
    // Gain knob 1.0 → +12 dB → voltage factor ≈ 3.98.  Side left
    // untouched so the test is isolated.
    let mut ms = MsMaster::new();
    let mut p = MsMasterParams::flat();
    p.mid_gain = 1.0;
    let (mid_out, side_out) = ms.process(0.1, 0.1, 48_000.0, p);
    let ratio = mid_out / 0.1;
    assert!(
        (ratio - 3.98).abs() < 0.1,
        "mid +12 dB should give ≈3.98× factor, got {ratio}",
    );
    // Side stays unchanged at flat defaults.
    assert!((side_out - 0.1).abs() < 1e-6);
}

#[test]
fn ms_side_gain_zero_halves_and_more_collapses_toward_mono() {
    // side_gain knob at 0.0 → -12 dB → 0.25× — pulls the side
    // channel down so the L/R recombination collapses toward mono.
    let mut ms = MsMaster::new();
    let mut p = MsMasterParams::flat();
    p.side_gain = 0.0;
    let (_, side_out) = ms.process(0.0, 0.4, 48_000.0, p);
    let ratio = side_out / 0.4;
    assert!(
        (ratio - 0.25).abs() < 0.02,
        "side -12 dB should give ≈0.25× factor, got {ratio}",
    );
}

// ─── Saturation ──────────────────────────────────────────────────────────────

#[test]
fn soft_clip_is_bypass_at_sat_zero() {
    // Zero drive takes the no-arctan branch so the master path
    // stays bit-identical until the user cranks saturation.
    for &x in &[-0.9_f32, -0.3, 0.0, 0.3, 0.9] {
        assert!((soft_clip(x, 0.0) - x).abs() < 1e-9);
    }
}

#[test]
fn soft_clip_boosts_mid_range_at_high_sat() {
    // Normalised arctan keeps full-scale anchored at ±1 (so master
    // peaks don't get smaller than the input ceiling), but the curve
    // bulges so mid-range values get pushed up — that's the audible
    // "saturation" character the knob is for.  At x = 0.5, sat = 1.0
    // the output lifts substantially.
    let dry = 0.5_f32;
    let saturated = soft_clip(dry, 1.0);
    assert!(
        saturated > dry + 0.2,
        "sat=1.0 should noticeably lift mid-range input ({dry} → {saturated})",
    );
    // Signed symmetric: negative input should lift by the same
    // magnitude toward -1.
    let saturated_neg = soft_clip(-dry, 1.0);
    assert!((saturated + saturated_neg).abs() < 1e-6);
}

#[test]
fn soft_clip_keeps_output_bounded_above_full_scale_input() {
    // Peak-limiting check — an over-driven input (|x| > 1) must
    // clamp inside ±1 at high sat.  Without this the downstream
    // `clamp(-1, 1)` would hard-clip the wet signal and colour the
    // saturator's output with square-wave artefacts.
    let limited = soft_clip(3.0, 1.0);
    assert!(
        limited <= 1.0001,
        "over-driven input should land inside ±1, got {limited}",
    );
    assert!(
        limited > 0.9,
        "at high sat the limiter should hit near ±1, got {limited}"
    );
}

// ─── Tilt EQ tonal shift ─────────────────────────────────────────────────────

#[test]
fn ms_mid_tilt_toward_treble_brightens_the_mid() {
    // Feed a 6 kHz sine through — at tilt=1.0 (treble-heavy) the
    // high shelf should boost; at tilt=0.0 (bass-heavy) it should
    // cut.  Measuring RMS ratio is more robust than peak amplitude.
    let sr = 48_000.0_f32;
    let freq = 6_000.0_f32;
    let settle = (sr * 0.1) as usize;
    let measure = (sr / freq * 8.0) as usize;
    let sum_rms = |tilt: f32| -> f32 {
        let mut ms = MsMaster::new();
        let mut p = MsMasterParams::flat();
        p.mid_tilt = tilt;
        let mut sum2 = 0.0_f32;
        for i in 0..(settle + measure) {
            let t = i as f32 / sr;
            let x = (std::f32::consts::TAU * freq * t).sin();
            let (m, _) = ms.process(x, 0.0, sr, p);
            if i >= settle {
                sum2 += m * m;
            }
        }
        (sum2 / measure as f32).sqrt()
    };
    let bright = sum_rms(1.0);
    let dark = sum_rms(0.0);
    assert!(
        bright > dark * 1.5,
        "tilt=1 should be notably louder at 6 kHz than tilt=0 (bright {bright}, dark {dark})",
    );
}

// ─── LLM apply ───────────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_writes_all_ms_master_params() {
    let s0 = AppState::default();
    let update = serde_json::json!({
        "fx": {
            "ms_mid_gain":  0.8,
            "ms_mid_tilt":  0.3,
            "ms_mid_sat":   0.4,
            "ms_side_gain": 0.65,
            "ms_side_tilt": 0.7,
            "ms_side_sat":  0.2,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!((s1.fx.ms_mid_gain - 0.8).abs() < 1e-6);
    assert!((s1.fx.ms_mid_tilt - 0.3).abs() < 1e-6);
    assert!((s1.fx.ms_mid_sat - 0.4).abs() < 1e-6);
    assert!((s1.fx.ms_side_gain - 0.65).abs() < 1e-6);
    assert!((s1.fx.ms_side_tilt - 0.7).abs() < 1e-6);
    assert!((s1.fx.ms_side_sat - 0.2).abs() < 1e-6);
}

#[test]
fn apply_llm_update_respects_lock_on_ms_mid_gain() {
    let mut s0 = AppState::default();
    s0.fx.ms_mid_gain = 0.85;
    s0.llm.locked_params.insert("fx.ms_mid_gain".to_string());
    let update = serde_json::json!({
        "fx": {
            "ms_mid_gain":  0.2,
            "ms_side_gain": 0.3,
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(
        (s1.fx.ms_mid_gain - 0.85).abs() < 1e-6,
        "locked mid_gain kept"
    );
    assert!(
        (s1.fx.ms_side_gain - 0.3).abs() < 1e-6,
        "unlocked side_gain applied"
    );
}
