// ─── tests/llm_apply_voice_tests.rs ──────────────────────────────────────────
// Per-melodic-voice and preecho LLM apply tests.  Covers the helpers
// extracted into state/llm_apply_seq.rs in the round-2 refactor:
// SeqScope, apply_melodic_lane_lens, apply_bass_notes, apply_bass_pans,
// apply_preecho_voices.
//
// Tests exercise the helpers via the public apply_llm_update so the full
// scope-resolution / lock-path stack is in scope on every assertion.

#[cfg(test)]
mod seq_scope_tests {
    use crate::state::llm_apply_seq::SeqScope;

    #[test]
    fn empty_scope_grants_everything() {
        let s = SeqScope::from_scope(&[]);
        assert!(s.seq);
        assert!(s.bass);
        assert!(s.hoover);
        assert!(s.an1x);
        assert!(s.kit_a);
        assert!(s.kit_b);
        assert!(s.amen);
        assert!(s.any());
    }

    #[test]
    fn explicit_sequencer_scope_grants_everything() {
        let s = SeqScope::from_scope(&["sequencer".to_string()]);
        assert!(s.seq);
        assert!(s.bass && s.hoover && s.an1x && s.kit_a && s.kit_b && s.amen);
    }

    #[test]
    fn voice_scope_only_unlocks_its_own_slice() {
        let s = SeqScope::from_scope(&["bass".to_string()]);
        assert!(!s.seq, "global sequencer fields should still be off");
        assert!(s.bass);
        assert!(!s.hoover);
        assert!(!s.an1x);
        assert!(!s.kit_a);
        assert!(s.any());
    }

    #[test]
    fn empty_voice_list_with_no_match_zeros_everything() {
        let s = SeqScope::from_scope(&["fx".to_string()]);
        assert!(!s.seq);
        assert!(!s.bass);
        assert!(!s.any());
    }

    #[test]
    fn for_preecho_voice_routes_to_matching_flag() {
        let s = SeqScope::from_scope(&["bass".to_string()]);
        assert!(s.for_preecho_voice("bass"));
        assert!(!s.for_preecho_voice("hoover"));
    }

    #[test]
    fn for_preecho_voice_unknown_falls_back_to_seq_scope() {
        // A bass-only scope should not let an unknown voice key through.
        let s = SeqScope::from_scope(&["bass".to_string()]);
        assert!(!s.for_preecho_voice("synth-z"));
        // Sequencer-wide scope grants the unknown key.
        let s = SeqScope::from_scope(&["sequencer".to_string()]);
        assert!(s.for_preecho_voice("synth-z"));
    }
}

#[cfg(test)]
mod melodic_lane_lens_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn bass_len_resizes_via_set_lane_steps() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_len": 24 } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_steps, 24);
    }

    #[test]
    fn hoover_len_writes_only_the_hoover_lane() {
        let s = AppState::default();
        let before_bass = s.sequencer.bass_steps;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "hoover_len": 12 } }),
            &[],
        );
        assert_eq!(s.sequencer.hoover_steps, 12);
        assert_eq!(s.sequencer.bass_steps, before_bass);
    }

    #[test]
    fn an1x_len_outside_an1x_scope_is_ignored() {
        let s = AppState::default();
        let before = s.sequencer.an1x_steps;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "an1x_len": 8 } }),
            &["bass".to_string()],
        );
        assert_eq!(s.sequencer.an1x_steps, before);
    }

    #[test]
    fn locked_bass_len_path_is_preserved() {
        let mut s = AppState::default();
        s.sequencer.bass_steps = 16;
        s.llm.locked_params.insert("sequencer.bass_len".to_string());
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_len": 32 } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_steps, 16);
    }
}

#[cfg(test)]
mod bass_notes_tests {
    use crate::state::{AppState, Scale, apply_llm_update};

    #[test]
    fn notes_write_through_to_bass_pattern_and_voice_0_mirror() {
        let mut s = AppState::default();
        s.sequencer.scale_snap = false;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_notes": [60, 62, 64, 65] } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_pattern[0].note, 60);
        assert_eq!(s.sequencer.bass_pattern[3].note, 65);
        // Voice 0 mirror stays in lockstep.
        assert_eq!(s.sequencer.bass_patterns[0][0].note, 60);
        assert_eq!(s.sequencer.bass_patterns[0][3].note, 65);
    }

    #[test]
    fn notes_clamp_to_midi_range() {
        let mut s = AppState::default();
        s.sequencer.scale_snap = false;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_notes": [9999] } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_pattern[0].note, 127);
    }

    #[test]
    fn scale_snap_rounds_to_nearest_scale_degree() {
        // C major root: snapping should leave 60 (C) alone but round 61
        // (C#, not in major) to one of its scale neighbours.
        let mut s = AppState::default();
        s.sequencer.scale_snap = true;
        s.sequencer.root_note = 0;
        s.sequencer.scale = Scale::Major;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_notes": [60, 61] } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_pattern[0].note, 60, "C survives snap");
        let snapped = s.sequencer.bass_pattern[1].note;
        assert!(
            snapped == 60 || snapped == 62,
            "C# snaps to C or D, got {snapped}"
        );
    }

    #[test]
    fn locked_bass_notes_are_preserved() {
        let mut s = AppState::default();
        s.sequencer.bass_pattern[0].note = 36;
        s.llm
            .locked_params
            .insert("sequencer.bass_notes".to_string());
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_notes": [72] } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_pattern[0].note, 36);
    }

    #[test]
    fn bass_notes_outside_bass_scope_skip() {
        let s = AppState::default();
        let before = s.sequencer.bass_pattern[0].note;
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_notes": [50] } }),
            &["hoover".to_string()],
        );
        assert_eq!(s.sequencer.bass_pattern[0].note, before);
    }
}

#[cfg(test)]
mod bass_pans_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn pans_clamp_to_signed_unit_range() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_pans": [-5.0, 0.5, 5.0] } }),
            &[],
        );
        assert_eq!(s.sequencer.bass_pattern[0].pan, -1.0);
        assert!((s.sequencer.bass_pattern[1].pan - 0.5).abs() < 1e-6);
        assert_eq!(s.sequencer.bass_pattern[2].pan, 1.0);
    }

    #[test]
    fn pans_mirror_into_voice_0_pattern() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_pans": [0.7] } }),
            &[],
        );
        assert!((s.sequencer.bass_patterns[0][0].pan - 0.7).abs() < 1e-6);
    }

    #[test]
    fn locked_bass_pans_are_preserved() {
        let mut s = AppState::default();
        s.sequencer.bass_pattern[0].pan = 0.3;
        s.llm
            .locked_params
            .insert("sequencer.bass_pans".to_string());
        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "bass_pans": [-0.9] } }),
            &[],
        );
        assert!((s.sequencer.bass_pattern[0].pan - 0.3).abs() < 1e-6);
    }
}

#[cfg(test)]
mod preecho_voices_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn writing_anchors_creates_a_preecho_entry() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "sequencer": { "preecho": { "kit_a": {
                    "enabled": true,
                    "anchors": [0, 8],
                    "length": 4,
                    "velocity_ramp": true
                }}}
            }),
            &[],
        );
        let entry = s.sequencer.preecho.get("kit_a").expect("kit_a entry");
        assert_eq!(entry.anchors, vec![0, 8]);
        assert_eq!(entry.length, 4);
        assert!(entry.velocity_ramp);
        assert!(entry.enabled);
    }

    #[test]
    fn null_value_clears_a_voice_entry() {
        let mut s = AppState::default();
        let mut cfg = crate::sequencer::PreechoConfig::default();
        cfg.anchors = vec![0, 8];
        cfg.length = 4;
        s.sequencer.preecho.insert("kit_a".to_string(), cfg);

        let s = apply_llm_update(
            s,
            &serde_json::json!({ "sequencer": { "preecho": { "kit_a": null } } }),
            &[],
        );
        assert!(s.sequencer.preecho.get("kit_a").is_none());
    }

    #[test]
    fn anchor_indices_clamp_to_max_step_index_63() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "sequencer": { "preecho": { "kit_a": { "anchors": [99, 200] } } }
            }),
            &[],
        );
        let entry = s.sequencer.preecho.get("kit_a").unwrap();
        assert_eq!(entry.anchors, vec![63, 63]);
    }

    #[test]
    fn length_clamps_at_16() {
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "sequencer": { "preecho": { "kit_a": { "length": 99 } } }
            }),
            &[],
        );
        assert_eq!(s.sequencer.preecho.get("kit_a").unwrap().length, 16);
    }

    #[test]
    fn locked_preecho_path_skips_that_voice_only() {
        let mut s = AppState::default();
        s.llm
            .locked_params
            .insert("sequencer.preecho.kit_a".to_string());
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "sequencer": { "preecho": {
                    "kit_a": { "anchors": [0, 8] },
                    "kit_b": { "anchors": [4, 12] }
                }}
            }),
            &[],
        );
        assert!(s.sequencer.preecho.get("kit_a").is_none());
        let kb = s.sequencer.preecho.get("kit_b").expect("kit_b unlocked");
        assert_eq!(kb.anchors, vec![4, 12]);
    }

    #[test]
    fn preecho_voice_outside_its_scope_is_skipped() {
        // bass-scope agent shouldn't be able to write kit_a's preecho.
        let s = AppState::default();
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "sequencer": { "preecho": { "kit_a": { "anchors": [0, 8] } } }
            }),
            &["bass".to_string()],
        );
        assert!(s.sequencer.preecho.get("kit_a").is_none());
    }

    #[test]
    fn partial_update_preserves_existing_fields() {
        let mut s = AppState::default();
        let mut cfg = crate::sequencer::PreechoConfig::default();
        cfg.anchors = vec![0, 8];
        cfg.length = 4;
        cfg.velocity_ramp = true;
        s.sequencer.preecho.insert("kit_a".to_string(), cfg);

        // Update only the length — other fields should survive.
        let s = apply_llm_update(
            s,
            &serde_json::json!({
                "sequencer": { "preecho": { "kit_a": { "length": 6 } } }
            }),
            &[],
        );
        let entry = s.sequencer.preecho.get("kit_a").unwrap();
        assert_eq!(entry.length, 6);
        assert_eq!(entry.anchors, vec![0, 8]);
        assert!(entry.velocity_ramp);
    }
}
