// ─── tests.rs ─────────────────────────────────────────────────────────────────
// Unit tests for pure functions.
// Run: ./run-tests.sh
//      ./run-tests.sh --coverage

#[cfg(test)]
mod sequencer_tests {
    use crate::sequencer::{ClockState, advance_clock, samples_per_step};
    use crate::state::{DrumVoice, SequencerState, Step};

    #[test]
    fn samples_per_step_at_120bpm_44100hz() {
        // 120 BPM → 2 beats/s → 8 16th-notes/s → 5512.5 samples/step
        let sps = samples_per_step(120.0, 44100.0);
        let expected = 44100.0 * 60.0 / (120.0 * 4.0);
        assert!((sps - expected).abs() < 0.01, "got {}", sps);
    }

    #[test]
    fn advance_clock_does_not_tick_when_stopped() {
        let seq = SequencerState {
            running: false,
            ..SequencerState::default()
        };
        let clock = ClockState::default();
        let (new_clock, events) = advance_clock(clock, &seq, 512, 44100.0);
        assert!(events.is_empty());
        assert_eq!(new_clock.current_step, 0);
    }

    #[test]
    fn advance_clock_wraps_at_max_steps() {
        // current_step is a global tick counter that wraps at MAX_STEPS (64).
        // Per-voice lengths are applied as modulo at trigger time.
        use crate::state::MAX_STEPS;
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState {
            sample_accumulator: 0.0,
            current_step: MAX_STEPS - 1,
            loop_count: 0,
            gate_counter: 0,
            gate_counter_hoover: 0,
            gate_counter_an1x: 0,
        };

        let (new_clock, _) = advance_clock(clock, &seq, sps + 1, 44100.0);
        assert_eq!(
            new_clock.current_step, 0,
            "should wrap from MAX_STEPS-1 to 0"
        );
    }

    #[test]
    fn advance_clock_fires_active_steps() {
        use crate::sequencer::TriggerEvent;

        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        // Activate step 1 of kick 808
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
        };

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState {
            sample_accumulator: 0.0,
            current_step: 0,
            loop_count: 0,
            gate_counter: 0,
            gate_counter_hoover: 0,
            gate_counter_an1x: 0,
        };

        let (_, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
        let has_kick = events.iter().any(|e| {
            matches!(
                e,
                TriggerEvent::DrumTrigger {
                    voice: DrumVoice::Kick808,
                    ..
                }
            )
        });
        assert!(has_kick, "expected kick trigger, got {:?}", events);
    }
}

#[cfg(test)]
mod state_tests {
    use crate::state::{AppState, DrumVoice, apply_llm_update, lock_param, toggle_drum_step};

    #[test]
    fn apply_llm_update_sets_cutoff() {
        let state = AppState::default();
        let update = serde_json::json!({ "bass": { "cutoff": 0.9 } });
        let next = apply_llm_update(state, &update);
        assert!((next.bass.cutoff - 0.9).abs() < 1e-4);
    }

    #[test]
    fn apply_llm_update_clamps_to_unit_range() {
        let state = AppState::default();
        let update = serde_json::json!({ "bass": { "cutoff": 1.5 } });
        let next = apply_llm_update(state, &update);
        assert!(next.bass.cutoff <= 1.0);
    }

    #[test]
    fn locked_param_not_overwritten_by_llm() {
        let state = AppState::default();
        let original_cutoff = state.bass.cutoff;
        let state = lock_param(state, "bass.cutoff");

        let update = serde_json::json!({ "bass": { "cutoff": 0.99 } });
        let next = apply_llm_update(state, &update);
        assert_eq!(
            next.bass.cutoff, original_cutoff,
            "locked param should be untouched"
        );
    }

    #[test]
    fn toggle_drum_step_flips_active() {
        let state = AppState::default();
        // Step 1 is silent in the default starter pattern — use it for a clean toggle test
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Kick808][1].active);

        let state = toggle_drum_step(state, DrumVoice::Kick808, 1);
        assert!(state.sequencer.drum_patterns[&DrumVoice::Kick808][1].active);

        let state = toggle_drum_step(state, DrumVoice::Kick808, 1);
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Kick808][1].active);
    }

    #[test]
    fn bpm_update_via_llm() {
        // BPM is locked by default — must explicitly unlock first
        let state = crate::state::unlock_param(AppState::default(), "sequencer.bpm");
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
        assert!(
            prompt.contains("Output ONLY valid JSON")
                || prompt.contains("ONLY valid JSON")
                || prompt.contains("Output JSON only")
                || prompt.contains("Output ONLY"),
            "prompt should instruct JSON-only output"
        );
    }

    #[test]
    fn build_system_prompt_reflects_current_cutoff() {
        let mut state = AppState::default();
        state.bass.cutoff = 0.5; // exact f32 representation
        let prompt = build_system_prompt(&state);
        assert!(
            prompt.contains("0.5"),
            "prompt should embed current cutoff value"
        );
    }

    #[test]
    fn build_system_prompt_lists_locked_params() {
        use crate::state::lock_param;
        let state = lock_param(AppState::default(), "bass.cutoff");
        let prompt = build_system_prompt(&state);
        assert!(
            prompt.contains("bass.cutoff"),
            "locked params should appear in prompt"
        );
    }

    #[test]
    fn param_json_schema_has_bass_cutoff_range() {
        let schema = param_json_schema();
        let min = schema["properties"]["bass"]["properties"]["cutoff"]["minimum"]
            .as_f64()
            .unwrap();
        let max = schema["properties"]["bass"]["properties"]["cutoff"]["maximum"]
            .as_f64()
            .unwrap();
        assert_eq!(min, 0.0);
        assert_eq!(max, 1.0);
    }

    #[test]
    fn param_json_schema_bpm_range_is_40_to_250() {
        let schema = param_json_schema();
        let min = schema["properties"]["sequencer"]["properties"]["bpm"]["minimum"]
            .as_f64()
            .unwrap();
        let max = schema["properties"]["sequencer"]["properties"]["bpm"]["maximum"]
            .as_f64()
            .unwrap();
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
mod instruction_tests {
    use crate::llm::instructions::InstructionSet;
    use crate::llm::mock_response;

    /// Recursively verify that every key present in `expected` also appears in
    /// `actual` (ignores `_comment` and does not assert exact leaf values).
    fn assert_keys_present(expected: &serde_json::Value, actual: &serde_json::Value, path: &str) {
        if let Some(obj) = expected.as_object() {
            for (k, v) in obj {
                if k == "_comment" {
                    continue;
                }
                let child_path = format!("{}.{}", path, k);
                assert!(
                    actual.get(k).is_some(),
                    "instruction '{}': expected key '{}' in mock output, but it was missing\noutput: {}",
                    path,
                    child_path,
                    actual
                );
                assert_keys_present(v, &actual[k], &child_path);
            }
        }
    }

    #[test]
    fn instruction_set_loads_and_is_non_empty() {
        let set = InstructionSet::get();
        assert!(
            set.len() > 0,
            "instruction set should have at least one entry"
        );
    }

    #[test]
    fn find_best_match_returns_none_for_empty_prompt() {
        let set = InstructionSet::get();
        // A prompt with no recognisable keywords should not match any instruction
        assert!(set.find_best_match("xyzzy blorple quux").is_none());
    }

    #[test]
    fn remove_clap_matches_negation_prompts() {
        let set = InstructionSet::get();
        for prompt in &["remove claps", "no claps", "mute clap", "clap off"] {
            let m = set.find_best_match(prompt);
            assert!(m.is_some(), "no match for '{}'", prompt);
            assert_eq!(
                m.unwrap().id,
                "remove_clap",
                "prompt '{}' should match 'remove_clap', got '{}'",
                prompt,
                m.unwrap().id
            );
        }
    }

    /// For every instruction, use its *first keyword* as a test prompt and
    /// verify that the mock output contains all the expected parameter keys.
    #[test]
    fn all_instructions_produce_expected_param_keys() {
        let set = InstructionSet::get();
        for inst in set.iter() {
            let test_prompt = inst.keywords.first().expect("instruction has no keywords");
            let result = mock_response(test_prompt, 0.5)
                .unwrap_or_else(|e| panic!("mock_response failed for '{}': {}", test_prompt, e));
            let output = result
                .param_update
                .unwrap_or_else(|| panic!("no param_update for instruction '{}'", inst.id));
            assert_keys_present(&inst.params, &output, &inst.id);
        }
    }

    /// Spot-check a few critical instructions by name.
    #[test]
    fn remove_instructions_emit_all_false_arrays() {
        let set = InstructionSet::get();
        let removal_ids = [
            "remove_clap",
            "remove_kick",
            "remove_hihat_a",
            "remove_snare_a",
        ];
        for id in removal_ids {
            let inst = set
                .iter()
                .find(|i| i.id == id)
                .unwrap_or_else(|| panic!("instruction '{}' not found", id));
            // Every array in params should be all-false
            if let Some(seq) = inst.params.get("sequencer").and_then(|s| s.as_object()) {
                for (field, val) in seq {
                    if let Some(arr) = val.as_array() {
                        assert!(
                            arr.iter().all(|v| v == &serde_json::json!(false)),
                            "instruction '{}', field '{}' should be all false",
                            id,
                            field
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod expand_steps_tests {
    use crate::state::{AppState, DrumVoice, expand_sequencer_steps, toggle_drum_step};

    #[test]
    fn expand_tiles_drum_pattern_into_new_slots() {
        // Snare808 starts silent; turn on step 0 and step 3, then expand 16 → 32
        let state = AppState::default();
        let state = toggle_drum_step(state, DrumVoice::Snare808, 0);
        let state = toggle_drum_step(state, DrumVoice::Snare808, 3);
        assert!(state.sequencer.drum_patterns[&DrumVoice::Snare808][0].active);
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Snare808][16].active);

        let state = expand_sequencer_steps(state, 32);
        assert_eq!(state.sequencer.steps, 32);
        // Step 16 should mirror step 0 (active)
        assert!(
            state.sequencer.drum_patterns[&DrumVoice::Snare808][16].active,
            "step 16 should be tiled from step 0"
        );
        // Step 19 should mirror step 3 (active)
        assert!(
            state.sequencer.drum_patterns[&DrumVoice::Snare808][19].active,
            "step 19 should be tiled from step 3"
        );
        // Step 17 should mirror step 1 (inactive)
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Snare808][17].active);
    }

    #[test]
    fn expand_16_to_64_tiles_four_copies() {
        let state = AppState::default();
        // Default has kick on steps 0, 4, 8, 12
        let state = expand_sequencer_steps(state, 64);
        assert_eq!(state.sequencer.steps, 64);
        let kick = &state.sequencer.drum_patterns[&DrumVoice::Kick808];
        // Each bank of 16 should repeat the same pattern
        for bank in 0..4 {
            assert!(kick[bank * 16].active, "kick missing at step {}", bank * 16);
            assert!(
                kick[bank * 16 + 4].active,
                "kick missing at step {}",
                bank * 16 + 4
            );
        }
    }

    #[test]
    fn shrink_does_not_tile_or_erase() {
        // Expand to 32, set step 20, shrink to 16 — step 20 stays in memory
        let state = AppState::default();
        let state = expand_sequencer_steps(state, 32);
        let state = toggle_drum_step(state, DrumVoice::Snare808, 20);
        assert!(state.sequencer.drum_patterns[&DrumVoice::Snare808][20].active);

        let mut state = state;
        state.sequencer.steps = 16; // shrink directly (UI minus button)
        assert_eq!(state.sequencer.steps, 16);
        // Data above step 16 is preserved (hidden but not lost)
        assert!(state.sequencer.drum_patterns[&DrumVoice::Snare808][20].active);
    }

    #[test]
    fn expand_to_same_count_is_noop() {
        let state = AppState::default();
        let before = state.sequencer.drum_patterns[&DrumVoice::Kick808].clone();
        let state = expand_sequencer_steps(state, 16);
        assert_eq!(state.sequencer.steps, 16);
        assert_eq!(state.sequencer.drum_patterns[&DrumVoice::Kick808], before);
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

#[cfg(test)]
mod music_theory_tests {
    use crate::state::{Scale, note_in_scale, scale_degree, scale_notes, snap_to_scale};

    #[test]
    fn a_minor_contains_expected_notes() {
        // A natural minor: A B C D E F G — MIDI offsets from A(9): 0,2,3,5,7,8,10
        assert!(note_in_scale(45, 9, Scale::NaturalMinor)); // A2
        assert!(note_in_scale(48, 9, Scale::NaturalMinor)); // C3
        assert!(note_in_scale(52, 9, Scale::NaturalMinor)); // E3
        assert!(!note_in_scale(46, 9, Scale::NaturalMinor)); // A#2 — not in A minor
    }

    #[test]
    fn tonic_is_degree_zero() {
        assert_eq!(scale_degree(45, 9, Scale::NaturalMinor), Some(0)); // A2 in A minor
        assert_eq!(scale_degree(57, 9, Scale::NaturalMinor), Some(0)); // A3
        assert_eq!(scale_degree(46, 9, Scale::NaturalMinor), None); // A#2 not in scale
    }

    #[test]
    fn snap_keeps_in_scale_note_unchanged() {
        // C3 (48) is in C major — should snap to itself
        assert_eq!(snap_to_scale(48, 0, Scale::Major), 48);
    }

    #[test]
    fn snap_moves_out_of_scale_note_to_nearest() {
        // C# (49) is not in C major — should snap to C (48) or D (50), whichever closer
        let snapped = snap_to_scale(49, 0, Scale::Major);
        assert!(snapped == 48 || snapped == 50);
    }

    #[test]
    fn chromatic_scale_contains_all_notes() {
        let notes = scale_notes(0, Scale::Chromatic);
        assert_eq!(notes.len(), 128);
    }

    #[test]
    fn pentatonic_has_five_notes_per_octave() {
        let notes = scale_notes(0, Scale::Pentatonic);
        // 5 notes per octave × ~10.67 octaves in 0-127 = ~53-54 notes
        assert!(notes.len() >= 50 && notes.len() <= 60);
    }

    #[test]
    fn llm_update_snaps_bass_notes_when_enabled() {
        use crate::state::{AppState, apply_llm_update};
        let mut state = AppState::default();
        state.sequencer.root_note = 0; // C
        state.sequencer.scale = Scale::Major;
        state.sequencer.scale_snap = true;
        // C# (49) should snap to C (48) or D (50) in C major
        let update = serde_json::json!({ "sequencer": { "bass_notes": [49] } });
        let new_state = apply_llm_update(state, &update);
        let note = new_state.sequencer.bass_pattern[0].note;
        assert!(
            note == 48 || note == 50,
            "snapped to {note}, expected 48 or 50"
        );
    }

    #[test]
    fn llm_update_leaves_notes_unsnapped_when_disabled() {
        use crate::state::{AppState, apply_llm_update};
        let mut state = AppState::default();
        state.sequencer.root_note = 0;
        state.sequencer.scale = Scale::Major;
        state.sequencer.scale_snap = false;
        let update = serde_json::json!({ "sequencer": { "bass_notes": [49] } });
        let new_state = apply_llm_update(state, &update);
        assert_eq!(new_state.sequencer.bass_pattern[0].note, 49);
    }
}

#[cfg(test)]
mod euclidean_tests {
    use crate::sequencer::euclidean_rhythm;

    #[test]
    fn euclid_pulse_count_matches() {
        for (pulses, steps) in [(4, 16), (5, 8), (3, 7), (1, 4), (7, 7), (0, 8)] {
            let r = euclidean_rhythm(pulses, steps);
            assert_eq!(r.len(), steps, "len mismatch {}/{}", pulses, steps);
            let count = r.iter().filter(|&&x| x).count();
            assert_eq!(count, pulses, "pulse count mismatch {}/{}", pulses, steps);
        }
    }

    #[test]
    fn euclid_edge_cases() {
        assert_eq!(euclidean_rhythm(0, 8), vec![false; 8]);
        assert_eq!(euclidean_rhythm(8, 8), vec![true; 8]);
        assert!(euclidean_rhythm(0, 0).is_empty());
    }

    #[test]
    fn euclid_4_in_16_is_four_on_floor() {
        // Classic: 4-on-the-floor places pulses at indices 0, 4, 8, 12.
        let r = euclidean_rhythm(4, 16);
        assert!(
            r[0] && r[4] && r[8] && r[12],
            "4-on-floor placement wrong: {:?}",
            r
        );
    }
}
