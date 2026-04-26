// ─── tests/vocal_tests.rs ────────────────────────────────────────────────────
// State-level tests for the Vocal formant synth voice.  DSP-side
// tests (silence / audibility / every preset audible / boundedness
// / release) live next to the voice in `audio/dsp/vocal.rs`.

#[cfg(test)]
mod vocal_state_tests {
    use crate::state::{AppState, VOCAL_VOWEL_PRESETS, VocalState, apply_llm_update};

    #[test]
    fn defaults_are_a_male_average_vowel_a() {
        let s = VocalState::default();
        assert!(!s.enabled);
        assert_eq!(s.vowel, 0); // A
        assert_eq!(s.morph, 0.0); // pure preset
        // Male-average centre on the formant_shift knob.
        assert!((s.formant_shift - 0.5).abs() < 1e-5);
        // Bright source by default so the formants articulate
        // clearly rather than reading as a hummed vowel.
        assert!(s.brightness > 0.5);
        // Small attack so the vowel doesn't click on retrigger.
        assert!(s.attack > 0.0 && s.attack < 0.2);
    }

    #[test]
    fn llm_apply_writes_voice_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "vocal": {
                "enabled": true,
                "volume": 1.0,
                "pan": -0.4,
                "vowel": 2,
                "morph": 0.6,
                "brightness": 0.3,
                "formant_shift": 0.8,
                "attack": 0.1,
                "decay": 0.2,
                "sustain": 0.6,
                "release": 0.5,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.vocal.enabled);
        assert!((s1.vocal.volume - 1.0).abs() < 1e-5);
        assert!((s1.vocal.pan - (-0.4)).abs() < 1e-5);
        assert_eq!(s1.vocal.vowel, 2);
        assert!((s1.vocal.morph - 0.6).abs() < 1e-5);
        assert!((s1.vocal.brightness - 0.3).abs() < 1e-5);
        assert!((s1.vocal.formant_shift - 0.8).abs() < 1e-5);
        assert!((s1.vocal.attack - 0.1).abs() < 1e-5);
        assert!((s1.vocal.decay - 0.2).abs() < 1e-5);
        assert!((s1.vocal.sustain - 0.6).abs() < 1e-5);
        assert!((s1.vocal.release - 0.5).abs() < 1e-5);
    }

    #[test]
    fn vowel_clamps_to_preset_range() {
        // Out-of-range values from the LLM must clamp; the DSP
        // table indexes by `vowel as usize` and a stale value
        // would either panic or silently pick the wrong vowel.
        let s0 = AppState::default();
        let json = serde_json::json!({ "vocal": { "vowel": 99 } });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.vocal.vowel < VOCAL_VOWEL_PRESETS);
    }

    #[test]
    fn locked_formant_shift_is_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "vocal": { "formant_shift": 0.95 } });
        let locked = ["vocal.formant_shift".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default formant_shift is 0.5 — locked apply must keep it.
        assert!((s1.vocal.formant_shift - 0.5).abs() < 1e-5);
    }

    #[test]
    fn sequencer_lane_writes_steps_and_notes() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "vocal": {
                "vocal_steps": [true, false, true, false, true],
                "vocal_notes": [60, 62, 64, 67, 72],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.sequencer.vocal_pattern[0].active);
        assert!(!s1.sequencer.vocal_pattern[1].active);
        assert!(s1.sequencer.vocal_pattern[2].active);
        assert!(!s1.sequencer.vocal_pattern[3].active);
        assert!(s1.sequencer.vocal_pattern[4].active);
        assert_eq!(s1.sequencer.vocal_pattern[0].note, 60);
        assert_eq!(s1.sequencer.vocal_pattern[2].note, 64);
        assert_eq!(s1.sequencer.vocal_pattern[4].note, 72);
    }
}

#[cfg(test)]
mod vocal_module_tests {
    use crate::state::{ModuleKind, rack_scope::parse_module_kind};

    #[test]
    fn label_is_vocal() {
        assert_eq!(ModuleKind::VocalVoice.label(), "VOCAL");
    }

    #[test]
    fn parses_from_aliases() {
        // "voice" deliberately omitted from the alias list — that
        // word collides with the NeuTts voice and would render
        // the parser ambiguous.
        for alias in ["vocal", "vocalvoice", "vowel", "formant", "choir"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::VocalVoice),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn produces_audio_and_lives_in_voice_zone() {
        use crate::state::Zone;
        let k = ModuleKind::VocalVoice;
        assert!(k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::Voice);
    }
}
