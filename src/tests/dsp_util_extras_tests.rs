// ─── tests/dsp_util_extras_tests.rs ──────────────────────────────────────────
// Covers three pure helpers that didn't have dedicated tests:
//   • `hz_to_midi` — inverse of `midi_to_hz`; roundtrips must land on the
//     same integer note within float noise.
//   • `lfo_target_to_u8` — stable opcode table.  Tests a representative
//     slice (None=0 sentinel, a handful of well-known mappings, unique
//     codes for adjacent variants) rather than every arm — the table is
//     72 entries and full coverage is churny.
//   • `kind_is_fx` — every FxXxx kind is FX; every voice / non-FX kind
//     is not.

use crate::audio::dsp::{
    AUDIBLE_HZ_MAX, AUDIBLE_HZ_MIN, NYQUIST_GUARD_FACTOR, db_to_lin, hz_to_midi, lin_to_db,
    midi_to_hz, midi_to_hz_f32, nyquist_guard, one_pole_coef, one_pole_lp_alpha,
};
use crate::state::LfoTarget;
use crate::state::fx_plan::kind_is_fx;
use crate::state::{ModuleKind, fx_plan};

// ─── hz_to_midi ─────────────────────────────────────────────────────────────

#[test]
fn hz_to_midi_is_inverse_of_midi_to_hz() {
    // Every integer MIDI note n in a reasonable range must round-trip:
    //   hz_to_midi(midi_to_hz(n)) ≈ n.
    for n in [12_u8, 36, 48, 60, 69, 72, 96, 108] {
        let hz = midi_to_hz(n);
        let back = hz_to_midi(hz);
        assert!(
            (back - n as f32).abs() < 1e-3,
            "roundtrip failed at MIDI {n}: hz={hz}, back={back}",
        );
    }
}

#[test]
fn hz_to_midi_a4_is_69() {
    // A4 = 440 Hz → MIDI 69 exactly (the reference pitch).
    assert!((hz_to_midi(440.0) - 69.0).abs() < 1e-4);
}

#[test]
fn hz_to_midi_octave_gap_is_twelve_semitones() {
    // Octave ratio 2× → exactly 12 semitones of MIDI distance.
    let lo = hz_to_midi(220.0);
    let hi = hz_to_midi(440.0);
    assert!((hi - lo - 12.0).abs() < 1e-4);
}

// ─── midi_to_hz_f32 ─────────────────────────────────────────────────────────

/// `midi_to_hz_f32(69.0)` is exactly the A4 reference (440 Hz) — no
/// integer→float rounding noise to mask a wrong constant.
#[test]
fn midi_to_hz_f32_at_a4_returns_440() {
    assert!((midi_to_hz_f32(69.0) - 440.0).abs() < 1e-4);
}

/// Quarter-tone between A4 (69) and Bb4 (70) lands at the geometric
/// mean of their frequencies — the property a fractional MIDI helper
/// is supposed to deliver.
#[test]
fn midi_to_hz_f32_quarter_tone_is_geometric_mean() {
    let a4 = midi_to_hz_f32(69.0);
    let bb4 = midi_to_hz_f32(70.0);
    let quarter = midi_to_hz_f32(69.5);
    let geom = (a4 * bb4).sqrt();
    assert!(
        (quarter - geom).abs() < 1e-3,
        "quarter-tone {quarter} should equal geometric mean {geom}"
    );
}

/// Composes with `hz_to_midi` for any fractional MIDI input — covers
/// the chord-bank / pitch-bend use case where the input isn't an
/// integer note.
#[test]
fn midi_to_hz_f32_round_trips_through_hz_to_midi() {
    for &m in &[24.5_f32, 48.25, 60.0, 72.7, 96.123] {
        let hz = midi_to_hz_f32(m);
        let back = hz_to_midi(hz);
        assert!(
            (back - m).abs() < 1e-3,
            "fractional MIDI {m} drifted on round trip (got {back})"
        );
    }
}

/// Integer-MIDI helper still produces the same answer as the
/// fractional primitive — guards against a future divergence between
/// `midi_to_hz(u8)` and `midi_to_hz_f32(f32)`.
#[test]
fn midi_to_hz_matches_midi_to_hz_f32_at_integer_notes() {
    for n in [0_u8, 24, 48, 60, 69, 72, 96, 127] {
        let a = midi_to_hz(n);
        let b = midi_to_hz_f32(n as f32);
        assert!(
            (a - b).abs() < 1e-4,
            "MIDI {n}: midi_to_hz={a}, midi_to_hz_f32={b}"
        );
    }
}

// ─── nyquist_guard ──────────────────────────────────────────────────────────

/// `nyquist_guard(sr)` returns 90 % of Nyquist (45 % of sr).  Pin the
/// factor so a future relaxation of the safety margin is a deliberate
/// constant change, not an accidental drift.
#[test]
fn nyquist_guard_at_engine_sample_rates() {
    for sr in [44_100.0_f32, 48_000.0, 96_000.0] {
        let got = nyquist_guard(sr);
        let want = sr * NYQUIST_GUARD_FACTOR;
        assert!((got - want).abs() < 1e-3);
        // Below true Nyquist by a healthy margin (≥ 10 % headroom).
        assert!(got < sr * 0.5);
        assert!(got > sr * 0.4);
    }
}

#[test]
fn nyquist_guard_factor_matches_legacy_literal() {
    // The 18 inline `sr * 0.45` sites were the empirical baseline;
    // the constant must reproduce that exactly, otherwise existing
    // filter clamps would shift their cutoffs.
    assert_eq!(NYQUIST_GUARD_FACTOR, 0.45);
}

// ─── one_pole_coef ──────────────────────────────────────────────────────────

/// Bit-identity with the inline formula every voice envelope used
/// before the helper existed — the refactor must not perturb any
/// envelope's coast curve.
#[test]
fn one_pole_coef_matches_inline_formula() {
    let sr = 48_000.0_f32;
    for &t in &[0.001_f32, 0.005, 0.05, 0.1, 0.5, 1.0, 2.0] {
        let inline = (-1.0_f32 / (t * sr)).exp();
        let helper = one_pole_coef(t, sr);
        assert_eq!(helper, inline, "drift at t={t}");
    }
}

/// Plug the helper into a one-pole approach loop and check it
/// reaches the target within the configured time-constant — the
/// envelope-stage handover is wired around exactly this property.
#[test]
fn one_pole_coef_reaches_target_within_time_constant() {
    let sr = 48_000.0_f32;
    let attack_s = 0.020_f32; // 20 ms attack
    let coef = one_pole_coef(attack_s, sr);
    let mut value = 0.0_f32;
    let target = 1.0_f32;
    for _ in 0..((attack_s * sr) as usize) {
        value = target + (value - target) * coef;
    }
    // After 1 time-constant, value should reach ~1 - 1/e ≈ 0.632.
    assert!(value > 0.6 && value < 0.7, "value at τ: {value}");
}

/// Larger time → coefficient closer to 1 (slower approach); smaller
/// time → coefficient closer to 0 (snap).  Pin the monotonicity so a
/// future formula refactor can't silently invert.
#[test]
fn one_pole_coef_is_monotone_in_time() {
    let sr = 48_000.0_f32;
    let mut prev = one_pole_coef(0.001, sr);
    for &t in &[0.005_f32, 0.05, 0.5, 5.0] {
        let c = one_pole_coef(t, sr);
        assert!(c > prev, "coef should grow with time (t={t}: {c} ≤ {prev})");
        prev = c;
    }
}

// ─── one_pole_lp_alpha ───────────────────────────────────────────────────────

/// Bit-identity with the inline `1 − exp(−2π · fc / sr)` formula
/// across a span of musical cutoffs.  Refactor must not perturb any
/// existing damping / smoothing curve.
#[test]
fn one_pole_lp_alpha_matches_inline_formula() {
    let sr = 48_000.0_f32;
    for &fc in &[1.0_f32, 20.0, 200.0, 2_000.0, 12_000.0] {
        let inline = 1.0_f32 - (-std::f32::consts::TAU * fc / sr).exp();
        let helper = one_pole_lp_alpha(fc, sr);
        assert_eq!(helper, inline, "drift at fc={fc}");
    }
}

/// Endpoints: at fc=0 the LP is fully closed (alpha→0, no input
/// passes); at fc≫sr/(2π) the LP is fully open (alpha→1, output
/// equals input).  The smoothing intuition has to hold.
#[test]
fn one_pole_lp_alpha_has_correct_endpoints() {
    let sr = 48_000.0_f32;
    let alpha_zero = one_pole_lp_alpha(0.0, sr);
    let alpha_huge = one_pole_lp_alpha(sr * 10.0, sr);
    assert!(alpha_zero.abs() < 1e-6, "alpha(0) should be ~0");
    assert!(
        (alpha_huge - 1.0).abs() < 1e-3,
        "alpha(huge fc) should approach 1 (got {alpha_huge})"
    );
}

/// At fc = `sr / (2π)` the alpha is exactly `1 − 1/e ≈ 0.632`
/// (the classic τ = 1 sample point).  Pin the closed-form value
/// so a wrong constant in the formula would surface immediately.
#[test]
fn one_pole_lp_alpha_at_unit_time_constant() {
    let sr = 48_000.0_f32;
    let fc = sr / std::f32::consts::TAU;
    let alpha = one_pole_lp_alpha(fc, sr);
    let expected = 1.0 - (1.0_f32 / std::f32::consts::E);
    assert!(
        (alpha - expected).abs() < 1e-5,
        "alpha at fc=sr/2π should equal 1−1/e (got {alpha}, want {expected})"
    );
}

// ─── db_to_lin ─────────────────────────────────────────────────────────────

/// 0 dB is the unity-gain reference.  The helper has to return
/// exactly 1.0 here — every gain stage in the codebase relies on
/// this.
#[test]
fn db_to_lin_zero_is_unity() {
    assert_eq!(db_to_lin(0.0), 1.0);
}

/// Classic gain landmarks: ±6 dB ≈ ±2× factor, ±20 dB = ±10×.
/// The 6 dB approximation is loose by ~0.2 % (true factor is 1.995…),
/// which is well within audible tolerance.
#[test]
fn db_to_lin_known_values() {
    assert!((db_to_lin(6.0) - 1.995_262).abs() < 1e-3);
    assert!((db_to_lin(-6.0) - 0.501_187).abs() < 1e-3);
    assert!((db_to_lin(20.0) - 10.0).abs() < 1e-3);
    assert!((db_to_lin(-20.0) - 0.1).abs() < 1e-4);
}

/// `db_to_lin(-x)` is the multiplicative inverse of `db_to_lin(x)` —
/// pulling 6 dB down then 6 dB up should land at unity.  Pin this
/// inverse property so the formula's polarity can't silently flip.
#[test]
fn db_to_lin_negation_is_reciprocal() {
    for &db in &[1.0_f32, 6.0, 12.0, 24.0, 48.0] {
        let up = db_to_lin(db);
        let down = db_to_lin(-db);
        assert!(
            (up * down - 1.0).abs() < 1e-4,
            "db_to_lin({db}) · db_to_lin(-{db}) should = 1 (got {})",
            up * down
        );
    }
}

// ─── lin_to_db ─────────────────────────────────────────────────────────────

/// Unity (1.0 linear) maps to 0 dB exactly — the reference point.
#[test]
fn lin_to_db_unity_is_zero() {
    assert_eq!(lin_to_db(1.0), 0.0);
}

/// Round-trips with `db_to_lin` for any finite dB value within
/// audible range.  Pins the inverse-pair invariant so a formula
/// drift on one side surfaces as a failed round-trip.
#[test]
fn lin_to_db_round_trips_through_db_to_lin() {
    for &db in &[-96.0_f32, -48.0, -6.0, 0.0, 6.0, 24.0, 48.0] {
        let lin = db_to_lin(db);
        let back = lin_to_db(lin);
        assert!(
            (back - db).abs() < 1e-3,
            "round-trip dB drifted at {db}: got {back}"
        );
    }
}

/// Doubling linear amplitude is +6.0206 dB (the canonical "double =
/// 6 dB" landmark).  Tighter than `db_to_lin_known_values` because
/// here we test the *exact* lin→dB direction.
#[test]
fn lin_to_db_doubling_is_six_db() {
    let two = lin_to_db(2.0);
    assert!((two - 6.020_6).abs() < 1e-3, "lin_to_db(2.0) was {two}");
    let half = lin_to_db(0.5);
    assert!(
        (half - (-6.020_6)).abs() < 1e-3,
        "lin_to_db(0.5) was {half}"
    );
}

// ─── Audible-range bounds ───────────────────────────────────────────────────

/// Pin the literal values: 20 Hz – 20 kHz is the conventional audible
/// band.  Pre-existing call sites used those exact numbers; the
/// constants must reproduce them or every filter-clamp / EQ-band
/// upper-bound in the codebase would drift.
#[test]
fn audible_hz_bounds_are_canonical_values() {
    assert_eq!(AUDIBLE_HZ_MIN, 20.0);
    assert_eq!(AUDIBLE_HZ_MAX, 20_000.0);
}

/// Sanity: MIN sits below MAX, both finite, both positive.  Catches a
/// future swap or sign error on the constants.
#[test]
fn audible_hz_bounds_are_well_ordered() {
    assert!(AUDIBLE_HZ_MIN > 0.0);
    assert!(AUDIBLE_HZ_MIN < AUDIBLE_HZ_MAX);
    assert!(AUDIBLE_HZ_MAX.is_finite());
}

// ─── lfo_target_to_u8 ───────────────────────────────────────────────────────

#[test]
fn lfo_target_none_is_zero_sentinel() {
    // `None` MUST map to 0 — the audio thread uses `target_u8 == 0` to
    // skip silent no-op routes.  Any non-zero mapping here would cause
    // depth+0.0 routes to burn a dispatch slot every sample.
    assert_eq!(crate::audio::dsp::lfo_target_to_u8(LfoTarget::None), 0);
}

#[test]
fn lfo_target_opcodes_are_unique() {
    // Stable IDs only work if distinct variants map to distinct codes.
    // Walk a representative slice and make sure no two produce the same
    // opcode.
    let variants = [
        LfoTarget::None,
        LfoTarget::BassCutoff,
        LfoTarget::BassResonance,
        LfoTarget::BassPitch,
        LfoTarget::BassVolume,
        LfoTarget::BassPan,
        LfoTarget::ReverbMix,
        LfoTarget::DelayTime,
        LfoTarget::DelayFeedback,
        LfoTarget::ChorusMix,
        LfoTarget::ChorusRate,
        LfoTarget::Kick808Pitch,
        LfoTarget::MasterVolume,
        LfoTarget::StereoWidth,
        LfoTarget::GabberKickPan,
        LfoTarget::NeuTtsVolume,
    ];
    let mut seen = std::collections::HashSet::new();
    for v in variants {
        let code = crate::audio::dsp::lfo_target_to_u8(v);
        assert!(
            seen.insert(code),
            "opcode collision: {v:?} → {code} already used by an earlier variant",
        );
    }
}

#[test]
fn lfo_target_known_codes_match_spec() {
    // Spot-check a handful of well-known opcodes.  Changing any of
    // these silently breaks the audio-thread dispatch for existing
    // sessions using those cables, so pin them down.
    use crate::audio::dsp::lfo_target_to_u8;
    assert_eq!(lfo_target_to_u8(LfoTarget::BassCutoff), 1);
    assert_eq!(lfo_target_to_u8(LfoTarget::BassResonance), 2);
    assert_eq!(lfo_target_to_u8(LfoTarget::BassPitch), 3);
    assert_eq!(lfo_target_to_u8(LfoTarget::ReverbMix), 5);
    assert_eq!(lfo_target_to_u8(LfoTarget::DelayTime), 6);
    assert_eq!(lfo_target_to_u8(LfoTarget::Kick808Pitch), 10);
    assert_eq!(lfo_target_to_u8(LfoTarget::MasterVolume), 14);
}

// ─── kind_is_fx ─────────────────────────────────────────────────────────────

#[test]
fn kind_is_fx_covers_every_fx_kind() {
    // Every `FxXxx` ModuleKind must register as FX.  Exhaustively listed
    // here (rather than filtered by `matches!(… FxXxx(…))`) so that
    // adding a new FX variant lights up the test before the cycle-check
    // code gets confused.
    let fx = [
        ModuleKind::FxReverb,
        ModuleKind::FxDelay,
        ModuleKind::FxChorus,
        ModuleKind::FxPhaser,
        ModuleKind::FxFlanger,
        ModuleKind::FxLimiter,
        ModuleKind::FxFilter,
        ModuleKind::FxComb,
        ModuleKind::FxTilt,
        ModuleKind::FxTransient,
        ModuleKind::FxExciter,
        ModuleKind::FxMultitap,
        ModuleKind::FxRevDelay,
        ModuleKind::FxTapeStop,
        ModuleKind::FxStutter,
        ModuleKind::FxFreeze,
        ModuleKind::FxRingMod,
        ModuleKind::FxWaveshaper,
        ModuleKind::FxBitcrush,
        ModuleKind::FxEq,
        ModuleKind::FxCompressor,
        ModuleKind::FxTapeSat,
        ModuleKind::FxDrive,
        ModuleKind::FxAutotune,
        ModuleKind::FxPan,
    ];
    for k in fx {
        assert!(fx_plan::kind_is_fx(k), "{k:?} must be classified as FX");
    }
}

#[test]
fn kind_is_fx_rejects_voice_and_utility_kinds() {
    let non_fx = [
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::HooverLead,
        ModuleKind::An1xVoice,
        ModuleKind::AmenSampler,
        ModuleKind::NoiseVoice,
        ModuleKind::GabberKick,
        ModuleKind::MasterOutput,
        ModuleKind::StepSequencer,
        ModuleKind::LlmAgent,
        ModuleKind::LlmConsole,
        ModuleKind::LfoModule,
        ModuleKind::NeuTts,
    ];
    for k in non_fx {
        assert!(
            !kind_is_fx(k),
            "{k:?} must NOT be classified as FX (only FxXxx kinds are)",
        );
    }
}
