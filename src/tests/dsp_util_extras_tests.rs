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

use crate::audio::dsp::{hz_to_midi, midi_to_hz};
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
