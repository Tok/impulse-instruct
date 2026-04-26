// ─── tests/additive_tests.rs ─────────────────────────────────────────────────
// State-level tests for the Additive synth voice.  DSP-side tests
// (silence, audibility, partial frequencies, boundedness, release)
// live next to the voice in `audio/dsp/additive.rs`.

#[cfg(test)]
mod additive_state_tests {
    use crate::state::{ADDITIVE_HARMONICS, AdditiveState, AppState, apply_llm_update};

    #[test]
    fn defaults_have_one_over_n_falloff() {
        let s = AdditiveState::default();
        assert!(!s.enabled);
        // 1/n falloff per the default — index 0 = 1.0,
        // index 1 = 0.5, index 7 = 1/8 = 0.125.
        assert!((s.levels[0] - 1.0).abs() < 1e-5);
        assert!((s.levels[1] - 0.5).abs() < 1e-5);
        assert!((s.levels[7] - 0.125).abs() < 1e-5);
    }

    #[test]
    fn llm_apply_writes_voice_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "additive": {
                "enabled": true,
                "volume": 0.9,
                "pan": -0.4,
                "attack": 0.2,
                "decay": 0.5,
                "sustain": 0.3,
                "release": 0.6,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.additive.enabled);
        assert!((s1.additive.volume - 0.9).abs() < 1e-5);
        assert!((s1.additive.pan + 0.4).abs() < 1e-5);
        assert!((s1.additive.attack - 0.2).abs() < 1e-5);
        assert!((s1.additive.decay - 0.5).abs() < 1e-5);
        assert!((s1.additive.sustain - 0.3).abs() < 1e-5);
        assert!((s1.additive.release - 0.6).abs() < 1e-5);
    }

    #[test]
    fn llm_apply_writes_partial_levels() {
        let s0 = AppState::default();
        // [1, 0, 1, 0, ...] = square approximation
        let json = serde_json::json!({
            "additive": {
                "levels": [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0,
                           1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        for i in 0..ADDITIVE_HARMONICS {
            let expected = if i.is_multiple_of(2) { 1.0 } else { 0.0 };
            assert!(
                (s1.additive.levels[i] - expected).abs() < 1e-5,
                "level[{i}] = {}, expected {expected}",
                s1.additive.levels[i]
            );
        }
    }

    #[test]
    fn shorter_levels_array_leaves_trailing_partials_alone() {
        // User sends only the first 3 levels — the rest should
        // keep the prior values, not zero out.  Useful so the
        // LLM can adjust just the fundamental + 2nd + 3rd
        // without having to repeat the 1/n tail every time.
        let s0 = AppState::default();
        let prior_4th = s0.additive.levels[3];
        let json = serde_json::json!({
            "additive": { "levels": [0.5, 0.6, 0.7] }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.additive.levels[0] - 0.5).abs() < 1e-5);
        assert!((s1.additive.levels[1] - 0.6).abs() < 1e-5);
        assert!((s1.additive.levels[2] - 0.7).abs() < 1e-5);
        assert!((s1.additive.levels[3] - prior_4th).abs() < 1e-5);
    }

    #[test]
    fn locked_levels_are_skipped_entirely() {
        let s0 = AppState::default();
        let prior = s0.additive.levels;
        let json = serde_json::json!({
            "additive": {
                "levels": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                           0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            }
        });
        let locked = ["additive.levels".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        for i in 0..ADDITIVE_HARMONICS {
            assert!(
                (s1.additive.levels[i] - prior[i]).abs() < 1e-5,
                "locked levels should be unchanged"
            );
        }
    }

    #[test]
    fn sequencer_lane_writes_steps_and_notes() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "additive": {
                "additive_steps": [true, false, true, false],
                "additive_notes": [60, 64, 67, 72],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.sequencer.additive_pattern[0].active);
        assert!(!s1.sequencer.additive_pattern[1].active);
        assert!(s1.sequencer.additive_pattern[2].active);
        assert_eq!(s1.sequencer.additive_pattern[0].note, 60);
        assert_eq!(s1.sequencer.additive_pattern[2].note, 67);
    }
}

#[cfg(test)]
mod additive_module_tests {
    use crate::state::{ModuleKind, rack_scope::parse_module_kind};

    #[test]
    fn label_is_additive() {
        assert_eq!(ModuleKind::AdditiveVoice.label(), "ADDITIVE");
    }

    #[test]
    fn parses_from_aliases() {
        for alias in ["additive", "harmonic", "partials", "drawbar", "organ"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::AdditiveVoice),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn produces_audio_and_lives_in_voice_zone() {
        use crate::state::Zone;
        let k = ModuleKind::AdditiveVoice;
        assert!(k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::Voice);
    }
}
