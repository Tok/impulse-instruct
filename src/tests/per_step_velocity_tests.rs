// ─── tests/per_step_velocity_tests.rs ────────────────────────────────────────
// Per-step velocity curves: bass accent is now a 0..=1 scalar (always
// was internally), so the LLM and the UI both expose fractional
// values.  These tests lock the new `set_bass_accent_voice` pure
// transition + the LLM round-trip.

use crate::state::{AppState, apply_llm_update, set_bass_accent_voice};

// ─── set_bass_accent_voice ───────────────────────────────────────────────────

#[test]
fn set_bass_accent_voice_writes_value_to_voice_zero_and_legacy_pattern() {
    // Voice 0 lives in two slots: `bass_pattern` (legacy) and
    // `bass_patterns[0]`.  Touching it must update both so any UI
    // that still reads the legacy pattern stays consistent.
    let s = AppState::default();
    let s = set_bass_accent_voice(s, 0, 3, 0.42);
    assert!((s.sequencer.bass_pattern[3].accent - 0.42).abs() < 1e-6);
    assert!((s.sequencer.bass_patterns[0][3].accent - 0.42).abs() < 1e-6);
}

#[test]
fn set_bass_accent_voice_only_touches_target_voice() {
    // Setting voice 1's accent must leave voice 0's pattern alone —
    // mirroring the legacy slot is voice-0 only.
    let s = AppState::default();
    let s = set_bass_accent_voice(s, 1, 5, 0.7);
    assert!((s.sequencer.bass_patterns[1][5].accent - 0.7).abs() < 1e-6);
    // bass_pattern (legacy = voice 0) stays at default zero.
    assert_eq!(s.sequencer.bass_pattern[5].accent, 0.0);
    assert_eq!(s.sequencer.bass_patterns[0][5].accent, 0.0);
}

#[test]
fn set_bass_accent_voice_clamps_to_unit_interval() {
    let s = AppState::default();
    let s = set_bass_accent_voice(s, 0, 0, 1.7);
    assert_eq!(s.sequencer.bass_patterns[0][0].accent, 1.0);
    let s = set_bass_accent_voice(s, 0, 1, -0.4);
    assert_eq!(s.sequencer.bass_patterns[0][1].accent, 0.0);
}

#[test]
fn set_bass_accent_voice_handles_out_of_range_voice_idx() {
    // Voice index > MAX-1 should clamp to the last voice rather
    // than panic — caller bugs shouldn't crash the audio thread.
    let s = AppState::default();
    let s = set_bass_accent_voice(s, 99, 0, 0.5);
    let last = crate::state::MAX_BASS_VOICES - 1;
    assert!((s.sequencer.bass_patterns[last][0].accent - 0.5).abs() < 1e-6);
}

// ─── LLM apply ───────────────────────────────────────────────────────────────

#[test]
fn apply_llm_update_round_trips_fractional_bass_accents() {
    // The intensity_array schema accepts floats per step.  The
    // pipeline used to push them through binary-leaning UI feedback;
    // with the per-step curve UI in place the LLM can now author
    // expressive grooves directly.
    let s0 = AppState::default();
    let update = serde_json::json!({
        "sequencer": {
            "bass_accents": [0.0, 0.3, 0.7, 1.0, 0.5, 0.0, 0.9, 0.25],
        }
    });
    let s1 = apply_llm_update(s0, &update, &[]);
    let p = &s1.sequencer.bass_patterns[0];
    assert!((p[0].accent - 0.0).abs() < 1e-6);
    assert!((p[1].accent - 0.3).abs() < 1e-6);
    assert!((p[2].accent - 0.7).abs() < 1e-6);
    assert!((p[3].accent - 1.0).abs() < 1e-6);
    assert!((p[4].accent - 0.5).abs() < 1e-6);
    assert!((p[5].accent - 0.0).abs() < 1e-6);
    assert!((p[6].accent - 0.9).abs() < 1e-6);
    assert!((p[7].accent - 0.25).abs() < 1e-6);
    // Legacy pattern slot must mirror voice 0's accents.
    let legacy = &s1.sequencer.bass_pattern;
    for i in 0..8 {
        assert!((legacy[i].accent - p[i].accent).abs() < 1e-6);
    }
}
