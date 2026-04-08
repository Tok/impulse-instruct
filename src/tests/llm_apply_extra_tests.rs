// ─── tests/llm_apply_extra_tests.rs ──────────────────────────────────────────
// Rack routing, scope filtering, step array, and combined LLM apply tests.
// Split from llm_apply_tests.rs to stay under the 1000-line limit.

#[cfg(test)]
mod llm_apply_rack_tests {
    use crate::state::rack::rack_kind_name_matches;
    use crate::state::{AppState, ModuleKind, apply_llm_update};

    #[test]
    fn rack_enable_disable_modules() {
        let mut s = AppState::default();
        for m in &mut s.rack.modules {
            if rack_kind_name_matches(m.kind, "reverb") {
                m.enabled = false;
            }
        }
        let update = serde_json::json!({ "rack": { "enable": ["reverb"] } });
        let s = apply_llm_update(s, &update, &[]);
        let rev = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::FxReverb);
        assert!(rev.unwrap().enabled);
    }

    #[test]
    fn rack_disable_module() {
        let s = AppState::default();
        let update = serde_json::json!({ "rack": { "disable": ["delay"] } });
        let s = apply_llm_update(s, &update, &[]);
        let del = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::FxDelay);
        assert!(!del.unwrap().enabled);
    }

    #[test]
    fn rack_connect_creates_cable() {
        let s = AppState::default();
        let initial_cables = s.rack.cables.len();
        // Use a forward-direction connection (waveshaper→eq) that doesn't
        // already exist as a direct cable and won't create a cycle.
        let update = serde_json::json!({
            "rack": { "connect": [{ "from": "waveshaper", "to": "eq" }] }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(
            s.rack.cables.len() > initial_cables || {
                let ws_id = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| m.kind == ModuleKind::FxWaveshaper)
                    .map(|m| m.id);
                let eq_id = s
                    .rack
                    .modules
                    .iter()
                    .find(|m| m.kind == ModuleKind::FxEq)
                    .map(|m| m.id);
                if let (Some(w), Some(e)) = (ws_id, eq_id) {
                    s.rack
                        .cables
                        .iter()
                        .any(|c| c.from.module_id == w && c.to.module_id == e)
                } else {
                    false
                }
            }
        );
    }

    #[test]
    fn rack_disconnect_removes_cable() {
        let s = AppState::default();
        let update = serde_json::json!({
            "rack": { "disconnect": [{ "from": "waveshaper", "to": "reverb" }] }
        });
        let initial = s.rack.cables.len();
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.rack.cables.len() < initial);
    }

    #[test]
    fn rack_connect_does_not_duplicate() {
        let s = AppState::default();
        let initial = s.rack.cables.len();
        let update = serde_json::json!({
            "rack": { "connect": [{ "from": "waveshaper", "to": "reverb" }] }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.rack.cables.len(), initial, "should not duplicate cable");
    }
}

#[cfg(test)]
mod llm_apply_scope_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn scope_restricts_to_specified_keys() {
        let s = AppState::default();
        let update = serde_json::json!({
            "bass": { "cutoff": 0.9 },
            "fx": { "reverb_mix": 0.8 }
        });
        let scope = vec!["bass".to_string()];
        let s = apply_llm_update(s, &update, &scope);
        assert!((s.bass_voices[0].synth.cutoff - 0.9).abs() < 1e-4);
        let default_fx = AppState::default().fx.reverb_mix;
        assert_eq!(s.fx.reverb_mix, default_fx);
    }

    #[test]
    fn empty_scope_allows_everything() {
        let s = AppState::default();
        let update = serde_json::json!({
            "bass": { "cutoff": 0.9 },
            "fx": { "reverb_mix": 0.8 }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.bass_voices[0].synth.cutoff - 0.9).abs() < 1e-4);
        assert!((s.fx.reverb_mix - 0.8).abs() < 1e-4);
    }

    #[test]
    fn scope_sequencer_only() {
        let s = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "swing": 0.7 },
            "bass": { "cutoff": 0.1 }
        });
        let scope = vec!["sequencer".to_string()];
        let s = apply_llm_update(s, &update, &scope);
        assert!((s.sequencer.swing - 0.7).abs() < 1e-4);
        let default_cutoff = AppState::default().bass_voices[0].synth.cutoff;
        assert_eq!(s.bass_voices[0].synth.cutoff, default_cutoff);
    }
}

#[cfg(test)]
mod llm_apply_step_array_tests {
    use crate::state::apply_llm_step_array;

    #[derive(Clone, Default)]
    struct FakeStep {
        active: bool,
    }

    #[test]
    fn empty_array_clears_all() {
        let mut items = vec![
            FakeStep { active: true },
            FakeStep { active: true },
            FakeStep { active: true },
        ];
        let arr: Vec<serde_json::Value> = vec![];
        apply_llm_step_array(&arr, &mut items, 3, |s, a| s.active = a);
        assert!(items.iter().all(|s| !s.active));
    }

    #[test]
    fn index_list_activates_positions() {
        let mut items: Vec<FakeStep> = (0..16).map(|_| FakeStep::default()).collect();
        let arr = vec![
            serde_json::json!(0),
            serde_json::json!(4),
            serde_json::json!(8),
        ];
        apply_llm_step_array(&arr, &mut items, 16, |s, a| s.active = a);
        assert!(items[0].active);
        assert!(!items[1].active);
        assert!(items[4].active);
        assert!(items[8].active);
    }

    #[test]
    fn inline_booleans_set_per_element() {
        let mut items: Vec<FakeStep> = (0..16).map(|_| FakeStep::default()).collect();
        let arr: Vec<serde_json::Value> = (0..16).map(|i| serde_json::json!(i % 2 == 0)).collect();
        apply_llm_step_array(&arr, &mut items, 16, |s, a| s.active = a);
        for (i, item) in items.iter().enumerate().take(16) {
            assert_eq!(item.active, i % 2 == 0);
        }
    }

    #[test]
    fn inline_integers_set_per_element() {
        let mut items: Vec<FakeStep> = (0..16).map(|_| FakeStep::default()).collect();
        let arr: Vec<serde_json::Value> = (0..16)
            .map(|i| serde_json::json!(if i % 4 == 0 { 1 } else { 0 }))
            .collect();
        apply_llm_step_array(&arr, &mut items, 16, |s, a| s.active = a);
        assert!(items[0].active);
        assert!(!items[1].active);
        assert!(items[4].active);
    }

    #[test]
    fn max_write_limits_writes() {
        let mut items: Vec<FakeStep> = (0..32).map(|_| FakeStep::default()).collect();
        items[20].active = true;
        let arr: Vec<serde_json::Value> = vec![];
        apply_llm_step_array(&arr, &mut items, 16, |s, a| s.active = a);
        assert!(
            items[20].active,
            "items beyond max_write should be untouched"
        );
    }

    #[test]
    fn index_out_of_bounds_ignored() {
        let mut items: Vec<FakeStep> = (0..16).map(|_| FakeStep::default()).collect();
        let arr = vec![serde_json::json!(0), serde_json::json!(999)];
        apply_llm_step_array(&arr, &mut items, 16, |s, a| s.active = a);
        assert!(items[0].active);
    }
}

#[cfg(test)]
mod llm_apply_combined_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn multiple_sections_in_single_update() {
        let s = AppState::default();
        let update = serde_json::json!({
            "bass": { "cutoff": 0.3 },
            "sequencer": { "bpm": 140.0 },
            "fx": { "reverb_mix": 0.5 },
            "noise": { "enabled": true },
            "hoover": { "enabled": true }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.bass_voices[0].synth.cutoff - 0.3).abs() < 1e-4);
        assert!((s.sequencer.bpm - 140.0).abs() < 0.01);
        assert!((s.fx.reverb_mix - 0.5).abs() < 1e-4);
        assert!(s.noise_voice.enabled);
        assert!(s.hoover.enabled);
    }

    #[test]
    fn empty_update_is_noop() {
        let s = AppState::default();
        let bpm = s.sequencer.bpm;
        let cutoff = s.bass_voices[0].synth.cutoff;
        let update = serde_json::json!({});
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.sequencer.bpm, bpm);
        assert_eq!(s.bass_voices[0].synth.cutoff, cutoff);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let s = AppState::default();
        let update = serde_json::json!({ "nonexistent_module": { "param": 0.5 } });
        let s2 = apply_llm_update(s.clone(), &update, &[]);
        assert_eq!(s2.sequencer.bpm, s.sequencer.bpm);
    }
}
