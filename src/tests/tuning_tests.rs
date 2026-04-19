// ─── tests/tuning_tests.rs ───────────────────────────────────────────────────
// Covers `midi_to_hz_tuned` — the dispatcher over the four tuning systems
// used by the an1x / bass voices: 12-TET, just intonation, Javanese
// slendro, Javanese pelog.  Test invariants that are stable across each
// system rather than every pitch/ratio combo: C4 reference pitch, octave
// doubling, and "just intonation disagrees with 12-TET at the major
// third" (the classic tuning-system smoke test).

use crate::audio::dsp::{midi_to_hz, midi_to_hz_tuned};

const C4_HZ: f32 = 261.626;
const A4_HZ: f32 = 440.0;

#[test]
fn tuning_zero_matches_12_tet() {
    // tuning=0 (and any unknown value >3) must fall back to 12-TET
    // unchanged.  A regression here means the dispatcher's default arm
    // has drifted from midi_to_hz.
    for note in [48_u8, 60, 69, 72, 84] {
        assert!(
            (midi_to_hz_tuned(note, 0) - midi_to_hz(note)).abs() < 1e-3,
            "tuning=0 must equal 12-TET at MIDI {note}",
        );
    }
    // Unknown tuning codes fall through to 12-TET too.
    assert!((midi_to_hz_tuned(69, 99) - A4_HZ).abs() < 1e-2);
}

#[test]
fn just_intonation_c4_anchors_at_standard_c() {
    // C4 (MIDI 60) is the unison in every tuning — degree=0, ratio=1.0.
    let c4 = midi_to_hz_tuned(60, 1);
    assert!(
        (c4 - C4_HZ).abs() < 0.5,
        "JI C4 should anchor at 261.626, got {c4}",
    );
}

#[test]
fn just_intonation_octave_doubles_frequency() {
    // Octaves must still double exactly — this holds across ALL tuning
    // systems because an octave is multiplication by 2 at the end.
    let c4 = midi_to_hz_tuned(60, 1);
    let c5 = midi_to_hz_tuned(72, 1);
    assert!(
        (c5 / c4 - 2.0).abs() < 1e-3,
        "JI octave ratio should be 2, got {}",
        c5 / c4,
    );
}

#[test]
fn just_intonation_major_third_differs_from_equal_tempered() {
    // The classic "proof" that JI is a different system: the major third
    // (E4, MIDI 64) is a 5/4 ratio in JI (~327.03 Hz) but ~329.63 Hz in
    // 12-TET.  Difference is ~2.6 Hz — comfortably outside float noise.
    let e4_ji = midi_to_hz_tuned(64, 1);
    let e4_12tet = midi_to_hz_tuned(64, 0);
    let expected_ji = C4_HZ * 5.0 / 4.0;
    assert!(
        (e4_ji - expected_ji).abs() < 0.5,
        "JI major third should be 5/4 * C4 = {expected_ji}, got {e4_ji}",
    );
    assert!(
        (e4_ji - e4_12tet).abs() > 1.0,
        "JI major third must audibly differ from 12-TET",
    );
}

#[test]
fn slendro_is_five_tone_equal() {
    // Slendro: 5-tone equal temperament, each MIDI step = one slendro
    // step of 2^(1/5) ≈ 1.1487.  Five MIDI steps equal one octave.
    let s0 = midi_to_hz_tuned(60, 2);
    let s1 = midi_to_hz_tuned(61, 2);
    let s5 = midi_to_hz_tuned(65, 2);
    assert!((s0 - C4_HZ).abs() < 0.5);
    let expected_step = 2.0_f32.powf(1.0 / 5.0);
    assert!(
        (s1 / s0 - expected_step).abs() < 1e-3,
        "slendro step ratio should be 2^(1/5) ≈ {expected_step}, got {}",
        s1 / s0,
    );
    assert!(
        (s5 / s0 - 2.0).abs() < 1e-3,
        "5 slendro steps must equal one octave, got ratio {}",
        s5 / s0,
    );
}

#[test]
fn slendro_differs_from_12_tet_above_the_fifth_step() {
    // At MIDI 65, slendro has completed one full octave (+1200 cents)
    // while 12-TET has only reached a perfect fourth (+500 cents).  The
    // frequencies should be audibly different — this is the regression
    // guard for the old bug where the slendro math silently reduced to
    // 12-TET.
    let slendro = midi_to_hz_tuned(65, 2);
    let twelve_tet = midi_to_hz_tuned(65, 0);
    assert!(
        slendro > twelve_tet * 1.3,
        "slendro at MIDI 65 must be ~1 octave above C4 (~523 Hz), 12-TET is ~349 Hz; got slendro={slendro}, 12-TET={twelve_tet}",
    );
}

#[test]
fn pelog_is_seven_tone_per_octave() {
    // Pelog maps 7 notes to an octave.  MIDI notes 60 and 67 (a fifth
    // apart in 12-TET) are the same scale degree 0 in pelog, one octave
    // apart.  The ratio must be exactly 2.
    let p0 = midi_to_hz_tuned(60, 3);
    let p7 = midi_to_hz_tuned(67, 3);
    assert!(
        (p7 / p0 - 2.0).abs() < 1e-3,
        "7 pelog steps must equal one octave, got ratio {}",
        p7 / p0,
    );
}

#[test]
fn pelog_intervals_are_non_equal() {
    // Pelog's defining trait is non-equal intervals — adjacent steps
    // should differ in size.  We compare step 60→61 to step 63→64 and
    // require them to differ by more than float noise.
    let r1 = midi_to_hz_tuned(61, 3) / midi_to_hz_tuned(60, 3);
    let r2 = midi_to_hz_tuned(64, 3) / midi_to_hz_tuned(63, 3);
    assert!(
        (r1 - r2).abs() > 1e-3,
        "pelog must have NON-equal intervals; got r1={r1}, r2={r2}",
    );
}
