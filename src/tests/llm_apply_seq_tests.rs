// ─── tests/llm_apply_seq_tests.rs ────────────────────────────────────────────
// Sequencer-scoped LLM apply tests: top-level sequencer fields (swing,
// BPM, steps, etc.), the Amen-sampler lane, and the Euclidean-rhythm
// update path.  Split out of llm_apply_extra_tests.rs to stay under the
// 1000-line cap; the rack / scope / combined / step-array tests stay in
// the original file.

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

    // Regression: prior to the unlocked_f32_range fix the inner clamp(0,1)
    // pinned amen.pitch to ±1.0, so a +12 / -12 semitone request became
    // +1.0 / 0.0.  These two tests would have failed against the old code.
    #[test]
    fn pitch_accepts_full_semitone_range() {
        let s = AppState::default();
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "pitch": 12.0 } }), &[]);
        assert!((s.amen.pitch - 12.0).abs() < 1e-6);
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "pitch": -7.0 } }), &[]);
        assert!((s.amen.pitch - -7.0).abs() < 1e-6);
    }

    #[test]
    fn pitch_clamps_at_plus_minus_24() {
        let s = AppState::default();
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "pitch": 99.0 } }), &[]);
        assert_eq!(s.amen.pitch, 24.0);
        let s = apply_llm_update(s, &serde_json::json!({ "amen": { "pitch": -99.0 } }), &[]);
        assert_eq!(s.amen.pitch, -24.0);
    }

    // Regression: source_bpm always ended up pinned to 40 because the inner
    // clamp(0,1) returned 1.0, then clamp(40, 300) snapped to 40.
    #[test]
    fn source_bpm_accepts_full_range() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "source_bpm": 174.0 } }),
            &[],
        );
        assert!((s.amen.source_bpm - 174.0).abs() < 1e-6);
    }

    #[test]
    fn source_bpm_clamps_to_40_300() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "source_bpm": 600.0 } }),
            &[],
        );
        assert_eq!(s.amen.source_bpm, 300.0);
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "source_bpm": 5.0 } }),
            &[],
        );
        assert_eq!(s.amen.source_bpm, 40.0);
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
    fn slice_reverses_apply_from_bool_array() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_reverses": [true, false, true, false] } }),
            &[],
        );
        assert_eq!(s.amen.slice_reverses, vec![true, false, true, false]);
    }

    #[test]
    fn slice_reverses_accept_integer_0_1() {
        // Older Gemma outputs sometimes emit 0/1 integers for bools — the
        // apply layer tolerates both.
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_reverses": [1, 0, 1] } }),
            &[],
        );
        assert_eq!(s.amen.slice_reverses, vec![true, false, true]);
    }

    #[test]
    fn slice_reverses_null_clears_the_vec() {
        let mut s = AppState::default();
        s.amen.slice_reverses = vec![true, false, true];
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_reverses": null } }),
            &[],
        );
        assert!(s.amen.slice_reverses.is_empty());
    }

    #[test]
    fn slice_reverses_truncates_at_sixteen() {
        let s = AppState::default();
        let arr: Vec<bool> = (0..20).map(|i| i % 2 == 0).collect();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_reverses": arr } }),
            &[],
        );
        assert_eq!(s.amen.slice_reverses.len(), 16);
    }

    #[test]
    fn slice_reverses_locked_field_is_preserved() {
        let mut s = AppState::default();
        s.amen.slice_reverses = vec![true, true];
        s.llm
            .locked_params
            .insert("amen.slice_reverses".to_string());
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "amen": { "slice_reverses": [false, false, false] } }),
            &[],
        );
        assert_eq!(s.amen.slice_reverses, vec![true, true]);
    }

    #[test]
    fn params_encode_slice_reverses_with_minus_one_sentinel() {
        // Empty Vec → every slot stays at -1 (inherit global).
        use crate::audio::AudioParams;
        let s = AppState::default();
        let p = AudioParams::from_app_state(&s);
        assert!(p.amen_slice_reverses.iter().all(|&x| x == -1));
    }

    #[test]
    fn params_encode_populated_slice_reverses_as_0_and_1() {
        use crate::audio::AudioParams;
        let mut s = AppState::default();
        s.amen.slice_reverses = vec![true, false, true];
        let p = AudioParams::from_app_state(&s);
        assert_eq!(&p.amen_slice_reverses[..5], &[1, 0, 1, -1, -1]);
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
