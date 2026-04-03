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
        assert_eq!(next.bass.cutoff, original_cutoff, "locked param should be untouched");
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
        assert!(prompt.contains("Output ONLY valid JSON") || prompt.contains("ONLY valid JSON") || prompt.contains("Output JSON only") || prompt.contains("Output ONLY"),
            "prompt should instruct JSON-only output");
    }

    #[test]
    fn build_system_prompt_reflects_current_cutoff() {
        let mut state = AppState::default();
        state.bass.cutoff = 0.5; // exact f32 representation
        let prompt = build_system_prompt(&state);
        assert!(prompt.contains("0.5"), "prompt should embed current cutoff value");
    }

    #[test]
    fn build_system_prompt_lists_locked_params() {
        use crate::state::lock_param;
        let state = lock_param(AppState::default(), "bass.cutoff");
        let prompt = build_system_prompt(&state);
        assert!(prompt.contains("bass.cutoff"), "locked params should appear in prompt");
    }

    #[test]
    fn param_json_schema_has_bass_cutoff_range() {
        let schema = param_json_schema();
        let min = schema["properties"]["bass"]["properties"]["cutoff"]["minimum"]
            .as_f64().unwrap();
        let max = schema["properties"]["bass"]["properties"]["cutoff"]["maximum"]
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
mod instruction_tests {
    use crate::llm::instructions::InstructionSet;
    use crate::llm::mock_response;

    /// Recursively verify that every key present in `expected` also appears in
    /// `actual` (ignores `_comment` and does not assert exact leaf values).
    fn assert_keys_present(expected: &serde_json::Value, actual: &serde_json::Value, path: &str) {
        if let Some(obj) = expected.as_object() {
            for (k, v) in obj {
                if k == "_comment" { continue; }
                let child_path = format!("{}.{}", path, k);
                assert!(
                    actual.get(k).is_some(),
                    "instruction '{}': expected key '{}' in mock output, but it was missing\noutput: {}",
                    path, child_path, actual
                );
                assert_keys_present(v, &actual[k], &child_path);
            }
        }
    }

    #[test]
    fn instruction_set_loads_and_is_non_empty() {
        let set = InstructionSet::get();
        assert!(set.len() > 0, "instruction set should have at least one entry");
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
                m.unwrap().id, "remove_clap",
                "prompt '{}' should match 'remove_clap', got '{}'",
                prompt, m.unwrap().id
            );
        }
    }

    /// For every instruction, use its *first keyword* as a test prompt and
    /// verify that the mock output contains all the expected parameter keys.
    #[test]
    fn all_instructions_produce_expected_param_keys() {
        let set = InstructionSet::get();
        for inst in set.iter() {
            let test_prompt = inst.keywords.first()
                .expect("instruction has no keywords");
            let result = mock_response(test_prompt, 0.5)
                .unwrap_or_else(|e| panic!("mock_response failed for '{}': {}", test_prompt, e));
            let output = result.param_update
                .unwrap_or_else(|| panic!("no param_update for instruction '{}'", inst.id));
            assert_keys_present(&inst.params, &output, &inst.id);
        }
    }

    /// Spot-check a few critical instructions by name.
    #[test]
    fn remove_instructions_emit_all_false_arrays() {
        let set = InstructionSet::get();
        let removal_ids = ["remove_clap", "remove_kick", "remove_hihat_a", "remove_snare_a"];
        for id in removal_ids {
            let inst = set.iter().find(|i| i.id == id)
                .unwrap_or_else(|| panic!("instruction '{}' not found", id));
            // Every array in params should be all-false
            if let Some(seq) = inst.params.get("sequencer").and_then(|s| s.as_object()) {
                for (field, val) in seq {
                    if let Some(arr) = val.as_array() {
                        assert!(
                            arr.iter().all(|v| v == &serde_json::json!(false)),
                            "instruction '{}', field '{}' should be all false",
                            id, field
                        );
                    }
                }
            }
        }
    }
}

// ─── LLM response tests ───────────────────────────────────────────────────────
// Fuzzy assertions over mock_response output.
// These verify that the mock (and by proxy the real model via system prompt)
// responds to prompts in the expected direction — not exact values, but:
//   • directional: acid → resonance up, darker → cutoff down
//   • clearing: remove kick → all-false array
//   • schema: no unknown keys, numeric params in 0.0–1.0, BPM in 40–250
//   • comment: every response includes a _comment field

#[cfg(test)]
mod llm_tests {
    use crate::llm::mock_response;
    use serde_json::Value;

    /// Navigate a dot-path (e.g. "bass.resonance") into a JSON object.
    fn at<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
        path.split('.').fold(Some(json), |v, k| v?.get(k))
    }

    fn num(json: &Value, path: &str) -> f64 {
        at(json, path)
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| panic!("expected numeric value at '{}'\nresponse: {}", path, json))
    }

    fn run(prompt: &str) -> Value {
        mock_response(prompt, 0.5)
            .unwrap_or_else(|e| panic!("mock_response failed for '{}': {}", prompt, e))
            .param_update
            .unwrap_or_else(|| panic!("no param_update for '{}'", prompt))
    }

    fn assert_all_false(json: &Value, path: &str) {
        let arr = at(json, path)
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("expected array at '{}'\nresponse: {}", path, json));
        assert!(
            arr.iter().all(|v| v == &serde_json::json!(false)),
            "expected all-false at '{}', got {:?}", path, arr
        );
    }

    // ── Directional assertions ─────────────────────────────────────────────────

    #[test]
    fn acid_raises_resonance() {
        let j = run("more acid");
        assert!(num(&j, "bass.resonance") >= 0.6,
            "acid should raise resonance to >= 0.6, got {}", num(&j, "bass.resonance"));
    }

    #[test]
    fn acid_raises_env_mod() {
        let j = run("acid squelch");
        assert!(at(&j, "bass.env_mod").is_some(), "acid should include bass.env_mod");
        assert!(num(&j, "bass.env_mod") >= 0.5,
            "acid env_mod should be >= 0.5, got {}", num(&j, "bass.env_mod"));
    }

    #[test]
    fn darker_lowers_cutoff() {
        let j = run("make it darker");
        assert!(num(&j, "bass.cutoff") <= 0.35,
            "darker should lower cutoff to <= 0.35, got {}", num(&j, "bass.cutoff"));
    }

    #[test]
    fn add_reverb_raises_reverb_mix() {
        let j = run("add more reverb");
        assert!(at(&j, "fx.reverb_mix").is_some(), "reverb prompt should set fx.reverb_mix");
        assert!(num(&j, "fx.reverb_mix") >= 0.1,
            "reverb_mix should be > 0, got {}", num(&j, "fx.reverb_mix"));
    }

    #[test]
    fn remove_reverb_zeroes_mix() {
        let j = run("remove reverb");
        assert!(num(&j, "fx.reverb_mix") < 0.01,
            "remove reverb should zero reverb_mix, got {}", num(&j, "fx.reverb_mix"));
    }

    #[test]
    fn harder_raises_distortion_somewhere() {
        let j = run("make it harder");
        let bass_dist = at(&j, "bass.distortion").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let fx_drive  = at(&j, "fx.distortion_drive").and_then(|v| v.as_f64()).unwrap_or(0.0);
        assert!(bass_dist > 0.05 || fx_drive > 0.05,
            "harder should raise distortion (bass={} or fx={})", bass_dist, fx_drive);
    }

    #[test]
    fn add_delay_sets_time_and_mix() {
        let j = run("add delay");
        assert!(at(&j, "fx.delay_time").is_some(), "add delay should set fx.delay_time");
        assert!(at(&j, "fx.delay_mix").is_some(),  "add delay should set fx.delay_mix");
        assert!(num(&j, "fx.delay_mix") > 0.0, "delay_mix should be > 0 after add delay");
    }

    #[test]
    fn remove_delay_zeroes_mix() {
        let j = run("remove delay");
        assert!(num(&j, "fx.delay_mix") < 0.01,
            "remove delay should zero delay_mix, got {}", num(&j, "fx.delay_mix"));
    }

    #[test]
    fn dry_signal_clears_all_fx_mixes() {
        let j = run("dry signal");
        assert!(num(&j, "fx.reverb_mix")     < 0.01, "dry: reverb_mix should be 0");
        assert!(num(&j, "fx.delay_mix")      < 0.01, "dry: delay_mix should be 0");
        assert!(num(&j, "fx.distortion_mix") < 0.01, "dry: distortion_mix should be 0");
    }

    // ── Pattern clearing ──────────────────────────────────────────────────────

    #[test]
    fn remove_kick_is_all_false() {
        assert_all_false(&run("remove kick"), "sequencer.kick808_steps");
    }

    #[test]
    fn remove_clap_is_all_false() {
        assert_all_false(&run("no claps"), "sequencer.clap909_steps");
    }

    #[test]
    fn remove_hihat_is_all_false() {
        assert_all_false(&run("no hats"), "sequencer.hihat808_steps");
    }

    #[test]
    fn remove_snare_is_all_false() {
        assert_all_false(&run("no snare"), "sequencer.snare808_steps");
    }

    #[test]
    fn clear_drums_clears_all_four_voices() {
        let j = run("clear all drums");
        assert_all_false(&j, "sequencer.kick808_steps");
        assert_all_false(&j, "sequencer.snare808_steps");
        assert_all_false(&j, "sequencer.hihat808_steps");
        assert_all_false(&j, "sequencer.clap909_steps");
    }

    // ── Schema compliance ─────────────────────────────────────────────────────

    #[test]
    fn every_response_has_comment() {
        for p in ["more acid", "darker", "harder", "add reverb", "remove reverb",
                  "remove kick", "no claps", "add delay", "dry signal", "more distortion"] {
            let j = run(p);
            assert!(j.get("_comment").is_some(), "no _comment for prompt '{}'", p);
        }
    }

    #[test]
    fn unit_params_stay_in_zero_to_one() {
        let paths = [
            "bass.cutoff", "bass.resonance", "bass.env_mod", "bass.decay",
            "bass.distortion", "bass.volume",
            "fx.reverb_mix", "fx.reverb_size", "fx.delay_mix", "fx.delay_feedback",
            "fx.distortion_drive", "fx.distortion_mix",
        ];
        for p in ["more acid", "darker", "harder", "add reverb", "add delay", "more distortion"] {
            let j = run(p);
            for path in paths {
                if let Some(v) = at(&j, path).and_then(|v| v.as_f64()) {
                    assert!(v >= 0.0 && v <= 1.0,
                        "prompt '{}': {} = {} is out of [0.0, 1.0]", p, path, v);
                }
            }
        }
    }

    #[test]
    fn bpm_stays_in_valid_range() {
        for p in ["more acid", "harder", "darker"] {
            let j = run(p);
            if let Some(bpm) = at(&j, "sequencer.bpm").and_then(|v| v.as_f64()) {
                assert!(bpm >= 40.0 && bpm <= 250.0,
                    "bpm {} out of range for '{}'", bpm, p);
            }
        }
    }

    #[test]
    fn no_unknown_top_level_keys() {
        let known = ["_comment", "bass", "sequencer", "fx"];
        for p in ["more acid", "darker", "add reverb", "remove kick", "dry signal"] {
            let j = run(p);
            if let Some(obj) = j.as_object() {
                for k in obj.keys() {
                    assert!(known.contains(&k.as_str()),
                        "prompt '{}': unexpected top-level key '{}'", p, k);
                }
            }
        }
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
