// ─── tests/modal_tests.rs ────────────────────────────────────────────────────
// State-level tests for the modal / struck physical-model voice.
// DSP-side tests (silence / audibility / boundedness / ring-out /
// per-preset coverage) live next to the voice in
// `audio/dsp/modal.rs`.

#[cfg(test)]
mod modal_state_tests {
    use crate::state::{AppState, MODAL_MODES, MODAL_RATIO_PRESETS, ModalState, apply_llm_update};

    #[test]
    fn defaults_are_a_bell_with_classic_partial_levels() {
        let s = ModalState::default();
        assert!(!s.enabled);
        assert_eq!(s.ratio_preset, 1); // Bell
        // Strike tone (mode 0) is the loudest by default; mode 1
        // (the "hum") is next, decaying through the higher modes.
        // This matches a real bell's perceived spectral envelope.
        assert!((s.levels[0] - 1.0).abs() < 1e-5);
        assert!(s.levels[0] > s.levels[1]);
        assert!(s.levels[1] > s.levels[2]);
        assert!(s.levels[3] > s.levels[7]);
    }

    #[test]
    fn llm_apply_writes_voice_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "modal": {
                "enabled": true,
                "volume": 0.9,
                "pan": 0.3,
                "brightness": 0.8,
                "decay_scale": 0.4,
                "ratio_preset": 2,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.modal.enabled);
        assert!((s1.modal.volume - 0.9).abs() < 1e-5);
        assert!((s1.modal.pan - 0.3).abs() < 1e-5);
        assert!((s1.modal.brightness - 0.8).abs() < 1e-5);
        assert!((s1.modal.decay_scale - 0.4).abs() < 1e-5);
        assert_eq!(s1.modal.ratio_preset, 2);
    }

    #[test]
    fn ratio_preset_clamps_to_valid_range() {
        // 99 is far past the 4-preset table; apply must clamp so
        // the audio thread never indexes out of bounds.
        let s0 = AppState::default();
        let json = serde_json::json!({ "modal": { "ratio_preset": 99 } });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.modal.ratio_preset < MODAL_RATIO_PRESETS);
    }

    #[test]
    fn llm_apply_writes_partial_levels() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "modal": {
                "levels": [1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        for i in 0..MODAL_MODES {
            let expected = if i.is_multiple_of(2) { 1.0 } else { 0.0 };
            assert!((s1.modal.levels[i] - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn shorter_levels_array_leaves_trailing_modes_alone() {
        let s0 = AppState::default();
        let prior_5th = s0.modal.levels[4];
        let json = serde_json::json!({
            "modal": { "levels": [0.5, 0.6, 0.7] }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.modal.levels[0] - 0.5).abs() < 1e-5);
        assert!((s1.modal.levels[1] - 0.6).abs() < 1e-5);
        assert!((s1.modal.levels[2] - 0.7).abs() < 1e-5);
        assert!((s1.modal.levels[4] - prior_5th).abs() < 1e-5);
    }

    #[test]
    fn locked_decay_scale_skipped() {
        let s0 = AppState::default();
        let prior = s0.modal.decay_scale;
        let json = serde_json::json!({ "modal": { "decay_scale": 0.0 } });
        let locked = ["modal.decay_scale".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        assert!((s1.modal.decay_scale - prior).abs() < 1e-5);
    }

    #[test]
    fn sequencer_lane_writes_steps_and_notes() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "modal": {
                "modal_steps": [true, false, true, false],
                "modal_notes": [60, 64, 67, 72],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.sequencer.modal_pattern[0].active);
        assert!(!s1.sequencer.modal_pattern[1].active);
        assert!(s1.sequencer.modal_pattern[2].active);
        assert_eq!(s1.sequencer.modal_pattern[0].note, 60);
        assert_eq!(s1.sequencer.modal_pattern[2].note, 67);
    }
}

#[cfg(test)]
mod modal_module_tests {
    use crate::state::{ModuleKind, rack_scope::parse_module_kind};

    #[test]
    fn label_is_modal() {
        assert_eq!(ModuleKind::ModalVoice.label(), "MODAL");
    }

    #[test]
    fn parses_from_aliases() {
        for alias in [
            "modal", "physical", "bell", "marimba", "tubular", "glass", "struck",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::ModalVoice),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn produces_audio_and_lives_in_voice_zone() {
        use crate::state::Zone;
        let k = ModuleKind::ModalVoice;
        assert!(k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::Voice);
    }
}
