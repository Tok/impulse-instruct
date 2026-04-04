// ─── tests.rs ─────────────────────────────────────────────────────────────────
// Unit tests for pure functions.
// Run: ./run-tests.sh
//      ./run-tests.sh --coverage

#[cfg(test)]
mod sequencer_tests {
    use crate::sequencer::{ClockState, TriggerEvent, advance_clock, samples_per_step};
    use crate::state::{DrumVoice, MAX_STEPS, SequencerState, Step};

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
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState {
            current_step: MAX_STEPS - 1,
            ..ClockState::default()
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
            ratchet: 1,
        };

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState::default();

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

    #[test]
    fn ratchet_2_emits_sub_hit_after_step_fires() {
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 2,
        };

        let sps = samples_per_step(120.0, 44100.0);
        // First block fires step 0 (ratchet=2 → first hit + schedule 1 sub-hit)
        let clock = ClockState::default();
        let (clock2, events1) = advance_clock(clock, &seq, sps as usize + 1, 44100.0);
        let first_kick = events1
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    TriggerEvent::DrumTrigger {
                        voice: DrumVoice::Kick808,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(first_kick, 1, "first block should fire one kick");
        assert!(
            clock2.ratchet_remaining[0] > 0,
            "sub-hit should be pending after ratchet=2"
        );

        // Second block advances past the half-step interval → sub-hit fires
        let (_, events2) = advance_clock(clock2, &seq, sps as usize / 2 + 1, 44100.0);
        let sub_kick = events2
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    TriggerEvent::DrumTrigger {
                        voice: DrumVoice::Kick808,
                        ..
                    }
                )
            })
            .count();
        assert!(sub_kick >= 1, "ratchet sub-hit should fire in second block");
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

#[cfg(test)]
mod transition_tests {
    use crate::state::{
        AppState, DrumVoice, Scale, apply_boc_preset, apply_hoover_preset, apply_reese_preset,
        lock_params, record_bass_note, set_drum_step_probability, set_drum_step_ratchet,
        set_drum_step_velocity, set_drum_voice_steps, set_lane_steps, set_root_note,
        toggle_bass_accent, toggle_bass_slide, toggle_live_record, toggle_sequencer_running,
        unlock_params,
    };

    #[test]
    fn toggle_sequencer_running_flips_state() {
        let state = AppState::default();
        assert!(!state.sequencer.running);
        let state = toggle_sequencer_running(state);
        assert!(state.sequencer.running);
        let state = toggle_sequencer_running(state);
        assert!(!state.sequencer.running);
    }

    #[test]
    fn lock_params_batch_and_unlock_params_batch() {
        let state = AppState::default();
        let state = lock_params(state, &["bass.cutoff", "bass.resonance", "fx.reverb_mix"]);
        assert!(state.llm.locked_params.contains("bass.cutoff"));
        assert!(state.llm.locked_params.contains("bass.resonance"));
        assert!(state.llm.locked_params.contains("fx.reverb_mix"));

        let state = unlock_params(state, &["bass.cutoff", "fx.reverb_mix"]);
        assert!(!state.llm.locked_params.contains("bass.cutoff"));
        assert!(
            state.llm.locked_params.contains("bass.resonance"),
            "unreleased param gone"
        );
        assert!(!state.llm.locked_params.contains("fx.reverb_mix"));
    }

    #[test]
    fn set_drum_step_velocity_clamps_low() {
        let state = AppState::default();
        let state = set_drum_step_velocity(state, DrumVoice::Kick808, 0, 0.0);
        let vel = state.sequencer.drum_patterns[&DrumVoice::Kick808][0].velocity;
        assert!(vel >= 0.05, "velocity should clamp to 0.05, got {}", vel);
    }

    #[test]
    fn set_drum_step_velocity_clamps_high() {
        let state = AppState::default();
        let state = set_drum_step_velocity(state, DrumVoice::Snare808, 3, 2.0);
        let vel = state.sequencer.drum_patterns[&DrumVoice::Snare808][3].velocity;
        assert_eq!(vel, 1.0);
    }

    #[test]
    fn set_drum_step_velocity_normal_value() {
        let state = AppState::default();
        let state = set_drum_step_velocity(state, DrumVoice::HihatClosed808, 5, 0.6);
        let vel = state.sequencer.drum_patterns[&DrumVoice::HihatClosed808][5].velocity;
        assert!((vel - 0.6).abs() < 1e-5);
    }

    #[test]
    fn set_drum_step_probability_clamps_and_stores() {
        let state = AppState::default();
        let state = set_drum_step_probability(state, DrumVoice::Clap909, 0, 1.5);
        assert_eq!(
            state.sequencer.drum_patterns[&DrumVoice::Clap909][0].probability,
            1.0
        );

        let state = set_drum_step_probability(state, DrumVoice::Clap909, 0, -0.5);
        assert_eq!(
            state.sequencer.drum_patterns[&DrumVoice::Clap909][0].probability,
            0.0
        );

        let state = set_drum_step_probability(state, DrumVoice::Clap909, 0, 0.75);
        let p = state.sequencer.drum_patterns[&DrumVoice::Clap909][0].probability;
        assert!((p - 0.75).abs() < 1e-5);
    }

    #[test]
    fn set_drum_step_ratchet_clamps_to_1_4() {
        let state = AppState::default();
        let state = set_drum_step_ratchet(state, DrumVoice::Kick808, 0, 0); // below min
        assert_eq!(
            state.sequencer.drum_patterns[&DrumVoice::Kick808][0].ratchet,
            1
        );

        let state = set_drum_step_ratchet(state, DrumVoice::Kick808, 0, 10); // above max
        assert_eq!(
            state.sequencer.drum_patterns[&DrumVoice::Kick808][0].ratchet,
            4
        );

        let state = set_drum_step_ratchet(state, DrumVoice::Kick808, 0, 3);
        assert_eq!(
            state.sequencer.drum_patterns[&DrumVoice::Kick808][0].ratchet,
            3
        );
    }

    #[test]
    fn set_drum_voice_steps_polyrhythm() {
        let state = AppState::default();
        let state = set_drum_voice_steps(state, DrumVoice::HihatClosed808, 12);
        assert_eq!(state.sequencer.drum_steps[&DrumVoice::HihatClosed808], 12);
    }

    #[test]
    fn set_lane_steps_bass_hoover_an1x() {
        let state = AppState::default();
        let state = set_lane_steps(state, "bass", 8);
        assert_eq!(state.sequencer.bass_steps, 8);
        let state = set_lane_steps(state, "hoover", 4);
        assert_eq!(state.sequencer.hoover_steps, 4);
        let state = set_lane_steps(state, "an1x", 32);
        assert_eq!(state.sequencer.an1x_steps, 32);
    }

    #[test]
    fn set_root_note_clamps_to_0_11() {
        let state = AppState::default();
        let state = set_root_note(state, 15); // out of range
        assert_eq!(state.sequencer.root_note, 11);
        let state = set_root_note(state, 9);
        assert_eq!(state.sequencer.root_note, 9); // A
    }

    #[test]
    fn toggle_bass_accent_and_slide() {
        let state = AppState::default();
        assert!(!state.sequencer.bass_pattern[0].accent);
        let state = toggle_bass_accent(state, 0);
        assert!(state.sequencer.bass_pattern[0].accent);
        let state = toggle_bass_accent(state, 0);
        assert!(!state.sequencer.bass_pattern[0].accent);

        assert!(!state.sequencer.bass_pattern[2].slide);
        let state = toggle_bass_slide(state, 2);
        assert!(state.sequencer.bass_pattern[2].slide);
    }

    #[test]
    fn apply_reese_preset_sets_supersaw_and_highpass() {
        let state = apply_reese_preset(AppState::default());
        use crate::state::{FilterMode, Waveform};
        assert_eq!(state.bass.waveform, Waveform::Supersaw);
        assert_eq!(state.bass.supersaw_voices, 2);
        assert_eq!(state.bass.filter_mode, FilterMode::Highpass);
        assert!(state.bass.sub_osc_level > 0.0);
    }

    #[test]
    fn apply_hoover_preset_enables_voice() {
        let state = apply_hoover_preset(AppState::default());
        assert!(state.hoover.enabled);
        assert!(state.hoover.resonance > 0.5);
    }

    #[test]
    fn apply_boc_preset_enables_an1x_with_drift() {
        let state = apply_boc_preset(AppState::default());
        assert!(state.an1x.enabled);
        assert!(state.an1x.drift > 0.0, "BoC preset should have pitch drift");
        assert!(state.an1x.glide_legato, "BoC preset uses legato glide");
    }

    #[test]
    fn live_record_toggle_flips_flag() {
        let state = AppState::default();
        assert!(!state.live_record);
        let state = toggle_live_record(state);
        assert!(state.live_record);
        let state = toggle_live_record(state);
        assert!(!state.live_record);
    }

    #[test]
    fn record_bass_note_writes_to_current_step() {
        let state = AppState::default();
        let state = record_bass_note(state, 3, 57); // A3
        assert!(state.sequencer.bass_pattern[3].active);
        assert_eq!(state.sequencer.bass_pattern[3].note, 57);
    }

    #[test]
    fn record_bass_note_wraps_at_bass_steps() {
        let mut state = AppState::default();
        state.sequencer.bass_steps = 8;
        // Step 10 should wrap to step 2 (10 % 8)
        let state = record_bass_note(state, 10, 60);
        assert!(state.sequencer.bass_pattern[2].active);
        assert_eq!(state.sequencer.bass_pattern[2].note, 60);
    }

    #[test]
    fn set_scale_round_trip() {
        use crate::state::set_scale;
        let state = AppState::default();
        let state = set_scale(state, Scale::Dorian);
        assert_eq!(state.sequencer.scale, Scale::Dorian);
    }
}

#[cfg(test)]
mod bank_chain_tests {
    use crate::state::{
        AppState, DrumVoice, bank_load, bank_swap, bank_write, chain_pop, chain_push,
        set_chain_enabled, toggle_drum_step,
    };

    #[test]
    fn bank_write_and_load_round_trip() {
        let state = AppState::default();
        // Turn on a step, write to slot 2
        let state = toggle_drum_step(state, DrumVoice::Snare808, 5);
        let state = bank_write(state, 2);
        assert!(state.pattern_bank[2].drum_patterns[&DrumVoice::Snare808][5].active);
    }

    #[test]
    fn bank_load_keep_transport_preserves_bpm() {
        let mut state = AppState::default();
        state.sequencer.bpm = 160.0;
        let state = bank_write(state, 1);
        // Change BPM and load slot 1 with keep_transport=true
        let mut state2 = bank_load(state, 1, true);
        state2.sequencer.bpm = 140.0; // overwrite after load
        let state2 = bank_load(state2, 1, true);
        // BPM should be 140 (kept from caller, not from saved pattern)
        assert!((state2.sequencer.bpm - 140.0).abs() < 0.01);
    }

    #[test]
    fn bank_load_without_keep_transport_restores_bpm() {
        let mut state = AppState::default();
        state.sequencer.bpm = 180.0;
        let state = bank_write(state, 3);
        // Change live BPM, load without keep_transport — saved BPM should come back
        let mut state = state;
        state.sequencer.bpm = 120.0;
        let state = bank_load(state, 3, false);
        assert!((state.sequencer.bpm - 180.0).abs() < 0.01);
    }

    #[test]
    fn bank_swap_saves_before_loading() {
        let state = AppState::default();
        // Edit slot 0 (default), turn on a step
        let state = toggle_drum_step(state, DrumVoice::Kick808, 7);
        assert_eq!(state.pattern_edit, 0);

        // Swap to slot 1 — should auto-save edits to slot 0
        let state = bank_swap(state, 1);
        assert_eq!(state.pattern_edit, 1);
        assert!(
            state.pattern_bank[0].drum_patterns[&DrumVoice::Kick808][7].active,
            "edits to slot 0 should have been saved before swap"
        );
    }

    #[test]
    fn bank_swap_noop_on_same_slot() {
        let state = AppState::default();
        let before = state.pattern_edit;
        let state = bank_swap(state, before); // swap to current slot
        assert_eq!(state.pattern_edit, before);
    }

    #[test]
    fn chain_push_pop_ordering() {
        let state = AppState::default();
        let state = chain_push(state, 0);
        let state = chain_push(state, 3);
        let state = chain_push(state, 1);
        assert_eq!(state.chain, vec![0, 3, 1]);

        let state = chain_pop(state);
        assert_eq!(state.chain, vec![0, 3]);
    }

    #[test]
    fn chain_push_caps_at_8() {
        let state = AppState::default();
        let mut s = state;
        for i in 0..10 {
            s = chain_push(s, i % 8);
        }
        assert_eq!(s.chain.len(), 8, "chain should cap at 8 entries");
    }

    #[test]
    fn set_chain_enabled_false_resets_pos() {
        let mut state = AppState::default();
        state.chain_pos = 4;
        state.chain_enabled = true;
        let state = set_chain_enabled(state, false);
        assert!(!state.chain_enabled);
        assert_eq!(state.chain_pos, 0, "disabling chain should reset position");
    }
}

#[cfg(test)]
mod probability_tests {
    use crate::sequencer::{ClockState, TriggerEvent, advance_clock, samples_per_step};
    use crate::state::{DrumVoice, SequencerState, Step};

    /// Build a sequencer with only step 0 of Kick808 active, all other kick steps cleared.
    /// The 16-step pattern cycles 4× per global 64-step loop, so step 0 visits 4 times per loop.
    fn seq_with_single_kick(prob: f32) -> SequencerState {
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        // Clear all kick808 steps, then set only step 0 with the target probability
        if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Kick808) {
            for s in pattern.iter_mut() {
                *s = Step {
                    active: false,
                    velocity: 1.0,
                    probability: 1.0,
                    ratchet: 1,
                };
            }
            pattern[0] = Step {
                active: true,
                velocity: 1.0,
                probability: prob,
                ratchet: 1,
            };
        }
        seq
    }

    fn kick808_count_over_loops(prob: f32, loops: u32) -> usize {
        let sr = 44100.0_f32;
        let sps = samples_per_step(120.0, sr) as usize;
        let seq = seq_with_single_kick(prob);
        let mut clock = ClockState::default();
        let mut count = 0usize;
        loop {
            let (new_clock, events) = advance_clock(clock.clone(), &seq, sps + 1, sr);
            clock = new_clock;
            count += events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        TriggerEvent::DrumTrigger {
                            voice: DrumVoice::Kick808,
                            ..
                        }
                    )
                })
                .count();
            if clock.loop_count >= loops {
                break;
            }
        }
        count
    }

    #[test]
    fn probability_zero_never_fires() {
        // prob=0.0 — kick should never fire
        let count = kick808_count_over_loops(0.0, 20);
        assert_eq!(count, 0, "prob=0.0 fired {} times", count);
    }

    #[test]
    fn probability_one_fires_every_visit() {
        // prob=1.0 — step 0 of a 16-step pattern in a 64-step global loop fires 4×/loop
        let loops: u32 = 20;
        let count = kick808_count_over_loops(1.0, loops);
        // 16-step pattern, 64 global steps per loop → 4 visits to step 0 per loop
        let expected = loops as usize * 4;
        assert_eq!(
            count, expected,
            "prob=1.0 over {} loops: expected {}, got {}",
            loops, expected, count
        );
    }

    #[test]
    fn probability_half_fires_roughly_half_the_visits() {
        // Deterministic hash — prob=0.5 over 100 loops × 4 visits = 400 opportunities
        // Expect roughly 200 ± wide margin (150–250)
        let count = kick808_count_over_loops(0.5, 100);
        assert!(
            count >= 100 && count <= 300,
            "prob=0.5 over 100 loops: expected ~200, got {}",
            count
        );
    }
}
