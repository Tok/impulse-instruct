// ─── tests/param_eq_tests.rs ─────────────────────────────────────────────────
// 8-band parametric EQ coverage.  The biquad coefficient math is the
// single point of failure for both the DSP cascade and the UI curve
// renderer (both call `band_magnitude` / `cascade_db`), so these
// tests lock the peak-gain-at-centre / unity-elsewhere contract.

use crate::audio::dsp::db_to_lin;
use crate::audio::dsp::param_eq::{ParamEq, band_magnitude, cascade_db};
use crate::state::{AppState, ModuleKind, ParamEqBand, ParamEqBandKind, apply_llm_update};

fn make_peak(freq_hz: f32, gain_db: f32, q: f32) -> ParamEqBand {
    ParamEqBand {
        kind: ParamEqBandKind::Peak,
        freq_hz,
        gain_db,
        q,
        enabled: true,
    }
}

// ─── FxState defaults ────────────────────────────────────────────────────────

#[test]
fn param_eq_default_bands_are_unity_gain() {
    // 8 bands at 0 dB must cascade to 0 dB — adding a ParamEq module
    // to the rack must not colour the bus until the user moves a knob.
    let s = AppState::default();
    for freq in [50.0, 200.0, 800.0, 3_200.0, 12_800.0] {
        let db = cascade_db(&s.fx.param_eq_bands, 48_000.0, freq);
        assert!(
            db.abs() < 0.01,
            "default cascade at {freq} Hz should be ~0 dB, got {db}",
        );
    }
}

#[test]
fn param_eq_default_band_layout_matches_spec() {
    // 8 bands: shelf / 6 peaks / shelf, 100 Hz to 15 kHz.  This fixes
    // the default layout so UI/tests don't drift from the plan.
    let s = AppState::default();
    let b = &s.fx.param_eq_bands;
    assert_eq!(b[0].kind, ParamEqBandKind::LowShelf);
    assert!((b[0].freq_hz - 100.0).abs() < 0.001);
    assert_eq!(b[7].kind, ParamEqBandKind::HighShelf);
    assert!((b[7].freq_hz - 15_000.0).abs() < 0.001);
    for band in &b[1..=6] {
        assert_eq!(band.kind, ParamEqBandKind::Peak);
    }
}

// ─── Biquad coefficient correctness ──────────────────────────────────────────

#[test]
fn peak_filter_gain_at_centre_matches_gain_db() {
    // RBJ peak filter at f0 with Q=1.0 and gain=+6 dB must produce
    // +6 dB at f0 exactly.  Numerical slack: 0.1 dB covers float
    // round-trip through cos/sin/exp in the coef calc.
    let band = make_peak(1_000.0, 6.0, 1.0);
    let mag = band_magnitude(&band, 48_000.0, 1_000.0);
    let db = 20.0 * mag.log10();
    assert!(
        (db - 6.0).abs() < 0.1,
        "peak gain at f0 should be 6 dB, got {db}"
    );
}

#[test]
fn peak_filter_gain_far_from_centre_is_near_zero() {
    // The same +6 dB peak at 1 kHz must leave DC and Nyquist
    // essentially unaffected (< 0.1 dB deviation with Q=1.0).
    let band = make_peak(1_000.0, 6.0, 1.0);
    let dc_db = 20.0 * band_magnitude(&band, 48_000.0, 20.0).log10();
    let nyq_db = 20.0 * band_magnitude(&band, 48_000.0, 22_000.0).log10();
    assert!(
        dc_db.abs() < 0.1,
        "peak at 1 kHz should leave DC clean, got {dc_db}"
    );
    assert!(
        nyq_db.abs() < 0.1,
        "peak at 1 kHz should leave Nyquist clean, got {nyq_db}"
    );
}

#[test]
fn disabled_band_contributes_zero_db() {
    // A band with `enabled: false` must be a pass-through regardless
    // of stored freq/gain/Q — so a big gain stored for A/B doesn't
    // leak into the sound while the band is bypassed.
    let mut band = make_peak(1_000.0, 12.0, 1.0);
    band.enabled = false;
    let mag = band_magnitude(&band, 48_000.0, 1_000.0);
    assert!((mag - 1.0).abs() < 1e-6);
}

#[test]
fn zero_gain_band_contributes_zero_db_regardless_of_kind() {
    // Even a shelf at the user's picked freq should be transparent
    // when gain_db is 0, so moving a node horizontally with the gain
    // parked at 0 doesn't ghost the signal.
    for kind in [
        ParamEqBandKind::LowShelf,
        ParamEqBandKind::Peak,
        ParamEqBandKind::HighShelf,
    ] {
        let band = ParamEqBand {
            kind,
            freq_hz: 500.0,
            gain_db: 0.0,
            q: 1.0,
            enabled: true,
        };
        let mag = band_magnitude(&band, 48_000.0, 500.0);
        assert!(
            (mag - 1.0).abs() < 1e-6,
            "{kind:?} at 0 dB should be unity, got mag {mag}",
        );
    }
}

// ─── Cascade DSP — sample-level sanity ───────────────────────────────────────

#[test]
fn param_eq_cascade_is_pass_through_with_default_bands() {
    // At default (all-zero-gain) bands the per-sample cascade must
    // approximate the identity.  Initial transient from unloaded
    // biquad state is bounded; settle first by driving a long
    // zero-padded tail.
    let mut eq = ParamEq::new();
    let bands = crate::state::default_param_eq_bands();
    // Settle 512 zero samples so Biquad::z1/z2 are at steady state.
    for _ in 0..512 {
        eq.process(0.0, &bands, 48_000.0);
    }
    // A non-zero impulse should come back unchanged (within
    // single-precision float drift from zero-multiplies).
    let out = eq.process(1.0, &bands, 48_000.0);
    assert!(
        (out - 1.0).abs() < 1e-6,
        "unity cascade should preserve the impulse, got {out}",
    );
}

#[test]
fn param_eq_cascade_boosts_when_band_gain_is_positive() {
    // Drive a 1 kHz sine through a +12 dB peak @ 1 kHz — RMS of the
    // output over a few cycles should be ~4× the input (12 dB =
    // voltage factor 10^(12/20) ≈ 3.98).
    let mut eq = ParamEq::new();
    let mut bands = crate::state::default_param_eq_bands();
    // Overwrite band 3 (default peak @ 1 kHz) with a +12 dB boost.
    bands[3].gain_db = 12.0;

    let sr = 48_000.0_f32;
    let freq = 1_000.0_f32;
    // Settle enough samples for the filter to reach steady state.
    let settle = (sr * 0.05) as usize; // 50 ms
    let measure = (sr / freq * 4.0) as usize; // 4 cycles
    let mut in_sum2 = 0.0_f32;
    let mut out_sum2 = 0.0_f32;
    for i in 0..(settle + measure) {
        let t = i as f32 / sr;
        let x = (std::f32::consts::TAU * freq * t).sin();
        let y = eq.process(x, &bands, sr);
        if i >= settle {
            in_sum2 += x * x;
            out_sum2 += y * y;
        }
    }
    let in_rms = (in_sum2 / measure as f32).sqrt();
    let out_rms = (out_sum2 / measure as f32).sqrt();
    let ratio = out_rms / in_rms;
    let expected = db_to_lin(12.0);
    assert!(
        (ratio - expected).abs() / expected < 0.1,
        "expected ~4× RMS boost, got {ratio:.3} (target {expected:.3})",
    );
}

// ─── LLM apply path ──────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_writes_param_eq_band_fields() {
    // `fx.param_eq_bands[N]` updates a single band; siblings remain
    // at their defaults.
    let s0 = AppState::default();
    let update = serde_json::json!({
        "fx": {
            "param_eq_bands": [
                null, null, null,
                { "kind": 1, "freq": 1200.0, "gain": -3.5, "q": 2.0, "enabled": true },
                null, null, null, null
            ]
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    let b = &s1.fx.param_eq_bands[3];
    assert_eq!(b.kind, ParamEqBandKind::Peak);
    assert!((b.freq_hz - 1200.0).abs() < 1e-3);
    assert!((b.gain_db - -3.5).abs() < 1e-6);
    assert!((b.q - 2.0).abs() < 1e-6);
    assert!(b.enabled);

    // Siblings untouched.
    let b0 = &s1.fx.param_eq_bands[0];
    let defaults = crate::state::default_param_eq_bands();
    assert_eq!(b0.kind, defaults[0].kind);
    assert!((b0.freq_hz - defaults[0].freq_hz).abs() < 1e-6);
}

#[test]
fn apply_llm_update_respects_per_band_lock() {
    // A UI touch on band-3 gain adds `fx.param_eq_bands.3.gain` to
    // locked_params; subsequent LLM writes that field must be ignored
    // while other fields of the same band still apply.
    let mut s0 = AppState::default();
    s0.fx.param_eq_bands[3].gain_db = 9.0;
    s0.llm
        .locked_params
        .insert("fx.param_eq_bands.3.gain".to_string());
    let update = serde_json::json!({
        "fx": {
            "param_eq_bands": [
                null, null, null,
                { "gain": -3.0, "q": 4.0 },
                null, null, null, null
            ]
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    assert!(
        (s1.fx.param_eq_bands[3].gain_db - 9.0).abs() < 1e-6,
        "locked gain must stay at 9.0, got {}",
        s1.fx.param_eq_bands[3].gain_db,
    );
    assert!(
        (s1.fx.param_eq_bands[3].q - 4.0).abs() < 1e-6,
        "unlocked q must follow update, got {}",
        s1.fx.param_eq_bands[3].q,
    );
}

// ─── Module kind wiring ──────────────────────────────────────────────────────

#[test]
fn fx_param_eq_is_rackable_and_does_not_expose_xy_pad() {
    // The curve editor replaces the XY pad, so supports_xy_pad must
    // return false — otherwise the chevron/pad expansion path would
    // try to render on top of the curve.
    let k = ModuleKind::FxParamEq;
    assert!(k.has_audio_output());
    assert!(!k.supports_xy_pad());
    assert!(k.allows_multiple());
    assert_eq!(k.default_zone(), crate::state::Zone::FxMod);
    assert_eq!(k.label(), "PARAM EQ");
    assert_eq!(
        crate::state::fx_plan::kind_to_fx_step(ModuleKind::FxParamEq),
        Some(crate::state::FxStep::ParamEq),
    );
}
