// ─── tests/chiptune_tests.rs ─────────────────────────────────────────────────
// State-level tests for the SID-flavoured chiptune voice.  DSP-
// side tests (silence / audibility / waveform coverage / sync /
// boundedness / release) live next to the voice in
// `audio/dsp/chiptune.rs`.

#[cfg(test)]
mod chiptune_state_tests {
    use crate::state::{
        AppState, CHIPTUNE_FILTER_MODES, CHIPTUNE_WAVEFORMS, ChiptuneState, apply_llm_update,
    };

    #[test]
    fn defaults_are_a_two_osc_lead_with_filter_bypassed() {
        let s = ChiptuneState::default();
        assert!(!s.enabled);
        // Osc 1 saw at full level (the SID-classic lead voice);
        // osc 2 a quieter pulse for unison detune; osc 3 silent.
        assert_eq!(s.osc1.waveform, 0);
        assert!(s.osc1.level > 0.5);
        assert_eq!(s.osc2.waveform, 2); // Pulse
        assert!(s.osc2.level > 0.0 && s.osc2.level < s.osc1.level);
        assert_eq!(s.osc3.level, 0.0);
        // Filter bypassed by default — user dials in deliberately.
        assert_eq!(s.filter_mix, 0.0);
        assert!(!s.ring_mod);
        assert!(!s.sync);
    }

    #[test]
    fn llm_apply_writes_voice_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "chiptune": {
                "enabled": true,
                "volume": 1.0,
                "pan": 0.5,
                "pulse_width": 0.3,
                "filter_cutoff": 0.6,
                "filter_resonance": 0.7,
                "filter_mode": 1,
                "filter_mix": 0.8,
                "ring_mod": true,
                "sync": true,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.chiptune.enabled);
        assert!((s1.chiptune.volume - 1.0).abs() < 1e-5);
        assert!((s1.chiptune.pan - 0.5).abs() < 1e-5);
        assert!((s1.chiptune.pulse_width - 0.3).abs() < 1e-5);
        assert!((s1.chiptune.filter_cutoff - 0.6).abs() < 1e-5);
        assert!((s1.chiptune.filter_resonance - 0.7).abs() < 1e-5);
        assert_eq!(s1.chiptune.filter_mode, 1);
        assert!((s1.chiptune.filter_mix - 0.8).abs() < 1e-5);
        assert!(s1.chiptune.ring_mod);
        assert!(s1.chiptune.sync);
    }

    #[test]
    fn llm_apply_writes_per_osc_fields() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "chiptune": {
                "osc1": { "waveform": 1, "level": 0.4, "attack": 0.1 },
                "osc3": { "waveform": 3, "level": 0.7 }
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert_eq!(s1.chiptune.osc1.waveform, 1); // Triangle
        assert!((s1.chiptune.osc1.level - 0.4).abs() < 1e-5);
        assert!((s1.chiptune.osc1.attack - 0.1).abs() < 1e-5);
        assert_eq!(s1.chiptune.osc3.waveform, 3); // Noise
        assert!((s1.chiptune.osc3.level - 0.7).abs() < 1e-5);
        // Osc 2 unchanged from default.
        assert_eq!(s1.chiptune.osc2.waveform, 2);
    }

    #[test]
    fn waveform_clamps_to_valid_range() {
        // Out-of-range values from the LLM must clamp; the audio
        // thread's match arm uses _ for the noise fallthrough,
        // but the apply layer should never write past the table.
        let s0 = AppState::default();
        let json = serde_json::json!({
            "chiptune": { "osc1": { "waveform": 99 } }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.chiptune.osc1.waveform < CHIPTUNE_WAVEFORMS);
    }

    #[test]
    fn filter_mode_clamps_to_valid_range() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "chiptune": { "filter_mode": 99 } });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.chiptune.filter_mode < CHIPTUNE_FILTER_MODES);
    }

    #[test]
    fn locked_ring_mod_is_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "chiptune": { "ring_mod": true } });
        let locked = ["chiptune.ring_mod".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        assert!(!s1.chiptune.ring_mod, "locked ring_mod should not flip");
    }

    #[test]
    fn sequencer_lane_writes_steps_and_notes() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "chiptune": {
                "chiptune_steps": [true, false, true, false],
                "chiptune_notes": [60, 64, 67, 72],
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.sequencer.chiptune_pattern[0].active);
        assert!(!s1.sequencer.chiptune_pattern[1].active);
        assert!(s1.sequencer.chiptune_pattern[2].active);
        assert_eq!(s1.sequencer.chiptune_pattern[0].note, 60);
        assert_eq!(s1.sequencer.chiptune_pattern[2].note, 67);
    }
}

#[cfg(test)]
mod chiptune_module_tests {
    use crate::state::{ModuleKind, rack_scope::parse_module_kind};

    #[test]
    fn label_is_chiptune() {
        assert_eq!(ModuleKind::ChiptuneVoice.label(), "CHIPTUNE");
    }

    #[test]
    fn parses_from_aliases() {
        for alias in ["chiptune", "sid", "c64", "8bit", "tracker"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::ChiptuneVoice),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn produces_audio_and_lives_in_voice_zone() {
        use crate::state::Zone;
        let k = ModuleKind::ChiptuneVoice;
        assert!(k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::Voice);
    }
}
