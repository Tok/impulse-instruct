// ─── tests/llm_apply_extra_tests.rs ──────────────────────────────────────────
// Rack routing, scope filtering, step array, and combined LLM apply tests.
// Split from llm_apply_tests.rs to stay under the 1000-line limit.

#[cfg(test)]
mod llm_apply_rack_tests {
    use crate::state::rack_scope::rack_kind_name_matches;
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
    fn rack_add_creates_module_and_wires_to_master() {
        let s = AppState::default();
        let initial = s.rack.modules.len();
        let initial_cables = s.rack.cables.len();
        let update = serde_json::json!({ "rack": { "add": ["bitcrush"] } });
        let s = apply_llm_update(s, &update, &[]);
        assert_eq!(s.rack.modules.len(), initial + 1);
        // Newly added FX module should be auto-cabled to master.
        let bc_id = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::FxBitcrush)
            .max_by_key(|m| m.id)
            .map(|m| m.id)
            .expect("bitcrush module not found");
        let master_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::MasterOutput)
            .map(|m| m.id)
            .expect("master output missing");
        assert!(
            s.rack
                .cables
                .iter()
                .any(|c| c.from.module_id == bc_id && c.to.module_id == master_id),
            "expected new module wired to master"
        );
        assert!(s.rack.cables.len() > initial_cables);
    }

    #[test]
    fn rack_remove_deletes_module_and_its_cables() {
        // Start by adding a fresh bitcrush module (so removing it leaves the
        // default rack stable for the kind-name lookup).
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "rack": { "add": ["bitcrush"] } }),
            &[],
        );
        let count_after_add = s.rack.modules.len();
        let cable_count_after_add = s.rack.cables.len();
        // Capture the id of the first bitcrush — that's the one rack.remove
        // will target ("first module matching kind").
        let removed_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::FxBitcrush)
            .map(|m| m.id)
            .unwrap();

        let s = apply_llm_update(
            s,
            &serde_json::json!({ "rack": { "remove": ["bitcrush"] } }),
            &[],
        );
        assert_eq!(s.rack.modules.len(), count_after_add - 1);
        assert!(
            !s.rack.modules.iter().any(|m| m.id == removed_id),
            "removed module should be gone"
        );
        // Cables touching the removed module must be cleaned up.
        assert!(
            !s.rack
                .cables
                .iter()
                .any(|c| c.from.module_id == removed_id || c.to.module_id == removed_id)
        );
        assert!(s.rack.cables.len() < cable_count_after_add);
    }

    #[test]
    fn rack_add_remove_round_trip_restores_module_count() {
        let s = AppState::default();
        let initial = s.rack.modules.len();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "rack": { "add": ["bitcrush"], "remove": ["bitcrush"] } }),
            &[],
        );
        // add then remove in the same pass: net zero modules.  add runs
        // before remove, so the just-added bitcrush is what gets removed
        // (or some pre-existing one — either way count is stable).
        assert_eq!(s.rack.modules.len(), initial);
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

#[cfg(test)]
mod sequencer_globals_tests {
    use crate::state::{AppState, Scale, apply_llm_update};

    #[test]
    fn bpm_clamps_into_range() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bpm": 9999.0 } }),
            &[],
        );
        assert_eq!(s.sequencer.bpm, 250.0);
        let s = apply_llm_update(s, &serde_json::json!({ "sequencer": { "bpm": 1.0 } }), &[]);
        assert_eq!(s.sequencer.bpm, 40.0);
    }

    #[test]
    fn swing_clamps_to_unit_range() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "swing": 5.0 } }),
            &[],
        );
        assert_eq!(s.sequencer.swing, 1.0);
    }

    #[test]
    fn time_sig_num_clamps_to_2_through_9() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "time_sig_num": 1 } }),
            &[],
        );
        assert_eq!(s.sequencer.time_sig_num, 2);
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "time_sig_num": 99 } }),
            &[],
        );
        assert_eq!(s.sequencer.time_sig_num, 9);
    }

    #[test]
    fn root_note_wraps_to_pitch_class() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "root_note": 200 } }),
            &[],
        );
        assert_eq!(s.sequencer.root_note, 11);
    }

    #[test]
    fn scale_string_parses_through_helper() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "scale": "Dorian" } }),
            &[],
        );
        assert!(matches!(s.sequencer.scale, Scale::Dorian));
    }

    #[test]
    fn scale_unknown_string_keeps_existing() {
        let mut s = AppState::default();
        s.sequencer.scale = Scale::Phrygian;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "scale": "not-a-scale" } }),
            &[],
        );
        assert!(matches!(s.sequencer.scale, Scale::Phrygian));
    }

    #[test]
    fn locked_bpm_is_preserved() {
        let mut s = AppState::default();
        s.sequencer.bpm = 130.0;
        s.llm.locked_params.insert("sequencer.bpm".to_string());
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bpm": 174.0 } }),
            &[],
        );
        assert_eq!(s.sequencer.bpm, 130.0);
    }

    #[test]
    fn out_of_scope_sequencer_globals_skip() {
        // A "bass" scope grants per-bass sequencer fields but NOT global
        // sequencer fields like bpm — those need explicit "sequencer".
        let mut s = AppState::default();
        s.sequencer.bpm = 120.0;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bpm": 174.0 } }),
            &["bass".to_string()],
        );
        assert_eq!(s.sequencer.bpm, 120.0);
    }

    #[test]
    fn steps_resize_updates_sequencer_steps() {
        let s = AppState::default();
        let s = apply_llm_update(s, &serde_json::json!({ "sequencer": { "steps": 32 } }), &[]);
        assert_eq!(s.sequencer.steps, 32);
    }

    #[test]
    fn steps_clamps_above_max() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "steps": 9999 } }),
            &[],
        );
        assert_eq!(s.sequencer.steps, crate::state::MAX_STEPS);
    }
}

#[cfg(test)]
mod amen_update_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn volume_round_trips_through_unit_range() {
        let s = AppState::default();
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "volume": 0.4 } }), &[]);
        assert!((s.amen.volume - 0.4).abs() < 1e-6);
    }

    #[test]
    fn loop_mode_toggles_via_bool() {
        let mut s = AppState::default();
        s.amen.loop_mode = false;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "loop_mode": true } }),
            &[],
        );
        assert!(s.amen.loop_mode);
    }

    #[test]
    fn end_offset_below_start_is_bumped_above_it() {
        let mut s = AppState::default();
        s.amen.start_offset = 0.6;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "end_offset": 0.1 } }),
            &[],
        );
        assert!(s.amen.end_offset > s.amen.start_offset);
    }

    #[test]
    fn slice_count_clamps_to_one_through_sixteen() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_count": 99 } }),
            &[],
        );
        assert_eq!(s.amen.slice_count, 16);
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "slice_count": 0 } }), &[]);
        assert_eq!(s.amen.slice_count, 1);
    }

    #[test]
    fn slice_pitches_truncates_at_sixteen_and_clamps_each() {
        let s = AppState::default();
        let arr: Vec<f32> = (0..30).map(|i| i as f32).collect();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_pitches": arr } }),
            &[],
        );
        assert_eq!(s.amen.slice_pitches.len(), 16);
        // Every value should be clamped into ±24.
        for p in &s.amen.slice_pitches {
            assert!(*p >= -24.0 && *p <= 24.0);
        }
    }

    #[test]
    fn slice_pitches_null_clears_the_vec() {
        let mut s = AppState::default();
        s.amen.slice_pitches = vec![1.0, 2.0, 3.0];
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_pitches": null } }),
            &[],
        );
        assert!(s.amen.slice_pitches.is_empty());
    }

    #[test]
    fn slice_volumes_clamps_to_zero_through_two() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_volumes": [-0.5, 0.5, 5.0] } }),
            &[],
        );
        assert_eq!(s.amen.slice_volumes, vec![0.0, 0.5, 2.0]);
    }

    #[test]
    fn locked_amen_field_is_preserved() {
        let mut s = AppState::default();
        s.amen.gate = 0.5;
        s.llm.locked_params.insert("amen.gate".to_string());
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "gate": 0.9 } }), &[]);
        assert_eq!(s.amen.gate, 0.5);
    }

    #[test]
    fn out_of_scope_amen_object_is_ignored() {
        let mut s = AppState::default();
        s.amen.gate = 0.5;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "gate": 0.9 } }),
            &["bass".to_string()],
        );
        assert_eq!(s.amen.gate, 0.5);
    }
}

#[cfg(test)]
mod euclidean_update_tests {
    use crate::state::llm_apply_seq::drum_voice_from_str;
    use crate::state::{AppState, DrumVoice, apply_llm_update};

    #[test]
    fn drum_voice_from_str_maps_known_aliases() {
        assert_eq!(drum_voice_from_str("kick_a"), Some(DrumVoice::Kick808));
        assert_eq!(
            drum_voice_from_str("hihat_a"),
            Some(DrumVoice::HihatClosed808)
        );
        assert_eq!(
            drum_voice_from_str("closed_hat_a"),
            Some(DrumVoice::HihatClosed808)
        );
        assert_eq!(
            drum_voice_from_str("hihat_a_open"),
            Some(DrumVoice::HihatOpen808)
        );
        assert_eq!(
            drum_voice_from_str("open_hat_a"),
            Some(DrumVoice::HihatOpen808)
        );
        assert_eq!(drum_voice_from_str("clap_b"), Some(DrumVoice::Clap909));
        assert_eq!(drum_voice_from_str(""), None);
        assert_eq!(drum_voice_from_str("guitar"), None);
    }

    #[test]
    fn euclidean_4_in_16_lights_every_fourth_step_on_kick_a() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "euclidean": { "voice": "kick_a", "pulses": 4, "steps": 16 }
            }),
            &[],
        );
        let row = s
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Kick808)
            .expect("kick_a row");
        let pulses: Vec<usize> = row
            .iter()
            .enumerate()
            .filter_map(|(i, st)| if st.active { Some(i) } else { None })
            .take(4)
            .collect();
        assert_eq!(pulses, vec![0, 4, 8, 12]);
    }

    #[test]
    fn euclidean_unknown_voice_is_a_noop() {
        let s = AppState::default();
        let before: Vec<bool> = s
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Kick808)
            .unwrap()
            .iter()
            .map(|st| st.active)
            .collect();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "euclidean": { "voice": "guitar", "pulses": 5, "steps": 16 }
            }),
            &[],
        );
        let after: Vec<bool> = s
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Kick808)
            .unwrap()
            .iter()
            .map(|st| st.active)
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn euclidean_respects_steps_lock() {
        let mut s = AppState::default();
        s.llm
            .locked_params
            .insert("sequencer.kick_a_steps".to_string());
        let before: Vec<bool> = s
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Kick808)
            .unwrap()
            .iter()
            .map(|st| st.active)
            .collect();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "euclidean": { "voice": "kick_a", "pulses": 5, "steps": 16 }
            }),
            &[],
        );
        let after: Vec<bool> = s
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Kick808)
            .unwrap()
            .iter()
            .map(|st| st.active)
            .collect();
        assert_eq!(before, after);
    }

    #[test]
    fn euclidean_defaults_pulses_to_4_when_unspecified() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "euclidean": { "voice": "snare_a", "steps": 16 }
            }),
            &[],
        );
        let n_active = s
            .sequencer
            .drum_patterns
            .get(&DrumVoice::Snare808)
            .unwrap()
            .iter()
            .take(16)
            .filter(|st| st.active)
            .count();
        assert_eq!(n_active, 4);
    }
}
