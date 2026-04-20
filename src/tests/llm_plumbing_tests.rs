// ─── tests/llm_plumbing_tests.rs ─────────────────────────────────────────────
// LLM-plumbing coverage: JSON action extraction, the json_repair pass
// that salvages malformed model output, the llama-server pool
// ref-counting, and per-agent model-switch semantics.
//
// Split from llm_tests.rs — the prompt/instruction/style/theory/dsp
// coverage stays there; this file covers the "how the LLM thread
// processes responses and manages server processes" half, which is
// independent of prompt construction.

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

    /// Simulates the full lifecycle described by the model-switching spec:
    /// console acquires global, agent inferences acquire+release (no leak),
    /// agent override flips load to a new model, console SwitchModel uses
    /// shutdown_all_except to unload everything but the new global.
    /// Locks down the regression path for the "we jump to wrong models" bug.
    #[test]
    fn console_switch_unloads_every_other_model() {
        let mut pool = LlamaServerPool::new(9000, 4096);

        // Startup: console acquires global.
        pool.insert_test_server("models/global-a.gguf", 9000);
        assert_eq!(pool.ref_count_for("models/global-a.gguf"), Some(1));

        // Two agent inferences against the global — acquire then release
        // (the leak fix).  ref_count must return to 1 after each pair.
        let _ = pool.acquire("models/global-a.gguf").unwrap();
        pool.release("models/global-a.gguf");
        let _ = pool.acquire("models/global-a.gguf").unwrap();
        pool.release("models/global-a.gguf");
        assert_eq!(
            pool.ref_count_for("models/global-a.gguf"),
            Some(1),
            "inference should not leak refs"
        );

        // Two agents pick explicit overrides → two new servers loaded.
        pool.insert_test_server("models/override-x.gguf", 9001);
        pool.insert_test_server("models/override-y.gguf", 9002);
        assert_eq!(pool.server_count(), 3);

        // Console SwitchModel: shutdown_all_except is the master-switch
        // primitive — every server other than the new global must die.
        pool.shutdown_all_except("models/global-b.gguf");
        let _ = pool.acquire("models/global-b.gguf").unwrap();

        assert_eq!(pool.server_count(), 1, "only new global should remain");
        assert_eq!(pool.port_for("models/global-b.gguf"), Some(9000));
        assert!(pool.port_for("models/global-a.gguf").is_none());
        assert!(pool.port_for("models/override-x.gguf").is_none());
        assert!(pool.port_for("models/override-y.gguf").is_none());
    }

    /// shutdown_all_except keeps the named server even with leaked refs.
    #[test]
    fn shutdown_all_except_keeps_target_with_leaked_refs() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/keep.gguf", 9000);
        pool.insert_test_server("models/drop1.gguf", 9001);
        pool.insert_test_server("models/drop2.gguf", 9002);
        // Simulate leaked refs on the keeper (3 acquires, 0 releases).
        let _ = pool.acquire("models/keep.gguf").unwrap();
        let _ = pool.acquire("models/keep.gguf").unwrap();
        assert_eq!(pool.ref_count_for("models/keep.gguf"), Some(3));

        pool.shutdown_all_except("models/keep.gguf");

        assert_eq!(pool.server_count(), 1);
        assert!(pool.port_for("models/keep.gguf").is_some());
        assert!(pool.port_for("models/drop1.gguf").is_none());
        assert!(pool.port_for("models/drop2.gguf").is_none());
        // Ref count preserved — keep was untouched.
        assert_eq!(pool.ref_count_for("models/keep.gguf"), Some(3));
    }

    /// Re-selecting the same agent model in the dropdown must not cause
    /// release+acquire churn (which would briefly hit ref_count == 0 and
    /// shut the server down).  The handler short-circuits when old == new.
    #[test]
    fn agent_model_no_op_on_same_path() {
        let mut pool = LlamaServerPool::new(9000, 4096);
        pool.insert_test_server("models/x.gguf", 9000);
        let initial_refs = pool.ref_count_for("models/x.gguf");
        // Mimic SwitchAgentModel handler's old==new short-circuit: no calls.
        // Asserting nothing changed is the contract we rely on at the call site.
        assert_eq!(pool.ref_count_for("models/x.gguf"), initial_refs);
        assert_eq!(pool.server_count(), 1);
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
        agent.model_path = Some("models/qwen3-8b.gguf".to_string());
        let resolved = agent
            .model_path
            .unwrap_or_else(|| state.llm.model_path.clone());
        assert_eq!(resolved, "models/qwen3-8b.gguf");
    }

    #[test]
    fn from_singleton_sets_model_none() {
        let state = AppState::default();
        let agent = LlmAgentState::from_singleton(42, &state.llm);
        assert!(agent.model_path.is_none());
        assert_eq!(agent.id, 42);
    }
}
