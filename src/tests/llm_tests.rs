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
