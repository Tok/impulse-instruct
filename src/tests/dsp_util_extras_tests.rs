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
    NYQUIST_GUARD_FACTOR, hz_to_midi, midi_to_hz, midi_to_hz_f32, nyquist_guard,
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
