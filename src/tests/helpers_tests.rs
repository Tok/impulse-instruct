// ─── tests/helpers_tests.rs ──────────────────────────────────────────────────
// Tests for helper functions: spawn_agent, connect_control.

#[cfg(test)]
mod spawn_agent_tests {
    use crate::state::{AgentRole, AppState, ModuleKind, PortKind, spawn_agent};

    #[test]
    fn spawn_agent_adds_module_and_state() {
        let s = AppState::default();
        let initial_agents = s.llm_agents.len();
        let (s, id) = spawn_agent(s, "TestBot", &[], AgentRole::Producer, None);
        assert_eq!(s.llm_agents.len(), initial_agents + 1);
        assert!(
            s.rack
                .modules
                .iter()
                .any(|m| m.id == id && m.kind == ModuleKind::LlmAgent)
        );
        assert_eq!(s.llm_agents.last().unwrap().persona_name, "TestBot");
    }

    #[test]
    fn spawn_agent_empty_scope_wires_all_controllable() {
        let s = AppState::default();
        let (s, id) = spawn_agent(s, "FullBot", &[], AgentRole::Producer, None);
        let control_cables: Vec<_> = s
            .rack
            .cables
            .iter()
            .filter(|c| c.from.module_id == id && c.from.kind == PortKind::Control)
            .collect();
        assert!(!control_cables.is_empty(), "should have control cables");
    }

    #[test]
    fn spawn_agent_scoped_wires_only_matching() {
        let s = AppState::default();
        let scope = vec!["bass".to_string()];
        let (s, id) = spawn_agent(s, "BassBot", &scope, AgentRole::Specialist, None);
        let targets: Vec<u32> = s
            .rack
            .cables
            .iter()
            .filter(|c| c.from.module_id == id && c.from.kind == PortKind::Control)
            .map(|c| c.to.module_id)
            .collect();
        for tid in &targets {
            let m = s.rack.modules.iter().find(|m| m.id == *tid).unwrap();
            assert_eq!(
                m.kind,
                ModuleKind::AcidBass,
                "scoped agent should only wire to bass"
            );
        }
    }

    #[test]
    fn spawn_agent_with_model_path() {
        let s = AppState::default();
        let (s, _) = spawn_agent(
            s,
            "ModelBot",
            &[],
            AgentRole::Producer,
            Some("models/test.gguf".to_string()),
        );
        let agent = s.llm_agents.last().unwrap();
        assert_eq!(agent.model_path, Some("models/test.gguf".to_string()));
    }

    #[test]
    fn spawn_agent_sets_role() {
        let s = AppState::default();
        let (s, _) = spawn_agent(s, "MC", &[], AgentRole::Mc, None);
        assert_eq!(s.llm_agents.last().unwrap().role, AgentRole::Mc);
    }
}

#[cfg(test)]
mod connect_control_tests {
    use crate::state::{ModuleKind, PortDir, PortKind, RackState};

    #[test]
    fn connect_control_creates_cable_with_correct_ports() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
        };
        let agent_id = rack.add_module(ModuleKind::LlmAgent);
        let bass_id = rack.add_module(ModuleKind::AcidBass);

        rack.connect_control(agent_id, bass_id);

        assert_eq!(rack.cables.len(), 1);
        let cable = &rack.cables[0];
        assert_eq!(cable.from.module_id, agent_id);
        assert_eq!(cable.from.dir, PortDir::Out);
        assert_eq!(cable.from.kind, PortKind::Control);
        assert_eq!(cable.to.module_id, bass_id);
        assert_eq!(cable.to.dir, PortDir::In);
        assert_eq!(cable.to.kind, PortKind::Control);
    }

    #[test]
    fn connect_control_multiple_targets() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
            dyn_sequencer_rows: None,
        };
        let agent_id = rack.add_module(ModuleKind::LlmAgent);
        let bass_id = rack.add_module(ModuleKind::AcidBass);
        let drum_id = rack.add_module(ModuleKind::DrumKit808);

        rack.connect_control(agent_id, bass_id);
        rack.connect_control(agent_id, drum_id);

        assert_eq!(rack.cables.len(), 2);
        assert_eq!(rack.cables[0].to.module_id, bass_id);
        assert_eq!(rack.cables[1].to.module_id, drum_id);
    }
}

#[cfg(test)]
mod apply_agent_mode_and_tts_tests {
    use crate::state::{
        AgentRole, AppState, ConversationMode, ModuleKind, PortKind, apply_agent_mode_and_tts,
        spawn_agent,
    };

    fn fresh_with_agent() -> (AppState, u32) {
        let s = AppState::default();
        spawn_agent(s, "Bot", &[], AgentRole::Producer, None)
    }

    #[test]
    fn mode_off_sets_conversation_mode() {
        let (s, id) = fresh_with_agent();
        let s = apply_agent_mode_and_tts(s, id, Some("off"), false);
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert_eq!(agent.conversation_mode, ConversationMode::Off);
    }

    #[test]
    fn mode_producer_dj_mc_all_recognised() {
        for (label, expected) in &[
            ("producer", ConversationMode::Producer),
            ("dj", ConversationMode::Dj),
            ("mc", ConversationMode::Mc),
        ] {
            let (s, id) = fresh_with_agent();
            let s = apply_agent_mode_and_tts(s, id, Some(label), false);
            let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
            assert_eq!(agent.conversation_mode, *expected, "mode {label}");
        }
    }

    #[test]
    fn mode_string_is_case_insensitive() {
        let (s, id) = fresh_with_agent();
        let s = apply_agent_mode_and_tts(s, id, Some("MC"), false);
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert_eq!(agent.conversation_mode, ConversationMode::Mc);
    }

    #[test]
    fn unknown_mode_string_leaves_conversation_mode_unchanged() {
        let (mut s, id) = fresh_with_agent();
        s.llm_agents
            .iter_mut()
            .find(|a| a.id == id)
            .unwrap()
            .conversation_mode = ConversationMode::Producer;
        let s = apply_agent_mode_and_tts(s, id, Some("synthwave"), false);
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert_eq!(agent.conversation_mode, ConversationMode::Producer);
    }

    #[test]
    fn tts_true_adds_neutts_module_and_wires_a_control_cable() {
        let (s, id) = fresh_with_agent();
        let neutts_before = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::NeuTts)
            .count();
        let tts_modules_before = s.tts_modules.len();
        let s = apply_agent_mode_and_tts(s, id, None, true);
        let neutts_after = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::NeuTts)
            .count();
        assert_eq!(neutts_after, neutts_before + 1);
        assert_eq!(s.tts_modules.len(), tts_modules_before + 1);
        // The agent should now have a control cable to the new NeuTts module.
        let neutts_id = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::NeuTts)
            .last()
            .unwrap()
            .id;
        assert!(
            s.rack.cables.iter().any(|c| c.from.module_id == id
                && c.to.module_id == neutts_id
                && c.from.kind == PortKind::Control),
            "expected control cable from agent to NeuTts"
        );
    }

    #[test]
    fn tts_true_sets_scroll_target() {
        let (s, id) = fresh_with_agent();
        let s = apply_agent_mode_and_tts(s, id, None, true);
        assert_eq!(s.scroll_target, Some("tts".to_string()));
    }

    #[test]
    fn tts_false_does_not_add_neutts_module() {
        let (s, id) = fresh_with_agent();
        let neutts_before = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::NeuTts)
            .count();
        let s = apply_agent_mode_and_tts(s, id, Some("dj"), false);
        let neutts_after = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::NeuTts)
            .count();
        assert_eq!(neutts_after, neutts_before);
    }
}

#[cfg(test)]
mod observe_user_edit_tests {
    use crate::state::{AgentRole, AppState, STYLE_OBS_MAX, observe_user_edit, spawn_agent};

    fn one_agent() -> AppState {
        let s = AppState::default();
        let (s, _) = spawn_agent(s, "Bot", &[], AgentRole::Producer, None);
        s
    }

    #[test]
    fn high_value_records_a_high_observation() {
        let s = observe_user_edit(one_agent(), "fx.reverb_mix", 0.85);
        let obs = &s.llm_agents.last().unwrap().style_observations;
        assert!(
            obs.iter()
                .any(|o| o.contains("high") && o.contains("reverb_mix"))
        );
    }

    #[test]
    fn low_value_records_a_low_observation() {
        let s = observe_user_edit(one_agent(), "bass.cutoff", 0.1);
        let obs = &s.llm_agents.last().unwrap().style_observations;
        assert!(
            obs.iter()
                .any(|o| o.contains("low") && o.contains("bass.cutoff"))
        );
    }

    #[test]
    fn mid_value_is_ignored() {
        let s = one_agent();
        let before = s.llm_agents.last().unwrap().style_observations.len();
        let s = observe_user_edit(s, "bass.cutoff", 0.5);
        let after = s.llm_agents.last().unwrap().style_observations.len();
        assert_eq!(before, after);
    }

    #[test]
    fn repeated_edit_to_same_param_replaces_prior_observation() {
        let s = observe_user_edit(one_agent(), "bass.cutoff", 0.85);
        let s = observe_user_edit(s, "bass.cutoff", 0.15);
        let obs = &s.llm_agents.last().unwrap().style_observations;
        // Only one observation about bass.cutoff (the new low one).
        let n = obs.iter().filter(|o| o.contains("bass.cutoff")).count();
        assert_eq!(n, 1, "old observation should be replaced, got {obs:?}");
        assert!(
            obs.iter()
                .any(|o| o.contains("low") && o.contains("bass.cutoff"))
        );
    }

    #[test]
    fn observations_cap_at_style_obs_max() {
        let mut s = one_agent();
        for i in 0..(STYLE_OBS_MAX + 5) {
            // Use a unique param each time so the dedup branch doesn't hide
            // the cap behaviour.
            let path = format!("fx.knob_{i}");
            s = observe_user_edit(s, &path, 0.9);
        }
        let obs = &s.llm_agents.last().unwrap().style_observations;
        assert_eq!(obs.len(), STYLE_OBS_MAX);
    }

    #[test]
    fn observation_propagates_to_every_agent() {
        let mut s = AppState::default();
        for i in 0..3 {
            let (next, _) = spawn_agent(s, &format!("Bot{i}"), &[], AgentRole::Producer, None);
            s = next;
        }
        let s = observe_user_edit(s, "fx.reverb_mix", 0.95);
        for a in &s.llm_agents {
            // Every agent (including the seeded default) should have the obs.
            assert!(
                a.style_observations
                    .iter()
                    .any(|o| o.contains("reverb_mix")),
                "missing on agent {}",
                a.persona_name
            );
        }
    }
}

#[cfg(test)]
mod push_agent_memory_tests {
    use crate::state::{AGENT_MEMORY_MAX, AgentRole, AppState, push_agent_memory, spawn_agent};

    #[test]
    fn pushing_appends_a_snippet() {
        let (s, id) = spawn_agent(AppState::default(), "Bot", &[], AgentRole::Producer, None);
        let s = push_agent_memory(s, id, "made the bass acidic".to_string());
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert_eq!(
            agent.memory.last().map(|s| s.as_str()),
            Some("made the bass acidic")
        );
    }

    #[test]
    fn pushing_to_an_unknown_id_is_a_no_op() {
        let s = AppState::default();
        let before = s.llm_agents.iter().map(|a| a.memory.len()).sum::<usize>();
        let s = push_agent_memory(s, 99999, "ignored".to_string());
        let after = s.llm_agents.iter().map(|a| a.memory.len()).sum::<usize>();
        assert_eq!(before, after);
    }

    #[test]
    fn memory_caps_at_agent_memory_max() {
        let (mut s, id) = spawn_agent(AppState::default(), "Bot", &[], AgentRole::Producer, None);
        for i in 0..(AGENT_MEMORY_MAX + 5) {
            s = push_agent_memory(s, id, format!("snippet {i}"));
        }
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert_eq!(agent.memory.len(), AGENT_MEMORY_MAX);
        // Oldest entries dropped — last entry should be the final snippet.
        assert_eq!(
            agent.memory.last().map(|s| s.as_str()),
            Some(format!("snippet {}", AGENT_MEMORY_MAX + 4).as_str())
        );
    }
}

#[cfg(test)]
mod push_agent_recent_output_tests {
    use crate::state::{
        AGENT_RECENT_OUTPUTS_MAX, AgentRole, AppState, push_agent_recent_output, spawn_agent,
    };

    #[test]
    fn push_appends_and_caps() {
        let (mut s, id) = spawn_agent(AppState::default(), "Bot", &[], AgentRole::Producer, None);
        // Push more than the max — oldest should drop off.
        for i in 0..(AGENT_RECENT_OUTPUTS_MAX + 2) {
            s = push_agent_recent_output(s, id, format!("cycle {i}"));
        }
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert_eq!(agent.recent_outputs.len(), AGENT_RECENT_OUTPUTS_MAX);
        // Front is the oldest survivor (index 2 since 0 and 1 dropped).
        assert_eq!(
            agent.recent_outputs.front().map(|s| s.as_str()),
            Some("cycle 2")
        );
        assert_eq!(
            agent.recent_outputs.back().map(|s| s.as_str()),
            Some(format!("cycle {}", AGENT_RECENT_OUTPUTS_MAX + 1).as_str())
        );
    }

    #[test]
    fn empty_snippet_is_noop() {
        let (s, id) = spawn_agent(AppState::default(), "Bot", &[], AgentRole::Producer, None);
        let s = push_agent_recent_output(s, id, "   ".to_string());
        let agent = s.llm_agents.iter().find(|a| a.id == id).unwrap();
        assert!(agent.recent_outputs.is_empty());
    }

    #[test]
    fn unknown_id_is_noop() {
        let s = AppState::default();
        let s = push_agent_recent_output(s, 999_999, "ignored".to_string());
        for a in &s.llm_agents {
            assert!(a.recent_outputs.is_empty());
        }
    }
}

#[cfg(test)]
mod propagate_seed_tests {
    use crate::state::{AgentRole, AppState, propagate_seed, spawn_agent};

    #[test]
    fn updates_global_seed_and_unlocked_agents() {
        // Spawn two agents; seed-lock the second.  propagate_seed must
        // push the new seed to the global + first agent only.
        let (s, id1) = spawn_agent(AppState::default(), "A", &[], AgentRole::Producer, None);
        let (mut s, id2) = spawn_agent(s, "B", &[], AgentRole::Producer, None);
        let old_b = s.llm_agents.iter().find(|a| a.id == id2).unwrap().seed;
        s.llm_agents
            .iter_mut()
            .find(|a| a.id == id2)
            .unwrap()
            .seed_locked = true;
        let s = propagate_seed(s, 42);
        assert_eq!(s.llm.seed, 42);
        let a = s.llm_agents.iter().find(|a| a.id == id1).unwrap();
        assert_eq!(a.seed, 42, "unlocked agent follows the global seed");
        let b = s.llm_agents.iter().find(|a| a.id == id2).unwrap();
        assert_eq!(
            b.seed, old_b,
            "seed-locked agent keeps its own seed independent of the global"
        );
    }

    #[test]
    fn random_seed_minus_one_propagates() {
        let (s, id) = spawn_agent(AppState::default(), "R", &[], AgentRole::Producer, None);
        let mut s = propagate_seed(s, 123);
        assert_eq!(s.llm_agents.iter().find(|a| a.id == id).unwrap().seed, 123);
        // -1 is the "random each call" sentinel; propagate like any other value.
        s = propagate_seed(s, -1);
        assert_eq!(s.llm.seed, -1);
        assert_eq!(s.llm_agents.iter().find(|a| a.id == id).unwrap().seed, -1);
    }
}

#[cfg(test)]
mod per_voice_bass_step_tests {
    // set_bass_step_voice / toggle_bass_accent_voice / toggle_bass_slide_voice
    // — the per-voice bass writers that drive voices 1..=3 *and* mirror
    // voice 0 into the legacy `bass_pattern` field.  Voice 0 is the
    // interesting case because a write must land in both places.
    use crate::state::{
        AppState, set_an1x_step, set_bass_step_voice, toggle_bass_accent_voice,
        toggle_bass_slide_voice,
    };

    #[test]
    fn set_bass_step_voice_zero_mirrors_legacy_pattern() {
        let s = AppState::default();
        let s = set_bass_step_voice(s, 0, 5, 48, true);
        assert!(s.sequencer.bass_patterns[0][5].active);
        assert_eq!(s.sequencer.bass_patterns[0][5].note, 48);
        // Voice 0 mirrors the legacy field.
        assert!(s.sequencer.bass_pattern[5].active);
        assert_eq!(s.sequencer.bass_pattern[5].note, 48);
    }

    #[test]
    fn set_bass_step_voice_nonzero_does_not_touch_legacy() {
        let s = AppState::default();
        let s = set_bass_step_voice(s, 1, 3, 55, true);
        assert!(s.sequencer.bass_patterns[1][3].active);
        assert_eq!(s.sequencer.bass_patterns[1][3].note, 55);
        assert!(
            !s.sequencer.bass_pattern[3].active,
            "voice 1 writes must not touch the legacy voice-0 pattern"
        );
    }

    #[test]
    fn toggle_bass_accent_voice_flips_between_zero_and_one() {
        let s = AppState::default();
        let s = toggle_bass_accent_voice(s, 0, 4);
        assert_eq!(s.sequencer.bass_patterns[0][4].accent, 1.0);
        assert_eq!(s.sequencer.bass_pattern[4].accent, 1.0);
        let s = toggle_bass_accent_voice(s, 0, 4);
        assert_eq!(s.sequencer.bass_patterns[0][4].accent, 0.0);
        assert_eq!(s.sequencer.bass_pattern[4].accent, 0.0);
    }

    #[test]
    fn toggle_bass_slide_voice_flips_between_zero_and_one() {
        let s = AppState::default();
        let s = toggle_bass_slide_voice(s, 2, 7);
        assert_eq!(s.sequencer.bass_patterns[2][7].slide, 1.0);
        let s = toggle_bass_slide_voice(s, 2, 7);
        assert_eq!(s.sequencer.bass_patterns[2][7].slide, 0.0);
    }

    #[test]
    fn set_an1x_step_writes_into_pattern() {
        let s = AppState::default();
        let s = set_an1x_step(s, 9, 62, true);
        assert!(s.sequencer.an1x_pattern[9].active);
        assert_eq!(s.sequencer.an1x_pattern[9].note, 62);
    }

    #[test]
    fn per_voice_writers_clamp_voice_index_to_max() {
        // Out-of-range voice_idx (e.g. 99) clamps to MAX_BASS_VOICES-1
        // rather than panicking — defensive path tests the `vi.min(…)`.
        let s = AppState::default();
        let s = set_bass_step_voice(s, 99, 0, 48, true);
        let last = crate::state::MAX_BASS_VOICES - 1;
        assert!(s.sequencer.bass_patterns[last][0].active);
    }
}

#[cfg(test)]
mod format_llm_display_tests {
    use crate::state::{ConversationMode, format_llm_display};
    use serde_json::json;

    #[test]
    fn no_param_update_returns_raw_text_unchanged() {
        let out = format_llm_display(None, "hello world", &ConversationMode::Producer);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn off_mode_lists_changed_keys_even_if_comment_present() {
        let upd = json!({ "_comment": "ignored", "bass": {} });
        let out = format_llm_display(Some(&upd), "raw", &ConversationMode::Off);
        assert!(out.starts_with("updated "));
        assert!(out.contains("bass"));
        assert!(!out.contains("_comment"));
    }

    #[test]
    fn non_off_mode_with_comment_returns_the_comment() {
        let upd = json!({ "_comment": "rolled into a half-time pattern", "bass": {} });
        let out = format_llm_display(Some(&upd), "raw", &ConversationMode::Producer);
        assert_eq!(out, "rolled into a half-time pattern");
    }

    #[test]
    fn non_off_mode_without_comment_falls_back_to_keys() {
        let upd = json!({ "bass": {}, "fx": {} });
        let out = format_llm_display(Some(&upd), "raw", &ConversationMode::Producer);
        assert!(out.starts_with("updated "));
        assert!(out.contains("bass"));
        assert!(out.contains("fx"));
    }
}
