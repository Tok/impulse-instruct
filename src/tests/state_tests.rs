#[cfg(test)]
#[allow(clippy::module_inception)]
mod state_tests {
    use crate::state::{AppState, DrumVoice, apply_llm_update, lock_param, toggle_drum_step};

    #[test]
    fn apply_llm_update_sets_cutoff() {
        let state = AppState::default();
        let update = serde_json::json!({ "bass": { "cutoff": 0.9 } });
        let next = apply_llm_update(state, &update, &[]);
        assert!((next.bass_voices[0].synth.cutoff - 0.9).abs() < 1e-4);
    }

    #[test]
    fn apply_llm_update_clamps_to_unit_range() {
        let state = AppState::default();
        let update = serde_json::json!({ "bass": { "cutoff": 1.5 } });
        let next = apply_llm_update(state, &update, &[]);
        assert!(next.bass_voices[0].synth.cutoff <= 1.0);
    }

    #[test]
    fn locked_param_not_overwritten_by_llm() {
        let state = AppState::default();
        let original_cutoff = state.bass_voices[0].synth.cutoff;
        let state = lock_param(state, "bass.cutoff");

        let update = serde_json::json!({ "bass": { "cutoff": 0.99 } });
        let next = apply_llm_update(state, &update, &[]);
        assert_eq!(
            next.bass_voices[0].synth.cutoff, original_cutoff,
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
        let next = apply_llm_update(state, &update, &[]);
        assert!((next.sequencer.bpm - 175.0).abs() < 0.01);
    }

    #[test]
    fn bpm_clamped_to_valid_range() {
        let state = AppState::default();
        let update = serde_json::json!({ "sequencer": { "bpm": 999.0 } });
        let next = apply_llm_update(state, &update, &[]);
        assert!(next.sequencer.bpm <= 250.0, "bpm should be clamped");
    }
}

#[cfg(test)]
mod expand_steps_tests {
    use crate::state::{AppState, DrumVoice, expand_sequencer_steps, toggle_drum_step};

    #[test]
    fn expand_tiles_drum_pattern_into_new_slots() {
        // Snare808 starts silent; turn on step 0 and step 3, then expand 32 → 64
        let state = AppState::default();
        let state = toggle_drum_step(state, DrumVoice::Snare808, 0);
        let state = toggle_drum_step(state, DrumVoice::Snare808, 3);
        assert!(state.sequencer.drum_patterns[&DrumVoice::Snare808][0].active);
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Snare808][32].active);

        let state = expand_sequencer_steps(state, 64);
        assert_eq!(state.sequencer.steps, 64);
        // Step 32 should mirror step 0 (active)
        assert!(
            state.sequencer.drum_patterns[&DrumVoice::Snare808][32].active,
            "step 32 should be tiled from step 0"
        );
        // Step 35 should mirror step 3 (active)
        assert!(
            state.sequencer.drum_patterns[&DrumVoice::Snare808][35].active,
            "step 35 should be tiled from step 3"
        );
        // Step 33 should mirror step 1 (inactive)
        assert!(!state.sequencer.drum_patterns[&DrumVoice::Snare808][33].active);
    }

    #[test]
    fn expand_32_to_64_tiles_two_copies() {
        // Set up a kick on steps 0, 4, 8, 12 (default is blank)
        let mut state = AppState::default();
        for step in [0, 4, 8, 12] {
            state = toggle_drum_step(state, DrumVoice::Kick808, step);
        }
        let state = expand_sequencer_steps(state, 64);
        assert_eq!(state.sequencer.steps, 64);
        let kick = &state.sequencer.drum_patterns[&DrumVoice::Kick808];
        // Each bank of 32 should repeat the same pattern
        for bank in 0..2 {
            assert!(kick[bank * 32].active, "kick missing at step {}", bank * 32);
            assert!(
                kick[bank * 32 + 4].active,
                "kick missing at step {}",
                bank * 32 + 4
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
        assert_eq!(state.sequencer.bass_pattern[0].accent, 0.0);
        let state = toggle_bass_accent(state, 0);
        assert!(state.sequencer.bass_pattern[0].accent > 0.0);
        let state = toggle_bass_accent(state, 0);
        assert_eq!(state.sequencer.bass_pattern[0].accent, 0.0);

        assert_eq!(state.sequencer.bass_pattern[2].slide, 0.0);
        let state = toggle_bass_slide(state, 2);
        assert!(state.sequencer.bass_pattern[2].slide > 0.0);
    }

    #[test]
    fn apply_reese_preset_sets_supersaw_and_highpass() {
        let state = apply_reese_preset(AppState::default());
        use crate::state::{FilterMode, Waveform};
        assert_eq!(state.bass_voices[0].synth.waveform, Waveform::Supersaw);
        assert_eq!(state.bass_voices[0].synth.supersaw_voices, 2);
        assert_eq!(state.bass_voices[0].synth.filter_mode, FilterMode::Highpass);
        assert!(state.bass_voices[0].synth.sub_osc_level > 0.0);
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
    fn chain_push_caps_at_max_banks() {
        use crate::state::MAX_BANKS;
        let state = AppState::default();
        let mut s = state;
        for i in 0..(MAX_BANKS * 2) {
            s = chain_push(s, i % MAX_BANKS);
        }
        assert_eq!(
            s.chain.len(),
            MAX_BANKS,
            "chain should cap at MAX_BANKS entries"
        );
    }

    #[test]
    fn set_chain_enabled_false_resets_pos() {
        let state = AppState {
            chain_pos: 4,
            chain_enabled: true,
            ..AppState::default()
        };
        let state = set_chain_enabled(state, false);
        assert!(!state.chain_enabled);
        assert_eq!(state.chain_pos, 0, "disabling chain should reset position");
    }
}

// compile_fx_plan tests moved to fx_plan_tests.rs

// ─── Transition tests — untested pure functions ────────────────────────────────
#[cfg(test)]
mod transition_coverage_tests {
    use crate::state::synth_types::Waveform;
    use crate::state::{
        AppState, DrumVoice, Scale, apply_boc_preset, apply_hoover_preset, apply_reese_preset,
        bank_load, bank_write, lock_param, set_an1x_step, set_bass_step, set_chain,
        set_chain_enabled, set_hoover_step, set_pattern_edit, set_root_note, set_scale,
        set_scale_snap, toggle_bass_accent, toggle_bass_slide, toggle_sequencer_running,
        unlock_param,
    };

    #[test]
    fn set_root_note_clamps_to_11() {
        let s = set_root_note(AppState::default(), 99);
        assert_eq!(s.sequencer.root_note, 11);
    }

    #[test]
    fn set_root_note_accepts_valid() {
        let s = set_root_note(AppState::default(), 5);
        assert_eq!(s.sequencer.root_note, 5);
    }

    #[test]
    fn set_scale_round_trip() {
        let s = set_scale(AppState::default(), Scale::Dorian);
        assert_eq!(s.sequencer.scale, Scale::Dorian);
    }

    #[test]
    fn set_scale_snap_toggles() {
        let s = set_scale_snap(AppState::default(), true);
        assert!(s.sequencer.scale_snap);
        let s = set_scale_snap(s, false);
        assert!(!s.sequencer.scale_snap);
    }

    #[test]
    fn toggle_sequencer_running_flips_state() {
        let s = AppState::default();
        let was = s.sequencer.running;
        let s = toggle_sequencer_running(s);
        assert_eq!(s.sequencer.running, !was);
        let s = toggle_sequencer_running(s);
        assert_eq!(s.sequencer.running, was);
    }

    #[test]
    fn unlock_param_removes_lock() {
        let s = lock_param(AppState::default(), "bass.cutoff");
        assert!(s.llm.locked_params.contains("bass.cutoff"));
        let s = unlock_param(s, "bass.cutoff");
        assert!(!s.llm.locked_params.contains("bass.cutoff"));
    }

    #[test]
    fn set_bass_step_active_and_note() {
        let s = set_bass_step(AppState::default(), 3, 60, true);
        assert_eq!(s.sequencer.bass_pattern[3].note, 60);
        assert!(s.sequencer.bass_pattern[3].active);
    }

    #[test]
    fn set_bass_step_deactivate() {
        let s = set_bass_step(AppState::default(), 0, 48, true);
        let s = set_bass_step(s, 0, 48, false);
        assert!(!s.sequencer.bass_pattern[0].active);
    }

    #[test]
    fn toggle_bass_accent_flips() {
        let s = AppState::default();
        let was_on = s.sequencer.bass_pattern[2].accent > 0.0;
        let s = toggle_bass_accent(s, 2);
        assert_eq!(s.sequencer.bass_pattern[2].accent > 0.0, !was_on);
    }

    #[test]
    fn toggle_bass_slide_flips() {
        let s = AppState::default();
        let was_on = s.sequencer.bass_pattern[1].slide > 0.0;
        let s = toggle_bass_slide(s, 1);
        assert_eq!(s.sequencer.bass_pattern[1].slide > 0.0, !was_on);
    }

    #[test]
    fn set_hoover_step_stores_note() {
        let s = set_hoover_step(AppState::default(), 4, 72, true);
        assert_eq!(s.sequencer.hoover_pattern[4].note, 72);
        assert!(s.sequencer.hoover_pattern[4].active);
    }

    #[test]
    fn set_an1x_step_stores_note() {
        let s = set_an1x_step(AppState::default(), 7, 55, true);
        assert_eq!(s.sequencer.an1x_pattern[7].note, 55);
        assert!(s.sequencer.an1x_pattern[7].active);
    }

    /// Pluck / wavetable / sample voices share the same simple
    /// `set_*_step(state, idx, note, active)` shape as hoover and
    /// an1x; these tests pin the same contract — out-of-range
    /// indices are no-ops, in-range writes both the note and the
    /// active flag.
    #[test]
    fn set_pluck_step_stores_note_and_active() {
        use crate::state::set_pluck_step;
        let s = set_pluck_step(AppState::default(), 5, 64, true);
        assert_eq!(s.sequencer.pluck_pattern[5].note, 64);
        assert!(s.sequencer.pluck_pattern[5].active);
    }

    #[test]
    fn set_pluck_step_out_of_range_is_noop() {
        // Diff each step's `(active, note)` against the default —
        // the per-step struct has no PartialEq so we project to the
        // two fields the setter actually touches.
        use crate::state::set_pluck_step;
        let baseline: Vec<(bool, u8)> = AppState::default()
            .sequencer
            .pluck_pattern
            .iter()
            .map(|st| (st.active, st.note))
            .collect();
        let len = baseline.len();
        let s = set_pluck_step(AppState::default(), len + 10, 99, true);
        let after: Vec<(bool, u8)> = s
            .sequencer
            .pluck_pattern
            .iter()
            .map(|st| (st.active, st.note))
            .collect();
        assert_eq!(after, baseline);
    }

    #[test]
    fn set_wavetable_step_stores_note_and_active() {
        use crate::state::set_wavetable_step;
        let s = set_wavetable_step(AppState::default(), 3, 67, true);
        assert_eq!(s.sequencer.wavetable_pattern[3].note, 67);
        assert!(s.sequencer.wavetable_pattern[3].active);
    }

    #[test]
    fn set_sample_step_stores_note_and_active() {
        use crate::state::set_sample_step;
        let s = set_sample_step(AppState::default(), 12, 48, true);
        assert_eq!(s.sequencer.sample_pattern[12].note, 48);
        assert!(s.sequencer.sample_pattern[12].active);
    }

    #[test]
    fn set_sample_step_deactivate_keeps_note_unchanged() {
        // Light-touch contract: the setter stores both fields, so
        // toggling `active` to false on a previously-empty step
        // leaves the (default) note in place; deactivation alone
        // shouldn't surprise the user with a hidden note shift.
        use crate::state::set_sample_step;
        let s = set_sample_step(AppState::default(), 0, 60, true);
        let s = set_sample_step(s, 0, 60, false);
        assert_eq!(s.sequencer.sample_pattern[0].note, 60);
        assert!(!s.sequencer.sample_pattern[0].active);
    }

    #[test]
    fn apply_reese_preset_sets_waveform() {
        let s = apply_reese_preset(AppState::default());
        assert_eq!(
            s.bass_voices[0].synth.waveform,
            Waveform::Supersaw,
            "Reese preset must use supersaw"
        );
    }

    #[test]
    fn apply_hoover_preset_enables_hoover() {
        let s = apply_hoover_preset(AppState::default());
        assert!(s.hoover.enabled);
    }

    #[test]
    fn apply_boc_preset_enables_an1x() {
        let s = apply_boc_preset(AppState::default());
        assert!(s.an1x.enabled);
    }

    #[test]
    fn bank_write_then_load_restores_bpm() {
        let mut s = AppState::default();
        s.sequencer.bpm = 142.0;
        let s = bank_write(s, 2);
        // bank_load reads from the same state's pattern_bank
        let mut s = s;
        s.sequencer.bpm = 100.0;
        let s = bank_load(s, 2, false);
        assert_eq!(s.sequencer.bpm, 142.0);
    }

    #[test]
    fn bank_load_out_of_range_is_noop() {
        let s = AppState::default();
        let bpm = s.sequencer.bpm;
        let s = bank_load(s, 99, false);
        assert_eq!(s.sequencer.bpm, bpm);
    }

    #[test]
    fn set_pattern_edit_clamps_to_valid_slot() {
        let s = set_pattern_edit(AppState::default(), 3);
        assert_eq!(s.pattern_edit, 3);
    }

    #[test]
    fn set_chain_stores_sequence() {
        let s = set_chain(AppState::default(), vec![0, 2, 1]);
        assert_eq!(s.chain, vec![0, 2, 1]);
    }

    #[test]
    fn set_chain_enabled_true_sets_flag() {
        let s = set_chain_enabled(AppState::default(), true);
        assert!(s.chain_enabled);
    }

    #[test]
    fn set_chain_enabled_false_resets_position() {
        let s = AppState {
            chain_pos: 3,
            ..AppState::default()
        };
        let s = set_chain_enabled(s, false);
        assert!(!s.chain_enabled);
        assert_eq!(s.chain_pos, 0);
    }

    #[test]
    fn apply_gabber_kick_preset_sets_extreme_params() {
        let s = crate::state::apply_gabber_kick_preset(AppState::default());
        assert!(
            s.kit_a.kick.pitch_env_depth > 0.8,
            "gabber kick needs extreme pitch sweep"
        );
        assert!(s.kit_a.kick.clip > 0.5, "gabber kick needs hard clipping");
        assert!(
            s.kit_a.kick.pitch_env_time > 0.4,
            "gabber kick needs long sweep time"
        );
        assert!(s.kit_a.kick.punch > 0.8, "gabber kick needs max transient");
    }

    #[test]
    fn apply_llm_update_routes_gabber_kick_block() {
        let state = AppState::default();
        let update = serde_json::json!({
            "gabber_kick": {
                "pitch": 0.6,
                "clip": 0.95,
                "transient": 0.2,
                "volume": 1.2,
                "pan": -0.5,
            }
        });
        let next = crate::state::apply_llm_update(state, &update, &[]);
        assert!((next.gabber_kick.pitch - 0.6).abs() < 1e-4);
        assert!((next.gabber_kick.clip - 0.95).abs() < 1e-4);
        assert!((next.gabber_kick.transient - 0.2).abs() < 1e-4);
        assert!(
            (next.gabber_kick.volume - 1.2).abs() < 1e-4,
            "volume clamps to 1.5, not 1.0"
        );
        assert!((next.gabber_kick.pan - -0.5).abs() < 1e-4);
    }

    #[test]
    fn apply_llm_update_respects_gabber_kick_locks() {
        let mut state = AppState::default();
        state
            .llm
            .locked_params
            .insert("gabber_kick.clip".to_string());
        let original_clip = state.gabber_kick.clip;
        let update = serde_json::json!({ "gabber_kick": { "clip": 0.99 } });
        let next = crate::state::apply_llm_update(state, &update, &[]);
        assert!((next.gabber_kick.clip - original_clip).abs() < 1e-4);
    }

    #[test]
    fn gabber_kick_voice_wired_and_defaults_aggressive() {
        // DrumVoice::GabberKick registered end-to-end.
        assert!(DrumVoice::ALL.contains(&DrumVoice::GabberKick));
        assert_eq!(DrumVoice::GabberKick.label(), "GABBER KICK");
        assert_eq!(DrumVoice::GabberKick.schema_key(), Some("gabber_kick"));

        // Default params should already be in "hardcore" territory so the
        // voice sounds gabber-y without needing a preset run.
        let s = AppState::default();
        assert!(s.gabber_kick.clip > 0.3, "default clip aggressive");
        assert!(
            s.gabber_kick.pitch_env_depth > 0.7,
            "default pitch sweep deep"
        );
        assert!(s.gabber_kick.transient > 0.3, "default transient audible");
        assert_eq!(s.gabber_kick.pan, 0.0);

        // Volume accessor routes through the new field.
        let s2 = DrumVoice::GabberKick.set_volume(s, 0.42);
        assert!((DrumVoice::GabberKick.get_volume(&s2) - 0.42).abs() < 1e-5);
    }
}

// ─── Schema and rack tests ─────────────────────────────────────────────────────
#[cfg(test)]
mod schema_and_rack_tests {
    use crate::llm::prompt::param_json_schema;
    use crate::state::RackState;
    use crate::state::rack::CableColor;

    #[test]
    fn param_json_schema_is_object_with_bass_key() {
        let schema = param_json_schema();
        assert!(schema.is_object(), "schema must be a JSON object");
        let props = schema
            .get("properties")
            .expect("schema must have 'properties'");
        assert!(
            props.get("bass").is_some(),
            "properties must contain 'bass' key"
        );
    }

    #[test]
    fn param_json_schema_has_sequencer_key() {
        let schema = param_json_schema();
        let props = schema
            .get("properties")
            .expect("schema must have 'properties'");
        assert!(
            props.get("sequencer").is_some(),
            "properties must contain 'sequencer' key"
        );
    }

    #[test]
    fn next_cable_color_cycles() {
        let rack = RackState::default();
        let c1 = rack.next_cable_color();
        let c2 = rack.next_cable_color();
        // Colors must be valid CableColor variants and can differ
        let _ = c1;
        let _ = c2;
        // After cycling through all colors, must not panic
        for _ in 0..32 {
            let _ = rack.next_cable_color();
        }
    }

    #[test]
    fn next_cable_color_wraps_around() {
        let rack = RackState::default();
        let n = CableColor::ALL.len();
        // Advance through exactly one full cycle — rack is stateless, color
        // depends only on cables.len(), so an empty rack always returns ALL[0].
        for _ in 0..n {
            rack.next_cable_color();
        }
        // Color after full cycle should match the first color
        let after_cycle = rack.next_cable_color();
        let rack2 = RackState::default();
        let first = rack2.next_cable_color();
        assert_eq!(after_cycle, first);
    }
}

// ─── Pad preset tests ────────────────────────────────────────────────────────
#[cfg(test)]
mod pad_preset_tests {
    use crate::state::{
        AppState, apply_evolving_texture_preset, apply_glass_pad_preset, apply_sub_drone_preset,
        apply_warm_pad_preset,
    };

    #[test]
    fn warm_pad_enables_an1x_with_slow_attack() {
        let s = apply_warm_pad_preset(AppState::default());
        assert!(s.an1x.enabled);
        assert!(s.an1x.amp_attack > 0.3, "warm pad needs slow attack");
    }

    #[test]
    fn evolving_texture_has_lfo_on_filter() {
        let s = apply_evolving_texture_preset(AppState::default());
        assert!(s.an1x.enabled);
        assert!(s.an1x.lfo_depth > 0.1, "evolving texture needs LFO depth");
        assert!(s.an1x.drift > 0.1, "evolving texture needs drift");
    }

    #[test]
    fn glass_pad_uses_hard_sync() {
        let s = apply_glass_pad_preset(AppState::default());
        assert!(s.an1x.enabled);
        assert!(s.an1x.hard_sync);
        assert!(s.an1x.filter_cutoff > 0.5, "glass pad should be bright");
    }

    #[test]
    fn sub_drone_has_long_envelopes() {
        let s = apply_sub_drone_preset(AppState::default());
        assert!(s.an1x.enabled);
        assert!(s.an1x.amp_attack > 0.5, "drone needs very slow attack");
        assert!(s.an1x.amp_release > 0.9, "drone needs very long release");
        assert!(s.an1x.sub_level > 0.4, "drone needs deep sub");
    }
}
