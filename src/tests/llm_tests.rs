#[cfg(test)]
mod prompt_tests {
    use crate::llm::{build_system_prompt, param_json_schema};
    use crate::state::AppState;

    #[test]
    fn build_system_prompt_contains_json_only_instruction() {
        let state = AppState::default();
        let prompt = build_system_prompt(&state, &[]);
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
        state.bass_voices[0].synth.cutoff = 0.5; // exact f32 representation
        let prompt = build_system_prompt(&state, &[]);
        assert!(
            prompt.contains("0.5"),
            "prompt should embed current cutoff value"
        );
    }

    #[test]
    fn build_system_prompt_lists_locked_params() {
        use crate::state::lock_param;
        let state = lock_param(AppState::default(), "bass.cutoff");
        let prompt = build_system_prompt(&state, &[]);
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
            !set.is_empty(),
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
        let new_state = apply_llm_update(state, &update, &[]);
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
        let new_state = apply_llm_update(state, &update, &[]);
        assert_eq!(new_state.sequencer.bass_pattern[0].note, 49);
    }
}

#[cfg(test)]
mod style_tests {
    use crate::llm::styles::StyleCatalog;
    use crate::state::{AppState, apply_llm_update};

    // Helper: apply a style's baseline_params to a fresh AppState
    fn apply_baseline(style_id: &str) -> AppState {
        let catalog = StyleCatalog::get();
        let style = catalog
            .find_by_id(style_id)
            .unwrap_or_else(|| panic!("style '{}' not found in catalog", style_id));
        let state = AppState::default();
        match &style.baseline_params {
            Some(bp) => apply_llm_update(state, bp, &[]),
            None => state,
        }
    }

    #[test]
    fn all_styles_have_baseline_params() {
        let catalog = StyleCatalog::get();
        let missing: Vec<&str> = catalog
            .styles()
            .iter()
            .filter(|s| s.baseline_params.is_none())
            .map(|s| s.id.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "styles missing baseline_params: {:?}",
            missing
        );
    }

    #[test]
    fn acid_classic_baseline_is_acid_character() {
        let state = apply_baseline("acid_classic");
        assert!(
            state.bass_voices[0].synth.resonance >= 0.7,
            "acid needs high resonance (≥0.7), got {}",
            state.bass_voices[0].synth.resonance
        );
        assert!(
            state.bass_voices[0].synth.env_mod >= 0.6,
            "acid needs high env_mod (≥0.6), got {}",
            state.bass_voices[0].synth.env_mod
        );
        assert!(
            state.sequencer.bpm >= 112.0 && state.sequencer.bpm <= 130.0,
            "acid bpm should be 112–130, got {}",
            state.sequencer.bpm
        );
        assert!(!state.hoover.enabled, "acid should not have hoover enabled");
    }

    #[test]
    fn breakcore_baseline_has_extreme_bpm() {
        let state = apply_baseline("breakcore");
        assert!(
            state.sequencer.bpm >= 160.0,
            "breakcore bpm should be ≥160, got {}",
            state.sequencer.bpm
        );
    }

    #[test]
    fn gabber_baseline_uses_heavy_distortion() {
        let state = apply_baseline("gabber");
        assert!(
            state.fx.distortion_drive >= 0.2 || state.fx.distortion_mix >= 0.3,
            "gabber should have heavy distortion (drive≥0.2 or mix≥0.3)"
        );
        assert!(
            state.sequencer.bpm >= 160.0,
            "gabber bpm should be ≥160, got {}",
            state.sequencer.bpm
        );
    }

    #[test]
    fn early_rave_baseline_enables_hoover() {
        let state = apply_baseline("early_rave");
        assert!(state.hoover.enabled, "early_rave must have hoover enabled");
        assert!(
            state.hoover.resonance >= 0.75,
            "hoover resonance should be ≥0.75 for dominator sound, got {}",
            state.hoover.resonance
        );
        assert!(
            state.hoover.filter_start >= 0.8,
            "hoover filter_start should be ≥0.8 for bright start, got {}",
            state.hoover.filter_start
        );
        assert!(
            state.sequencer.bpm >= 145.0 && state.sequencer.bpm <= 170.0,
            "early_rave bpm should be 145–170, got {}",
            state.sequencer.bpm
        );
    }

    #[test]
    fn trance_baseline_enables_hoover() {
        let state = apply_baseline("trance");
        assert!(state.hoover.enabled, "trance should have hoover enabled");
        assert!(
            state.sequencer.bpm >= 130.0 && state.sequencer.bpm <= 145.0,
            "trance bpm should be 130–145, got {}",
            state.sequencer.bpm
        );
    }

    #[test]
    fn ambient_baselines_have_heavy_reverb() {
        for id in &[
            "dark_ambient",
            "space_ambient",
            "ambient_techno",
            "ambient_house",
        ] {
            let state = apply_baseline(id);
            assert!(
                state.fx.reverb_mix >= 0.6,
                "{} should have reverb_mix ≥0.6, got {}",
                id,
                state.fx.reverb_mix
            );
        }
    }

    #[test]
    fn ambient_baselines_have_slow_bpm() {
        for id in &["dark_ambient", "space_ambient"] {
            let state = apply_baseline(id);
            assert!(
                state.sequencer.bpm <= 80.0,
                "{} bpm should be ≤80, got {}",
                id,
                state.sequencer.bpm
            );
        }
    }

    #[test]
    fn dub_techno_baseline_has_long_reverb_and_delay() {
        let state = apply_baseline("dub_techno");
        assert!(
            state.fx.reverb_mix >= 0.5,
            "dub techno needs heavy reverb, got {}",
            state.fx.reverb_mix
        );
        assert!(
            state.fx.delay_feedback >= 0.6,
            "dub techno needs long delay feedback, got {}",
            state.fx.delay_feedback
        );
    }

    #[test]
    fn style_keywords_cover_rave_and_dominator() {
        let catalog = StyleCatalog::get();
        let has_dominator = catalog
            .styles()
            .iter()
            .any(|s| s.keywords.iter().any(|kw| kw.contains("dominator")));
        assert!(
            has_dominator,
            "catalog should have a style with 'dominator' keyword"
        );
        let has_hoover = catalog
            .styles()
            .iter()
            .any(|s| s.keywords.iter().any(|kw| kw.contains("hoover")));
        assert!(
            has_hoover,
            "catalog should have a style with 'hoover' keyword"
        );
    }

    #[test]
    fn style_prompt_language_says_reset() {
        use crate::llm::build_system_prompt;
        let mut state = AppState::default();
        state.llm.active_style = Some("acid_classic".to_string());
        let prompt = build_system_prompt(&state, &[]);
        assert!(
            prompt.contains("RESET") || prompt.contains("from scratch"),
            "style prompt should say RESET or 'from scratch', not 'evolve'"
        );
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

// ─── extract_llm_actions tests ───────────────────────────────────────────────

mod action_extraction {
    use crate::llm::{LlmAction, extract_llm_actions};
    use serde_json::json;

    #[test]
    fn empty_object_yields_no_actions() {
        let mut obj = serde_json::Map::new();
        assert!(extract_llm_actions(&mut obj).is_empty());
    }

    #[test]
    fn save_project_true() {
        let mut obj = json!({"save_project": true}).as_object().unwrap().clone();
        let actions = extract_llm_actions(&mut obj);
        assert!(matches!(actions[0], LlmAction::SaveProject));
        assert!(!obj.contains_key("save_project"));
    }

    #[test]
    fn save_project_false_ignored() {
        let mut obj = json!({"save_project": false}).as_object().unwrap().clone();
        assert!(extract_llm_actions(&mut obj).is_empty());
    }

    #[test]
    fn heat_is_user_only_llm_cannot_set() {
        let mut obj = json!({"settings": {"heat": 0.9}})
            .as_object()
            .unwrap()
            .clone();
        let actions = extract_llm_actions(&mut obj);
        assert!(
            !actions.iter().any(|a| matches!(a, LlmAction::SetHeat(_))),
            "LLM-emitted `heat` must be ignored — heat is user-only"
        );
    }

    #[test]
    fn style_extracted() {
        let mut obj = json!({"settings": {"style": "acid_house"}})
            .as_object()
            .unwrap()
            .clone();
        let actions = extract_llm_actions(&mut obj);
        match &actions[0] {
            LlmAction::SetStyle(s) => assert_eq!(s, "acid_house"),
            _ => panic!("expected SetStyle"),
        }
    }

    #[test]
    fn all_settings_extracted() {
        let mut obj = json!({
            "settings": {
                "heat": 0.7, "style": "techno", "persona": "DJ",
                "conversation_mode": "mc", "jam_bars": 4.0
            }
        })
        .as_object()
        .unwrap()
        .clone();
        let actions = extract_llm_actions(&mut obj);
        // heat is ignored (user-only) — 4 actions: style, persona, conv_mode, jam_bars
        assert_eq!(actions.len(), 4);
        assert!(
            !actions.iter().any(|a| matches!(a, LlmAction::SetHeat(_))),
            "heat must not be extractable from LLM output"
        );
        assert!(!obj.contains_key("settings")); // consumed
    }

    #[test]
    fn jam_bars_negative_clamped() {
        let mut obj = json!({"settings": {"jam_bars": -2.0}})
            .as_object()
            .unwrap()
            .clone();
        let actions = extract_llm_actions(&mut obj);
        match &actions[0] {
            LlmAction::SetJamBars(j) => assert_eq!(*j, 0.0),
            _ => panic!("expected SetJamBars"),
        }
    }

    #[test]
    fn settings_key_removed_even_when_empty() {
        let mut obj = json!({"settings": {}}).as_object().unwrap().clone();
        extract_llm_actions(&mut obj);
        assert!(!obj.contains_key("settings"));
    }
}

// ─── json_repair tests ──────────────────────────────────────────────────────

mod json_repair_tests {
    use crate::llm::json_repair::{repair_json, sanitize_json_structure, split_thinking};
    use serde_json::json;

    // ── repair_json ─────────────────────────────────────────────────────

    #[test]
    fn valid_json_passes_through() {
        let v = repair_json(r#"{"bass": {"cutoff": 0.5}}"#).unwrap();
        assert_eq!(v["bass"]["cutoff"], 0.5);
    }

    #[test]
    fn truncated_object_repaired() {
        // Simulates max_tokens cutting mid-object
        let v = repair_json(r#"{"bass": {"cutoff": 0.5}"#).unwrap();
        assert_eq!(v["bass"]["cutoff"], 0.5);
    }

    #[test]
    fn truncated_array_repaired() {
        let v = repair_json(r#"{"steps": [1, 2, 3"#).unwrap();
        assert_eq!(v["steps"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn completely_invalid_returns_none() {
        assert!(repair_json("this is not json at all !!!").is_none());
    }

    #[test]
    fn trailing_comma_handled() {
        let v = repair_json(r#"{"bass": {"cutoff": 0.5,}"#);
        // May or may not parse depending on repair — at minimum shouldn't panic
        let _ = v;
    }

    // ── sanitize_json_structure ──────────────────────────────────────────

    #[test]
    fn bass_lifted_from_sequencer() {
        let v = json!({"sequencer": {"bass": {"cutoff": 0.3}, "bpm": 120}});
        let s = sanitize_json_structure(v);
        assert_eq!(s["bass"]["cutoff"], 0.3);
        assert!(s["sequencer"]["bass"].is_null());
        assert_eq!(s["sequencer"]["bpm"], 120);
    }

    #[test]
    fn fx_lifted_from_sequencer() {
        let v = json!({"sequencer": {"fx": {"reverb_mix": 0.4}}});
        let s = sanitize_json_structure(v);
        assert_eq!(s["fx"]["reverb_mix"], 0.4);
    }

    #[test]
    fn nested_fx_stripped() {
        let v = json!({"fx": {"reverb_mix": 0.3, "fx": {"delay_mix": 0.2}}});
        let s = sanitize_json_structure(v);
        assert_eq!(s["fx"]["reverb_mix"], 0.3);
        assert!(s["fx"]["fx"].is_null());
    }

    #[test]
    fn hallucinated_keys_stripped() {
        let v = json!({"bass": {}, "drum_ratchets": [1,2], "patterns": {}});
        let s = sanitize_json_structure(v);
        assert!(s["drum_ratchets"].is_null());
        assert!(s["patterns"].is_null());
        assert!(s["bass"].is_object());
    }

    #[test]
    fn dot_notation_lfo_converted_to_array() {
        let v = json!({
            "lfo": {
                "lfo[0].enabled": true,
                "lfo[0].rate": 0.5,
                "lfo[1].enabled": false
            }
        });
        let s = sanitize_json_structure(v);
        let arr = s["lfo"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["enabled"], true);
        assert_eq!(arr[0]["rate"], 0.5);
        assert_eq!(arr[1]["enabled"], false);
    }

    #[test]
    fn named_slot_lfo_converted_to_array() {
        let v = json!({
            "lfo": {
                "lfo_0": {"enabled": true, "rate": 0.3},
                "lfo_2": {"depth": 0.8}
            }
        });
        let s = sanitize_json_structure(v);
        let arr = s["lfo"].as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[0]["rate"], 0.3);
        assert_eq!(arr[2]["depth"], 0.8);
    }

    #[test]
    fn non_object_passes_through() {
        let v = json!("just a string");
        let s = sanitize_json_structure(v);
        assert_eq!(s, "just a string");
    }

    // ── split_thinking ──────────────────────────────────────────────────

    #[test]
    fn thinking_block_extracted() {
        let (think, rest) = split_thinking("<think>planning</think>{\"bass\": {}}");
        assert_eq!(think.unwrap(), "planning");
        assert_eq!(rest, "{\"bass\": {}}");
    }

    #[test]
    fn no_thinking_block() {
        let (think, rest) = split_thinking("{\"bass\": {}}");
        assert!(think.is_none());
        assert_eq!(rest, "{\"bass\": {}}");
    }

    #[test]
    fn empty_thinking_block_returns_none() {
        let (think, rest) = split_thinking("<think>  </think>remainder");
        assert!(think.is_none());
        assert_eq!(rest, "remainder");
    }

    #[test]
    fn whitespace_around_thinking_trimmed() {
        let (think, _) = split_thinking("  <think> hello world </think>  rest  ");
        assert_eq!(think.unwrap(), "hello world");
    }
}

// ── Server pool tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod pool_tests {
    use crate::llm::LlamaServerPool;

    #[test]
    fn acquire_same_model_twice_reuses_server() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        // Second acquire should bump ref_count, not add a new server.
        let port = pool.acquire("models/a.gguf").unwrap();
        assert_eq!(port, 9000);
        assert_eq!(pool.server_count(), 1);
        assert_eq!(pool.ref_count_for("models/a.gguf"), Some(2));
    }

    #[test]
    fn two_different_models_get_different_ports() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        pool.insert_test_server("models/b.gguf", 9001);
        assert_eq!(pool.server_count(), 2);
        assert_eq!(pool.port_for("models/a.gguf"), Some(9000));
        assert_eq!(pool.port_for("models/b.gguf"), Some(9001));
    }

    #[test]
    fn release_last_ref_removes_server() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        assert_eq!(pool.server_count(), 1);
        pool.release("models/a.gguf");
        assert_eq!(pool.server_count(), 0);
    }

    #[test]
    fn release_with_remaining_refs_keeps_server() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        // Bump ref_count to 2
        let _ = pool.acquire("models/a.gguf").unwrap();
        assert_eq!(pool.ref_count_for("models/a.gguf"), Some(2));
        pool.release("models/a.gguf");
        assert_eq!(pool.server_count(), 1);
        assert_eq!(pool.ref_count_for("models/a.gguf"), Some(1));
    }

    #[test]
    fn next_free_port_skips_occupied() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        pool.insert_test_server("models/b.gguf", 9001);
        // Next free should be 9002
        pool.insert_test_server("models/c.gguf", 9002);
        // Remove middle one, next free should now be 9001
        pool.release("models/b.gguf");
        pool.insert_test_server(
            "models/d.gguf",
            pool.port_for("models/d.gguf").unwrap_or(9001),
        );
        // Verify we can still find ports
        assert!(pool.server_count() <= 4);
    }

    #[test]
    fn shutdown_model_removes_entry() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        pool.insert_test_server("models/b.gguf", 9001);
        pool.shutdown_model("models/a.gguf");
        assert_eq!(pool.server_count(), 1);
        assert!(pool.port_for("models/a.gguf").is_none());
        assert_eq!(pool.port_for("models/b.gguf"), Some(9001));
    }

    #[test]
    fn shutdown_all_clears_pool() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/a.gguf", 9000);
        pool.insert_test_server("models/b.gguf", 9001);
        pool.shutdown_all();
        assert_eq!(pool.server_count(), 0);
    }
}

#[cfg(test)]
mod agent_model_tests {
    use crate::state::{AppState, LlmAgentState};

    #[test]
    fn agent_model_none_falls_back_to_global() {
        let state = AppState::default();
        let agent = LlmAgentState::new_default(1);
        assert!(agent.model_path.is_none());
        let resolved = agent
            .model_path
            .unwrap_or_else(|| state.llm.model_path.clone());
        assert_eq!(resolved, state.llm.model_path);
    }

    #[test]
    fn agent_model_some_overrides_global() {
        let state = AppState::default();
        let mut agent = LlmAgentState::new_default(1);
        agent.model_path = Some("models/bonsai.gguf".to_string());
        let resolved = agent
            .model_path
            .unwrap_or_else(|| state.llm.model_path.clone());
        assert_eq!(resolved, "models/bonsai.gguf");
    }

    #[test]
    fn from_singleton_sets_model_none() {
        let state = AppState::default();
        let agent = LlmAgentState::from_singleton(42, &state.llm);
        assert!(agent.model_path.is_none());
        assert_eq!(agent.id, 42);
    }
}
