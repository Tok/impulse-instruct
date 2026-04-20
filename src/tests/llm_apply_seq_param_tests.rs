// ─── tests/llm_apply_seq_param_tests.rs ──────────────────────────────────────
// Tests for apply_llm_update()'s `sequencer` branch — all BPM/swing/time-sig
// + per-voice pattern array decoding.  Split out of llm_apply_tests.rs to
// keep both files under the 1000-line hook limit.

use crate::state::{AppState, DrumVoice, Scale, apply_llm_update, lock_param};

#[test]
fn sequencer_bpm_applied_and_clamped() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "bpm": 180.0 } });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.bpm - 180.0).abs() < 0.01);

    let update = serde_json::json!({ "sequencer": { "bpm": 999.0 } });
    let s = apply_llm_update(s, &update, &[]);
    assert!(s.sequencer.bpm <= 250.0);
}

#[test]
fn sequencer_swing() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "swing": 0.65 } });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.swing - 0.65).abs() < 1e-4);
}

#[test]
fn sequencer_time_sig_num() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "time_sig_num": 7 } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.time_sig_num, 7);
}

#[test]
fn sequencer_time_sig_num_clamps() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "time_sig_num": 99 } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.time_sig_num, 9);
}

#[test]
fn sequencer_root_note_and_scale() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "root_note": 5, "scale": "Dorian" } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.root_note, 5);
    assert_eq!(s.sequencer.scale, Scale::Dorian);
}

#[test]
fn sequencer_drum_lengths_polyrhythm() {
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": { "drum_lengths": { "kick_a": 12, "hihat_a": 7 } }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.drum_steps[&DrumVoice::Kick808], 12);
    assert_eq!(s.sequencer.drum_steps[&DrumVoice::HihatClosed808], 7);
}

#[test]
fn sequencer_drum_ratchets() {
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": { "drum_ratchets": { "kick_a": [2, 1, 1, 3] } }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.drum_patterns[&DrumVoice::Kick808][0].ratchet, 2);
    assert_eq!(s.sequencer.drum_patterns[&DrumVoice::Kick808][3].ratchet, 3);
}

#[test]
fn sequencer_drum_probabilities() {
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": {
            "drum_probabilities": {
                "hihat_a": [1.0, 0.6, 1.0, 0.5, 1.0, 0.8, 1.0, 0.4]
            }
        }
    });
    let s = apply_llm_update(s, &update, &[]);
    let pat = &s.sequencer.drum_patterns[&DrumVoice::HihatClosed808];
    assert!((pat[0].probability - 1.0).abs() < 1e-4);
    assert!((pat[1].probability - 0.6).abs() < 1e-4);
    assert!((pat[7].probability - 0.4).abs() < 1e-4);
    // Untouched steps stay at default 1.0.
    assert!((pat[8].probability - 1.0).abs() < 1e-4);
}

#[test]
fn sequencer_drum_probabilities_clamps_and_lock() {
    // Out-of-range values clamp to [0, 1]; `sequencer.drum_probabilities`
    // lock keeps the whole object from applying.
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": {
            "drum_probabilities": { "kick_a": [2.5, -1.0, 0.33] }
        }
    });
    let s = apply_llm_update(s, &update, &[]);
    let pat = &s.sequencer.drum_patterns[&DrumVoice::Kick808];
    assert!((pat[0].probability - 1.0).abs() < 1e-4);
    assert!((pat[1].probability - 0.0).abs() < 1e-4);
    assert!((pat[2].probability - 0.33).abs() < 1e-4);

    let s = crate::state::lock_param(AppState::default(), "sequencer.drum_probabilities");
    let update = serde_json::json!({
        "sequencer": { "drum_probabilities": { "kick_a": [0.25] } }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.drum_patterns[&DrumVoice::Kick808][0].probability - 1.0).abs() < 1e-4);
}

#[test]
fn sequencer_bass_len_hoover_len_an1x_len() {
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": { "bass_len": 8, "hoover_len": 12, "an1x_len": 24 }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bass_steps, 8);
    assert_eq!(s.sequencer.hoover_steps, 12);
    assert_eq!(s.sequencer.an1x_steps, 24);
}

#[test]
fn sequencer_bass_steps_index_list() {
    let s = AppState::default();
    // Index list format: [0, 4, 8] activates those positions, clears rest
    let update = serde_json::json!({ "sequencer": { "bass_steps": [0, 4, 8] } });
    let s = apply_llm_update(s, &update, &[]);
    assert!(s.sequencer.bass_pattern[0].active);
    assert!(!s.sequencer.bass_pattern[1].active);
    assert!(s.sequencer.bass_pattern[4].active);
    assert!(s.sequencer.bass_pattern[8].active);
}

#[test]
fn sequencer_bass_notes() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "bass_notes": [36, 38, 40, 41] } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bass_pattern[0].note, 36);
    assert_eq!(s.sequencer.bass_pattern[1].note, 38);
    assert_eq!(s.sequencer.bass_pattern[3].note, 41);
}

#[test]
fn sequencer_bass_accents_index_list() {
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": { "bass_accents": [0, 6, 19] }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert!(s.sequencer.bass_pattern[0].accent > 0.0);
    assert_eq!(s.sequencer.bass_pattern[1].accent, 0.0);
    assert!(s.sequencer.bass_pattern[6].accent > 0.0);
    assert!(s.sequencer.bass_pattern[19].accent > 0.0);
    // Voice 0's per-voice pattern mirrors the legacy bass_pattern.
    assert!(s.sequencer.bass_patterns[0][0].accent > 0.0);
    assert!(s.sequencer.bass_patterns[0][6].accent > 0.0);
}

#[test]
fn sequencer_bass_slides_bool_array() {
    let s = AppState::default();
    // 32-element inline bool array (≥16 triggers inline path in helper).
    let mut arr = vec![false; 32];
    arr[3] = true;
    arr[10] = true;
    arr[19] = true;
    let update = serde_json::json!({ "sequencer": { "bass_slides": arr } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bass_pattern[0].slide, 0.0);
    assert!(s.sequencer.bass_pattern[3].slide > 0.0);
    assert!(s.sequencer.bass_pattern[10].slide > 0.0);
    assert!(s.sequencer.bass_pattern[19].slide > 0.0);
    assert_eq!(s.sequencer.bass_pattern[20].slide, 0.0);
    // Voice 0 mirror.
    assert!(s.sequencer.bass_patterns[0][3].slide > 0.0);
}

#[test]
fn sequencer_bass_accents_respects_lock() {
    let s = AppState::default();
    let s = lock_param(s, "sequencer.bass_accents");
    let update = serde_json::json!({
        "sequencer": { "bass_accents": [0, 4, 8] }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bass_pattern[0].accent, 0.0);
    assert_eq!(s.sequencer.bass_pattern[4].accent, 0.0);
}

#[test]
fn sequencer_bass2_steps_target_voice_1() {
    // bass2_steps writes bass_patterns[1] only — voice 0 (bass_pattern)
    // must remain untouched.
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": { "bass2_steps": [0, 6, 12] }
    });
    let s = apply_llm_update(s, &update, &[]);
    // Voice 1 got the pattern.
    assert!(s.sequencer.bass_patterns[1][0].active);
    assert!(s.sequencer.bass_patterns[1][6].active);
    assert!(s.sequencer.bass_patterns[1][12].active);
    // Voice 0 and the legacy mirror are untouched.
    assert!(!s.sequencer.bass_pattern[0].active);
    assert!(!s.sequencer.bass_patterns[0][6].active);
}

#[test]
fn sequencer_bass_pans_applied() {
    let s = AppState::default();
    let mut pans = vec![0.0_f32; 32];
    pans[3] = 0.3;
    pans[10] = -0.3;
    let update = serde_json::json!({ "sequencer": { "bass_pans": pans } });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.bass_pattern[3].pan - 0.3).abs() < 1e-6);
    assert!((s.sequencer.bass_pattern[10].pan - (-0.3)).abs() < 1e-6);
    assert_eq!(s.sequencer.bass_patterns[0][3].pan, 0.3);
}

#[test]
fn sequencer_bass_accents_proportional_float_array() {
    // New behaviour: LLM can emit float intensities 0..=1 (not just
    // bools).  Inline per-step format requires ≥16 values.
    let s = AppState::default();
    let mut accents = vec![0.0_f32; 32];
    accents[0] = 1.0; // full accent on the downbeat
    accents[4] = 0.5; // half-accent colour hit
    accents[12] = 0.3; // lighter
    let update = serde_json::json!({ "sequencer": { "bass_accents": accents } });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.bass_pattern[0].accent - 1.0).abs() < 1e-6);
    assert!((s.sequencer.bass_pattern[4].accent - 0.5).abs() < 1e-6);
    assert!((s.sequencer.bass_pattern[12].accent - 0.3).abs() < 1e-6);
    assert_eq!(s.sequencer.bass_pattern[1].accent, 0.0);
    // Voice 0 mirror follows.
    assert!((s.sequencer.bass_patterns[0][4].accent - 0.5).abs() < 1e-6);
}

#[test]
fn sequencer_bass_slides_proportional_float_array() {
    let s = AppState::default();
    let mut slides = vec![0.0_f32; 32];
    slides[3] = 0.2; // light glide
    slides[10] = 1.0; // full glide
    let update = serde_json::json!({ "sequencer": { "bass_slides": slides } });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.bass_pattern[3].slide - 0.2).abs() < 1e-6);
    assert!((s.sequencer.bass_pattern[10].slide - 1.0).abs() < 1e-6);
    assert_eq!(s.sequencer.bass_pattern[0].slide, 0.0);
}

#[test]
fn sequencer_bass_accents_bool_array_still_accepted() {
    // Backwards compat: old LLM output emitted bool inline arrays.
    let s = AppState::default();
    let mut accents: Vec<bool> = vec![false; 32];
    accents[0] = true;
    accents[8] = true;
    let update = serde_json::json!({ "sequencer": { "bass_accents": accents } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bass_pattern[0].accent, 1.0);
    assert_eq!(s.sequencer.bass_pattern[8].accent, 1.0);
    assert_eq!(s.sequencer.bass_pattern[1].accent, 0.0);
}

#[test]
fn sequencer_bass2_pans_applied() {
    let s = AppState::default();
    let mut pans = vec![0.0_f32; 32];
    pans[0] = 0.4;
    pans[16] = -0.4;
    let update = serde_json::json!({ "sequencer": { "bass2_pans": pans } });
    let s = apply_llm_update(s, &update, &[]);
    assert!((s.sequencer.bass_patterns[1][0].pan - 0.4).abs() < 1e-6);
    assert!((s.sequencer.bass_patterns[1][16].pan - (-0.4)).abs() < 1e-6);
    // Voice 0 untouched.
    assert_eq!(s.sequencer.bass_pattern[0].pan, 0.0);
}

#[test]
fn sequencer_bass2_notes_accents_slides() {
    let s = AppState::default();
    let update = serde_json::json!({
        "sequencer": {
            "bass2_notes":   [48, 43, 41, 36],
            "bass2_accents": [0, 3],
            "bass2_slides":  [0]
        }
    });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bass_patterns[1][0].note, 48);
    assert_eq!(s.sequencer.bass_patterns[1][3].note, 36);
    assert!(s.sequencer.bass_patterns[1][0].accent > 0.0);
    assert!(s.sequencer.bass_patterns[1][3].accent > 0.0);
    assert!(s.sequencer.bass_patterns[1][0].slide > 0.0);
    // Voice 0 untouched.
    assert_eq!(s.sequencer.bass_pattern[0].note, 36); // default C2
    assert_eq!(s.sequencer.bass_pattern[0].accent, 0.0);
}

#[test]
fn sequencer_bass2_steps_independent_lock_from_bass_steps() {
    // Locking sequencer.bass_steps must NOT freeze bass2_steps.
    let s = AppState::default();
    let s = lock_param(s, "sequencer.bass_steps");
    let update = serde_json::json!({
        "sequencer": {
            "bass_steps":  [0, 4, 8, 12],
            "bass2_steps": [0, 6]
        }
    });
    let s = apply_llm_update(s, &update, &[]);
    // Voice 0 pattern locked — no changes.
    assert!(!s.sequencer.bass_pattern[4].active);
    // Voice 1 still writable.
    assert!(s.sequencer.bass_patterns[1][0].active);
    assert!(s.sequencer.bass_patterns[1][6].active);
}

#[test]
fn sequencer_kick_a_steps_index_list() {
    let s = AppState::default();
    let update = serde_json::json!({ "sequencer": { "kick_a_steps": [0, 4, 8, 12] } });
    let s = apply_llm_update(s, &update, &[]);
    let kick = &s.sequencer.drum_patterns[&DrumVoice::Kick808];
    assert!(kick[0].active);
    assert!(!kick[1].active);
    assert!(kick[4].active);
    assert!(kick[12].active);
}

#[test]
fn sequencer_locked_bpm_not_overwritten() {
    let s = lock_param(AppState::default(), "sequencer.bpm");
    let orig = s.sequencer.bpm;
    let update = serde_json::json!({ "sequencer": { "bpm": 200.0 } });
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.bpm, orig);
}

#[test]
fn sequencer_drum_steps_locked() {
    let s = lock_param(AppState::default(), "sequencer.kick_a_steps");
    let update = serde_json::json!({ "sequencer": { "kick_a_steps": [0, 2, 4] } });
    let before = s.sequencer.drum_patterns[&DrumVoice::Kick808].clone();
    let s = apply_llm_update(s, &update, &[]);
    assert_eq!(s.sequencer.drum_patterns[&DrumVoice::Kick808], before);
}
