// ─── tests.rs ─────────────────────────────────────────────────────────────────
// Unit tests for pure functions.
// Run: ./run-tests.sh
//      ./run-tests.sh --coverage

#[cfg(test)]
mod sequencer_tests {
    use crate::sequencer::{advance_clock, ClockState, samples_per_step};
    use crate::state::{AppState, DrumVoice, SequencerState, Step};

    #[test]
    fn samples_per_step_at_120bpm_44100hz() {
        // 120 BPM → 2 beats/s → 8 16th-notes/s → 5512.5 samples/step
        let sps = samples_per_step(120.0, 44100.0);
        let expected = 44100.0 * 60.0 / (120.0 * 4.0);
        assert!((sps - expected).abs() < 0.01, "got {}", sps);
    }

    #[test]
    fn advance_clock_does_not_tick_when_stopped() {
        let seq = SequencerState { running: false, ..SequencerState::default() };
        let clock = ClockState::default();
        let (new_clock, events) = advance_clock(clock, &seq, 512, 44100.0);
        assert!(events.is_empty());
        assert_eq!(new_clock.current_step, 0);
    }

    #[test]
    fn advance_clock_wraps_at_step_count() {
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        seq.steps = 4; // 4-step pattern

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState { sample_accumulator: 0.0, current_step: 3, gate_counter: 0 };

        // Advance by slightly more than one step
        let (new_clock, _) = advance_clock(clock, &seq, sps + 1, 44100.0);
        assert_eq!(new_clock.current_step, 0, "should wrap from 3 to 0");
    }

    #[test]
    fn advance_clock_fires_active_steps() {
        use crate::sequencer::TriggerEvent;

        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        // Activate step 1 of kick 808
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] =
            Step { active: true, velocity: 1.0 };

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState { sample_accumulator: 0.0, current_step: 0, gate_counter: 0 };

        let (_, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
        let has_kick = events.iter().any(|e| matches!(e, TriggerEvent::DrumTrigger { voice: DrumVoice::Kick808, .. }));
        assert!(has_kick, "expected kick trigger, got {:?}", events);
    }
}

#[cfg(test)]
mod state_tests {
    use crate::state::{AppState, DrumVoice, apply_llm_update, lock_param, toggle_drum_step};

    #[test]
    fn apply_llm_update_sets_cutoff() {
        let state = AppState::default();
        let update = serde_json::json!({ "tb303": { "cutoff": 0.9 } });
        let next = apply_llm_update(state, &update);
        assert!((next.tb303.cutoff - 0.9).abs() < 1e-4);
    }

    #[test]
    fn apply_llm_update_clamps_to_unit_range() {
        let state = AppState::default();
        let update = serde_json::json!({ "tb303": { "cutoff": 1.5 } });
        let next = apply_llm_update(state, &update);
        assert!(next.tb303.cutoff <= 1.0);
    }

    #[test]
    fn locked_param_not_overwritten_by_llm() {
        let state = AppState::default();
        let original_cutoff = state.tb303.cutoff;
        let state = lock_param(state, "tb303.cutoff");

        let update = serde_json::json!({ "tb303": { "cutoff": 0.99 } });
        let next = apply_llm_update(state, &update);
        assert_eq!(next.tb303.cutoff, original_cutoff, "locked param should be untouched");
    }

    #[test]
    fn toggle_drum_step_flips_active() {
        let state = AppState::default();
        // Step 0 starts inactive (silent default)
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Kick808][0].active);

        let state = toggle_drum_step(state, DrumVoice::Kick808, 0);
        assert!(state.sequencer.drum_patterns[&DrumVoice::Kick808][0].active);

        let state = toggle_drum_step(state, DrumVoice::Kick808, 0);
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Kick808][0].active);
    }

    #[test]
    fn bpm_update_via_llm() {
        let state = AppState::default();
        let update = serde_json::json!({ "sequencer": { "bpm": 175.0 } });
        let next = apply_llm_update(state, &update);
        assert!((next.sequencer.bpm - 175.0).abs() < 0.01);
    }

    #[test]
    fn bpm_clamped_to_valid_range() {
        let state = AppState::default();
        let update = serde_json::json!({ "sequencer": { "bpm": 999.0 } });
        let next = apply_llm_update(state, &update);
        assert!(next.sequencer.bpm <= 250.0, "bpm should be clamped");
    }
}

#[cfg(test)]
mod prompt_tests {
    use crate::llm::{build_system_prompt, param_json_schema};
    use crate::state::AppState;

    #[test]
    fn build_system_prompt_contains_json_only_instruction() {
        let state = AppState::default();
        let prompt = build_system_prompt(&state);
        assert!(prompt.contains("Output JSON only"), "prompt should instruct JSON-only output");
    }

    #[test]
    fn build_system_prompt_reflects_current_cutoff() {
        let mut state = AppState::default();
        state.tb303.cutoff = 0.5; // exact f32 representation
        let prompt = build_system_prompt(&state);
        assert!(prompt.contains("0.5"), "prompt should embed current cutoff value");
    }

    #[test]
    fn build_system_prompt_lists_locked_params() {
        use crate::state::lock_param;
        let state = lock_param(AppState::default(), "tb303.cutoff");
        let prompt = build_system_prompt(&state);
        assert!(prompt.contains("tb303.cutoff"), "locked params should appear in prompt");
    }

    #[test]
    fn param_json_schema_has_tb303_cutoff_range() {
        let schema = param_json_schema();
        let min = schema["properties"]["tb303"]["properties"]["cutoff"]["minimum"]
            .as_f64().unwrap();
        let max = schema["properties"]["tb303"]["properties"]["cutoff"]["maximum"]
            .as_f64().unwrap();
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn param_json_schema_bpm_range_is_40_to_250() {
        let schema = param_json_schema();
        let min = schema["properties"]["sequencer"]["properties"]["bpm"]["minimum"]
            .as_f64().unwrap();
        let max = schema["properties"]["sequencer"]["properties"]["bpm"]["maximum"]
            .as_f64().unwrap();
        assert_eq!(min, 40.0);
        assert_eq!(max, 250.0);
    }

    #[test]
    fn param_json_schema_rejects_additional_properties() {
        let schema = param_json_schema();
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    }
}

#[cfg(test)]
mod dsp_tests {
    use crate::audio::dsp::midi_to_hz;

    #[test]
    fn midi_note_69_is_a440() {
        assert!((midi_to_hz(69) - 440.0).abs() < 0.01);
    }

    #[test]
    fn midi_note_60_is_middle_c() {
        assert!((midi_to_hz(60) - 261.626).abs() < 0.1);
    }

    #[test]
    fn midi_note_octave_doubles_frequency() {
        let c4 = midi_to_hz(60);
        let c5 = midi_to_hz(72);
        assert!((c5 / c4 - 2.0).abs() < 0.001);
    }
}
