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

// ─── compile_fx_plan tests ───────────────────────────────────────────────────

#[cfg(test)]
mod fx_plan_tests {
    use crate::state::{
        FxStep, ModuleKind, PortDir, PortKind, PortRef, RackState, compile_fx_plan,
    };

    #[test]
    fn default_rack_compiles_full_chain_in_order() {
        let rack = RackState::default();
        let plan = compile_fx_plan(&rack);
        // Default rack wires FX serially: Waveshaper→Reverb→Delay→Bitcrush→
        // Chorus→Phaser→RingMod→Eq→Compressor→TapeSat→Drive
        assert_eq!(plan.steps.len(), 11);
        assert_eq!(plan.steps[0], FxStep::Waveshaper);
        assert_eq!(plan.steps[1], FxStep::Reverb);
        assert_eq!(plan.steps[2], FxStep::Delay);
        assert_eq!(plan.steps[3], FxStep::Bitcrush);
        assert_eq!(plan.steps[4], FxStep::Chorus);
        assert_eq!(plan.steps[5], FxStep::Phaser);
        assert_eq!(plan.steps[6], FxStep::RingMod);
        assert_eq!(plan.steps[7], FxStep::Eq);
        assert_eq!(plan.steps[8], FxStep::Compressor);
        assert_eq!(plan.steps[9], FxStep::TapeSat);
        assert_eq!(plan.steps[10], FxStep::Drive);
    }

    #[test]
    fn empty_rack_returns_empty_plan() {
        let rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 0,
        };
        let plan = compile_fx_plan(&rack);
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn disabled_fx_module_excluded_from_plan() {
        let mut rack = RackState::default();
        // Disable the Reverb module
        if let Some(m) = rack
            .modules
            .iter_mut()
            .find(|m| m.kind == ModuleKind::FxReverb)
        {
            m.enabled = false;
        }
        let plan = compile_fx_plan(&rack);
        assert!(
            !plan.steps.contains(&FxStep::Reverb),
            "disabled module must not appear in plan"
        );
        // Rest of chain still intact (10 steps, not 11)
        assert_eq!(plan.steps.len(), 10);
    }

    #[test]
    fn two_fx_custom_chain_compiles_in_correct_order() {
        // Minimal rack: two FX modules wired A → B
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let rev_id = rack.add_module(ModuleKind::FxReverb);
        let del_id = rack.add_module(ModuleKind::FxDelay);
        rack.connect(
            PortRef {
                module_id: rev_id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: del_id,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
        let plan = compile_fx_plan(&rack);
        assert_eq!(plan.steps, vec![FxStep::Reverb, FxStep::Delay]);
    }
}
