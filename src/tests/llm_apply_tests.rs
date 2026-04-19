// ─── tests/llm_apply_tests.rs ────────────────────────────────────────────────
// Tests for apply_llm_update() and apply_llm_step_array() in state/llm_apply.rs.
// Covers every top-level JSON key and the scope / lock filtering logic.

#[cfg(test)]
mod llm_apply_bass_tests {
    use crate::state::{AppState, apply_llm_update, lock_param};

    #[test]
    fn bass_sets_multiple_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "bass": { "cutoff": 0.3, "resonance": 0.8, "decay": 0.5 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.bass_voices[0].synth.cutoff - 0.3).abs() < 1e-4);
        assert!((s.bass_voices[0].synth.resonance - 0.8).abs() < 1e-4);
        assert!((s.bass_voices[0].synth.decay - 0.5).abs() < 1e-4);
    }

    #[test]
    fn bass_waveform_square() {
        let s = AppState::default();
        let update = serde_json::json!({ "bass": { "waveform": "Square" } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(
            s.bass_voices[0].synth.waveform,
            crate::state::Waveform::Square
        );
    }

    #[test]
    fn bass_filter_mode_highpass() {
        let s = AppState::default();
        let update = serde_json::json!({ "bass": { "filter_mode": "Highpass" } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(
            s.bass_voices[0].synth.filter_mode,
            crate::state::FilterMode::Highpass
        );
    }

    #[test]
    fn bass_locked_cutoff_not_overwritten() {
        let s = lock_param(AppState::default(), "bass.cutoff");
        let orig = s.bass_voices[0].synth.cutoff;
        let update = serde_json::json!({ "bass": { "cutoff": 0.99 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.bass_voices[0].synth.cutoff, orig);
    }

    #[test]
    fn bass_voices_per_voice_update() {
        let s = AppState::default();
        // voice 0 = null (skip), voice 1 = set cutoff
        let update = serde_json::json!({
            "bass_voices": [null, { "cutoff": 0.7, "enabled": true }]
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.bass_voices[1].synth.cutoff - 0.7).abs() < 1e-4);
        assert!(s.bass_voices[1].enabled);
    }

    #[test]
    fn bass_supersaw_detune_and_voices() {
        let s = AppState::default();
        let update = serde_json::json!({
            "bass": { "supersaw_detune": 0.6, "supersaw_voices": 5 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.bass_voices[0].synth.supersaw_detune - 0.6).abs() < 1e-4);
        assert_eq!(s.bass_voices[0].synth.supersaw_voices, 5);
    }
}

#[cfg(test)]
mod llm_apply_sequencer_tests {
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
}

#[cfg(test)]
mod llm_apply_kit_tests {
    use crate::state::{AppState, apply_llm_update, lock_param};

    #[test]
    fn kit_a_kick_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "kit_a": { "kick": { "pitch_env_depth": 0.7, "pitch_env_time": 0.3, "clip": 0.9 } }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.kit_a.kick.pitch_env_depth - 0.7).abs() < 1e-4);
        assert!((s.kit_a.kick.pitch_env_time - 0.3).abs() < 1e-4);
        assert!((s.kit_a.kick.clip - 0.9).abs() < 1e-4);
    }

    #[test]
    fn kit_b_kick_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "kit_b": { "kick": { "pitch_env_depth": 0.5, "clip": 0.4 } }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.kit_b.kick.pitch_env_depth - 0.5).abs() < 1e-4);
        assert!((s.kit_b.kick.clip - 0.4).abs() < 1e-4);
    }

    #[test]
    fn kit_a_locked_param_not_overwritten() {
        let s = lock_param(AppState::default(), "kit_a.kick.clip");
        let orig = s.kit_a.kick.clip;
        let update = serde_json::json!({ "kit_a": { "kick": { "clip": 0.99 } } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.kit_a.kick.clip, orig);
    }
}

#[cfg(test)]
mod llm_apply_fx_tests {
    use crate::state::{AppState, apply_llm_update, lock_param};

    #[test]
    fn fx_reverb_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "fx": { "reverb_size": 0.8, "reverb_damp": 0.3, "reverb_mix": 0.5 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.reverb_size - 0.8).abs() < 1e-4);
        assert!((s.fx.reverb_damp - 0.3).abs() < 1e-4);
        assert!((s.fx.reverb_mix - 0.5).abs() < 1e-4);
    }

    #[test]
    fn fx_delay_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "fx": { "delay_time": 0.4, "delay_feedback": 0.6, "delay_mix": 0.3 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.delay_time - 0.4).abs() < 1e-4);
        assert!((s.fx.delay_feedback - 0.6).abs() < 1e-4);
        assert!((s.fx.delay_mix - 0.3).abs() < 1e-4);
    }

    #[test]
    fn fx_reverb_xy_pad_writes_pair_0() {
        // `reverb_xy: [x, y]` maps to the canonical Pair 0 knobs
        // (reverb_size × reverb_damp).  reverb_mix stays untouched so
        // agents using Pair 1 / Pair 2 via individual paths aren't
        // overwritten.
        let s = AppState::default();
        let mix_before = s.fx.reverb_mix;
        let update = serde_json::json!({ "fx": { "reverb_xy": [0.72, 0.44] } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.reverb_size - 0.72).abs() < 1e-4);
        assert!((s.fx.reverb_damp - 0.44).abs() < 1e-4);
        assert_eq!(s.fx.reverb_mix, mix_before);
    }

    #[test]
    fn fx_xy_pads_cover_every_declared_pair() {
        // Smoke test across all 13 FX pads: send a distinct [x,y] to each
        // pad's _xy path and verify the pair's fields land on those
        // values.  Guards the XY_PAIRS table against field renames.
        let cases: &[(&str, [f32; 2], fn(&crate::state::FxState) -> (f32, f32))] = &[
            ("reverb_xy", [0.1, 0.2], |f| (f.reverb_size, f.reverb_damp)),
            ("delay_xy", [0.3, 0.4], |f| (f.delay_time, f.delay_feedback)),
            ("chorus_xy", [0.5, 0.6], |f| (f.chorus_rate, f.chorus_depth)),
            ("phaser_xy", [0.7, 0.8], |f| (f.phaser_rate, f.phaser_depth)),
            ("ring_mod_xy", [0.12, 0.24], |f| {
                (f.ring_mod_freq, f.ring_mod_mix)
            }),
            ("waveshaper_xy", [0.36, 0.48], |f| {
                (f.waveshaper_drive, f.waveshaper_mix)
            }),
            ("bitcrush_xy", [0.6, 0.72], |f| {
                (f.bitcrush_bits, f.bitcrush_rate)
            }),
            ("eq_xy", [-0.5, 0.5], |f| (f.eq_low_gain, f.eq_mid_gain)),
            ("compressor_xy", [0.25, 0.75], |f| {
                (f.compressor_threshold, f.compressor_ratio)
            }),
            ("tape_xy", [0.35, 0.65], |f| (f.tape_drive, f.tape_flutter)),
            ("distortion_xy", [0.45, 0.55], |f| {
                (f.distortion_drive, f.distortion_mix)
            }),
            ("autotune_xy", [0.15, 0.85], |f| {
                (f.autotune_amount, f.autotune_mix)
            }),
            ("fx_pan_xy", [0.3, 0.7], |f| (f.fx_pan_pos, f.fx_pan_width)),
        ];
        for (key, [x, y], read) in cases {
            let update = serde_json::json!({ "fx": { *key: [*x, *y] } });
            let s = apply_llm_update(AppState::default(), &update, &[]);
            let (a, b) = read(&s.fx);
            assert!(
                (a - x).abs() < 1e-4 && (b - y).abs() < 1e-4,
                "{}: expected ({},{}), got ({},{})",
                key,
                x,
                y,
                a,
                b
            );
        }
    }

    #[test]
    fn fx_xy_pad_respects_xy_lock_path() {
        // Locking `fx.reverb_xy` blocks the pad but leaves the underlying
        // knobs individually writeable — a selective lock, not a nuke.
        let s = lock_param(AppState::default(), "fx.reverb_xy");
        let size_before = s.fx.reverb_size;
        let update = serde_json::json!({ "fx": { "reverb_xy": [0.99, 0.99] } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.reverb_size, size_before);
        // Individual knob still works.
        let update = serde_json::json!({ "fx": { "reverb_size": 0.6 } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.reverb_size - 0.6).abs() < 1e-4);
    }

    #[test]
    fn fx_xy_pad_respects_individual_knob_lock() {
        // Locking a single field inside a pad lets the other axis still
        // move — avoids the pad silently bypassing a per-knob lock.
        let base = AppState::default();
        let size_before = base.fx.reverb_size;
        let s = lock_param(base, "fx.reverb_size");
        let update = serde_json::json!({ "fx": { "reverb_xy": [0.9, 0.1] } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.reverb_size, size_before);
        assert!((s.fx.reverb_damp - 0.1).abs() < 1e-4);
    }

    #[test]
    fn fx_xy_pad_ignores_non_two_element_arrays() {
        // Malformed inputs (wrong arity, non-numbers) are a no-op —
        // caller's knobs stay put, and the next valid field in the
        // same update still applies.
        let s = AppState::default();
        let size_before = s.fx.reverb_size;
        let update = serde_json::json!({
            "fx": {
                "reverb_xy": [0.5, 0.5, 0.5],
                "reverb_mix": 0.3
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.reverb_size, size_before);
        assert!((s.fx.reverb_mix - 0.3).abs() < 1e-4);
    }

    #[test]
    fn fx_master_pitch_st_clamped() {
        let s = AppState::default();
        let update = serde_json::json!({ "fx": { "master_pitch_st": -7.0 } });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.master_pitch_st - (-7.0)).abs() < 1e-4);

        let update = serde_json::json!({ "fx": { "master_pitch_st": 99.0 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.master_pitch_st, 12.0);
    }

    #[test]
    fn fx_locked_reverb_mix_not_overwritten() {
        let s = lock_param(AppState::default(), "fx.reverb_mix");
        let orig = s.fx.reverb_mix;
        let update = serde_json::json!({ "fx": { "reverb_mix": 0.99 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.fx.reverb_mix, orig);
    }

    #[test]
    fn fx_bitcrush_and_chorus() {
        let s = AppState::default();
        let update = serde_json::json!({
            "fx": { "bitcrush_bits": 0.5, "bitcrush_rate": 0.3, "chorus_rate": 0.7, "chorus_mix": 0.4 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.bitcrush_bits - 0.5).abs() < 1e-4);
        assert!((s.fx.chorus_rate - 0.7).abs() < 1e-4);
    }

    #[test]
    fn fx_eq_and_compressor() {
        let s = AppState::default();
        let update = serde_json::json!({
            "fx": {
                "eq_low_gain": 0.7, "eq_mid_gain": 0.5, "eq_hi_gain": 0.3,
                "compressor_threshold": 0.6, "compressor_ratio": 0.8
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.fx.eq_low_gain - 0.7).abs() < 1e-4);
        assert!((s.fx.compressor_threshold - 0.6).abs() < 1e-4);
    }
}

#[cfg(test)]
mod llm_apply_lfo_tests {
    use crate::state::{AppState, LfoTarget, LfoWaveform, apply_llm_update, lock_param};

    #[test]
    fn lfo_slot_0_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "lfo": [
                { "enabled": true, "rate": 0.5, "depth": 0.8,
                  "waveform": "Triangle", "target": "BassCutoff" }
            ]
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.lfo[0].enabled);
        assert!((s.lfo[0].rate - 0.5).abs() < 1e-4);
        assert!((s.lfo[0].depth - 0.8).abs() < 1e-4);
        assert_eq!(s.lfo[0].waveform, LfoWaveform::Triangle);
        assert_eq!(s.lfo[0].target, LfoTarget::BassCutoff);
    }

    #[test]
    fn lfo_multiple_slots() {
        let s = AppState::default();
        let update = serde_json::json!({
            "lfo": [
                { "rate": 0.2 },
                { "rate": 0.4 },
                { "rate": 0.6 },
                { "rate": 0.8 }
            ]
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.lfo[0].rate - 0.2).abs() < 1e-4);
        assert!((s.lfo[1].rate - 0.4).abs() < 1e-4);
        assert!((s.lfo[2].rate - 0.6).abs() < 1e-4);
        assert!((s.lfo[3].rate - 0.8).abs() < 1e-4);
    }

    #[test]
    fn lfo_waveform_variants() {
        let s = AppState::default();
        let update = serde_json::json!({ "lfo": [{ "waveform": "Saw" }] });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.lfo[0].waveform, LfoWaveform::Saw);

        let update = serde_json::json!({ "lfo": [{ "waveform": "SampleAndHold" }] });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.lfo[0].waveform, LfoWaveform::SampleAndHold);
    }

    #[test]
    fn lfo_target_variants() {
        let s = AppState::default();
        let update = serde_json::json!({ "lfo": [{ "target": "DelayFeedback" }] });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.lfo[0].target, LfoTarget::DelayFeedback);

        let update = serde_json::json!({ "lfo": [{ "target": "Kick808Pitch" }] });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.lfo[0].target, LfoTarget::Kick808Pitch);
    }

    #[test]
    fn lfo_locked_slot_not_overwritten() {
        let s = lock_param(AppState::default(), "lfo[0]");
        let orig_rate = s.lfo[0].rate;
        let update = serde_json::json!({ "lfo": [{ "rate": 0.99 }] });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.lfo[0].rate, orig_rate);
    }

    #[test]
    fn lfo_phase_offset() {
        let s = AppState::default();
        let update = serde_json::json!({ "lfo": [{ "phase_offset": 0.25 }] });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.lfo[0].phase_offset - 0.25).abs() < 1e-4);
    }
}

#[cfg(test)]
mod llm_apply_free_eg_tests {
    use crate::state::{AppState, LfoTarget, apply_llm_update, lock_param};

    #[test]
    fn free_eg_basic_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "free_eg": {
                "enabled": true, "period": 0.5, "depth": 0.7,
                "loop_mode": true, "target": "ReverbMix"
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.free_eg.enabled);
        assert!((s.free_eg.period - 0.5).abs() < 1e-4);
        assert!((s.free_eg.depth - 0.7).abs() < 1e-4);
        assert!(s.free_eg.loop_mode);
        assert_eq!(s.free_eg.target, LfoTarget::ReverbMix);
    }

    #[test]
    fn free_eg_values_array() {
        let s = AppState::default();
        let update = serde_json::json!({
            "free_eg": { "values": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8] }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.free_eg.values[0] - 0.1).abs() < 1e-4);
        assert!((s.free_eg.values[7] - 0.8).abs() < 1e-4);
    }

    #[test]
    fn free_eg_locked_not_overwritten() {
        let s = lock_param(AppState::default(), "free_eg");
        let orig = s.free_eg.period;
        let update = serde_json::json!({ "free_eg": { "period": 0.99 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.free_eg.period, orig);
    }
}

#[cfg(test)]
mod llm_apply_noise_tests {
    use crate::state::{AppState, apply_llm_update, lock_param};

    #[test]
    fn noise_basic_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "noise": { "enabled": true, "volume": 0.6, "color": 0.3, "cutoff": 0.8 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.noise_voice.enabled);
        assert!((s.noise_voice.volume - 0.6).abs() < 1e-4);
        assert!((s.noise_voice.color - 0.3).abs() < 1e-4);
        assert!((s.noise_voice.cutoff - 0.8).abs() < 1e-4);
    }

    #[test]
    fn noise_locked_volume() {
        let s = lock_param(AppState::default(), "noise.volume");
        let orig = s.noise_voice.volume;
        let update = serde_json::json!({ "noise": { "volume": 0.99 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.noise_voice.volume, orig);
    }
}

#[cfg(test)]
mod llm_apply_hoover_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn hoover_basic_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "hoover": {
                "enabled": true, "filter_start": 0.3, "sweep_time": 0.7,
                "resonance": 0.6, "detune": 0.4, "volume": 0.8, "voices": 5
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.hoover.enabled);
        assert!((s.hoover.filter_start - 0.3).abs() < 1e-4);
        assert!((s.hoover.sweep_time - 0.7).abs() < 1e-4);
        assert!((s.hoover.resonance - 0.6).abs() < 1e-4);
        assert!((s.hoover.detune - 0.4).abs() < 1e-4);
        assert!((s.hoover.volume - 0.8).abs() < 1e-4);
        assert_eq!(s.hoover.voices, 5);
    }

    #[test]
    fn hoover_steps_and_notes() {
        let s = AppState::default();
        let update = serde_json::json!({
            "hoover": {
                "hoover_steps": [true, false, true, false],
                "hoover_notes": [60, 62, 64, 65]
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.sequencer.hoover_pattern[0].active);
        assert!(!s.sequencer.hoover_pattern[1].active);
        assert!(s.sequencer.hoover_pattern[2].active);
        assert_eq!(s.sequencer.hoover_pattern[0].note, 60);
        assert_eq!(s.sequencer.hoover_pattern[2].note, 64);
    }

    #[test]
    fn hoover_lfo_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "hoover": { "pitch_lfo_rate": 0.3, "pitch_lfo_depth": 0.5 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.hoover.pitch_lfo_rate - 0.3).abs() < 1e-4);
        assert!((s.hoover.pitch_lfo_depth - 0.5).abs() < 1e-4);
    }
}

#[cfg(test)]
mod llm_apply_an1x_tests {
    use crate::state::{AppState, apply_llm_update, lock_param};

    #[test]
    fn an1x_basic_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "an1x": {
                "enabled": true, "volume": 0.7, "filter_cutoff": 0.6,
                "filter_resonance": 0.4, "drift": 0.2
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.an1x.enabled);
        assert!((s.an1x.volume - 0.7).abs() < 1e-4);
        assert!((s.an1x.filter_cutoff - 0.6).abs() < 1e-4);
        assert!((s.an1x.filter_resonance - 0.4).abs() < 1e-4);
        assert!((s.an1x.drift - 0.2).abs() < 1e-4);
    }

    #[test]
    fn an1x_envelope_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "an1x": {
                "filter_attack": 0.1, "filter_decay": 0.3,
                "filter_sustain": 0.5, "filter_release": 0.7,
                "amp_attack": 0.2, "amp_decay": 0.4,
                "amp_sustain": 0.6, "amp_release": 0.8
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.an1x.filter_attack - 0.1).abs() < 1e-4);
        assert!((s.an1x.filter_release - 0.7).abs() < 1e-4);
        assert!((s.an1x.amp_attack - 0.2).abs() < 1e-4);
        assert!((s.an1x.amp_release - 0.8).abs() < 1e-4);
    }

    #[test]
    fn an1x_steps_and_notes() {
        let s = AppState::default();
        let update = serde_json::json!({
            "an1x": {
                "an1x_steps": [true, true, false, false],
                "an1x_notes": [48, 50, 52, 53]
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.sequencer.an1x_pattern[0].active);
        assert!(s.sequencer.an1x_pattern[1].active);
        assert!(!s.sequencer.an1x_pattern[2].active);
        assert_eq!(s.sequencer.an1x_pattern[0].note, 48);
        assert_eq!(s.sequencer.an1x_pattern[3].note, 53);
    }

    #[test]
    fn an1x_locked_volume_not_overwritten() {
        let s = lock_param(AppState::default(), "an1x.volume");
        let orig = s.an1x.volume;
        let update = serde_json::json!({ "an1x": { "volume": 0.99 } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.an1x.volume, orig);
    }

    #[test]
    fn an1x_lfo_and_pitch_env() {
        let s = AppState::default();
        let update = serde_json::json!({
            "an1x": {
                "lfo_rate": 0.3, "lfo_depth": 0.5, "lfo_bpm_sync": true,
                "pitch_env_attack": 0.2, "pitch_env_decay": 0.4, "pitch_env_amount": 0.6
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.an1x.lfo_rate - 0.3).abs() < 1e-4);
        assert!(s.an1x.lfo_bpm_sync);
        assert!((s.an1x.pitch_env_amount - 0.6).abs() < 1e-4);
    }
}

#[cfg(test)]
mod llm_apply_euclidean_tests {
    use crate::state::{AppState, DrumVoice, apply_llm_update, lock_param};

    #[test]
    fn euclidean_kick_5_over_16() {
        let s = AppState::default();
        let update = serde_json::json!({
            "euclidean": { "voice": "kick_a", "pulses": 5, "steps": 16 }
        });
        let s = apply_llm_update(s, &update, &[]);
        let kick = &s.sequencer.drum_patterns[&DrumVoice::Kick808];
        let active_count: usize = kick.iter().take(16).filter(|s| s.active).count();
        assert_eq!(active_count, 5);
    }

    #[test]
    fn euclidean_hihat_b() {
        let s = AppState::default();
        let update = serde_json::json!({
            "euclidean": { "voice": "hihat_b", "pulses": 8, "steps": 16 }
        });
        let s = apply_llm_update(s, &update, &[]);
        let hat = &s.sequencer.drum_patterns[&DrumVoice::HihatClosed909];
        let active_count: usize = hat.iter().take(16).filter(|s| s.active).count();
        assert_eq!(active_count, 8);
    }

    #[test]
    fn euclidean_locked_voice_not_overwritten() {
        let s = lock_param(AppState::default(), "sequencer.kick_a_steps");
        let before = s.sequencer.drum_patterns[&DrumVoice::Kick808].clone();
        let update = serde_json::json!({
            "euclidean": { "voice": "kick_a", "pulses": 7, "steps": 16 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.sequencer.drum_patterns[&DrumVoice::Kick808], before);
    }

    #[test]
    fn euclidean_unknown_voice_is_noop() {
        let s = AppState::default();
        let before_kick = s.sequencer.drum_patterns[&DrumVoice::Kick808].clone();
        let update = serde_json::json!({
            "euclidean": { "voice": "cowbell", "pulses": 4, "steps": 16 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.sequencer.drum_patterns[&DrumVoice::Kick808], before_kick);
    }
}

// Rack, scope, step-array, and combined tests are in llm_apply_extra_tests.rs.
