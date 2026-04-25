// ─── tests/chain_advance_tests.rs ────────────────────────────────────────────
// Pure decision logic for chain advancement.  Used to live inline in
// the audio thread; lifted to `state/chain_advance.rs` so every branch
// has a unit test.

#[cfg(test)]
mod classify {
    use crate::state::{
        ChainSlotOverride, LoopBoundaryAction, SequencerState, classify_loop_boundary,
    };

    fn ovr_repeats(n: u8) -> ChainSlotOverride {
        ChainSlotOverride {
            repeats: n,
            ..ChainSlotOverride::default()
        }
    }

    fn ovr_morph(bars: u8) -> ChainSlotOverride {
        ChainSlotOverride {
            morph_bars: bars,
            ..ChainSlotOverride::default()
        }
    }

    fn ovr_bpm(bpm: f32) -> ChainSlotOverride {
        ChainSlotOverride {
            bpm: Some(bpm),
            ..ChainSlotOverride::default()
        }
    }

    fn ovr_style(id: &str) -> ChainSlotOverride {
        ChainSlotOverride {
            style: Some(id.to_string()),
            ..ChainSlotOverride::default()
        }
    }

    #[test]
    fn empty_chain_yields_none() {
        let action = classify_loop_boundary(&[], &[], 0, 0, true, None, 120.0, 0.0);
        assert_eq!(action, LoopBoundaryAction::None);
    }

    #[test]
    fn single_slot_with_default_repeats_advances_immediately() {
        // A 1-slot chain with default repeats=1 wraps back to slot 0
        // every loop.  No bump-counter step.
        let action = classify_loop_boundary(
            &[3],
            &[ChainSlotOverride::default()],
            0,
            0,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        assert!(matches!(action, LoopBoundaryAction::AdvanceTo { .. }));
    }

    #[test]
    fn override_repeats_three_bumps_counter_twice_then_advances() {
        let chain = vec![0, 1];
        let overrides = vec![ovr_repeats(3), ChainSlotOverride::default()];
        // Counter 0 → 1
        let a = classify_loop_boundary(
            &chain,
            &overrides,
            0,
            0,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        assert_eq!(a, LoopBoundaryAction::BumpRepeatCount(1));
        // Counter 1 → 2
        let a = classify_loop_boundary(
            &chain,
            &overrides,
            0,
            1,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        assert_eq!(a, LoopBoundaryAction::BumpRepeatCount(2));
        // Counter 2 → advance (3 plays of the slot done).
        let a = classify_loop_boundary(
            &chain,
            &overrides,
            0,
            2,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        assert!(matches!(
            a,
            LoopBoundaryAction::AdvanceTo { next_pos: 1, .. }
        ));
    }

    #[test]
    fn one_shot_song_stops_at_end() {
        // chain_loop=false + cur_pos at last slot + counter exhausted →
        // StopAtEnd.  This is the MIDI-import path.
        let chain = vec![0, 1, 2];
        let action = classify_loop_boundary(
            &chain,
            &vec![ChainSlotOverride::default(); 3],
            2, // last slot
            0, // last repeat done
            false,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        assert_eq!(action, LoopBoundaryAction::StopAtEnd);
    }

    #[test]
    fn one_shot_song_wraps_repeats_within_last_slot() {
        // chain_loop=false but the LAST slot still has repeats > 1 —
        // we should bump the counter, not StopAtEnd.
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ovr_repeats(3)];
        let action = classify_loop_boundary(
            &chain,
            &overrides,
            1,
            0,
            false,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        assert_eq!(action, LoopBoundaryAction::BumpRepeatCount(1));
    }

    #[test]
    fn looped_song_at_last_slot_wraps_to_first() {
        let chain = vec![5, 6, 7];
        let action = classify_loop_boundary(
            &chain,
            &vec![ChainSlotOverride::default(); 3],
            2, // last slot
            0,
            true, // chain_loop on
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        match action {
            LoopBoundaryAction::AdvanceTo {
                next_pos,
                next_slot,
                ..
            } => {
                assert_eq!(next_pos, 0);
                assert_eq!(next_slot, 5);
            }
            other => panic!("expected AdvanceTo to slot 5, got {other:?}"),
        }
    }

    #[test]
    fn morph_bars_carry_through_to_advance_action() {
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ovr_morph(4)];
        let action = classify_loop_boundary(
            &chain,
            &overrides,
            0,
            0,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        match action {
            LoopBoundaryAction::AdvanceTo { morph_bars, .. } => assert_eq!(morph_bars, 4),
            other => panic!("expected AdvanceTo with morph_bars=4, got {other:?}"),
        }
    }

    #[test]
    fn bpm_override_wins_over_loaded_bpm_apply() {
        // Override sets BPM to 142; loaded.pattern_bpm_apply is true
        // with bpm=128.  Override wins (the next-slot override is
        // evaluated FIRST, so the loaded apply flag doesn't matter).
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ovr_bpm(142.0)];
        let mut loaded = SequencerState::default();
        loaded.bpm = 128.0;
        loaded.pattern_bpm_apply = true;
        let action =
            classify_loop_boundary(&chain, &overrides, 0, 0, true, Some(&loaded), 120.0, 0.0);
        match action {
            LoopBoundaryAction::AdvanceTo { eff_bpm, .. } => {
                assert!((eff_bpm - 142.0).abs() < 1e-6, "got {eff_bpm}");
            }
            other => panic!("expected AdvanceTo, got {other:?}"),
        }
    }

    #[test]
    fn loaded_bpm_apply_wins_over_prior_when_no_override() {
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ChainSlotOverride::default()];
        let mut loaded = SequencerState::default();
        loaded.bpm = 95.0;
        loaded.pattern_bpm_apply = true;
        let action =
            classify_loop_boundary(&chain, &overrides, 0, 0, true, Some(&loaded), 120.0, 0.0);
        match action {
            LoopBoundaryAction::AdvanceTo { eff_bpm, .. } => {
                assert!((eff_bpm - 95.0).abs() < 1e-6, "got {eff_bpm}");
            }
            other => panic!("expected AdvanceTo, got {other:?}"),
        }
    }

    #[test]
    fn prior_bpm_preserved_when_neither_override_nor_apply() {
        // Default override (no bpm), default loaded (pattern_bpm_apply=false)
        // → keep the prior BPM (the chain doesn't auto-jump tempo).
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ChainSlotOverride::default()];
        let action = classify_loop_boundary(
            &chain,
            &overrides,
            0,
            0,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        match action {
            LoopBoundaryAction::AdvanceTo { eff_bpm, .. } => {
                assert!((eff_bpm - 120.0).abs() < 1e-6, "got {eff_bpm}");
            }
            other => panic!("expected AdvanceTo, got {other:?}"),
        }
    }

    #[test]
    fn style_override_wins_over_loaded_pattern_style() {
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ovr_style("jungle")];
        let mut loaded = SequencerState::default();
        loaded.pattern_style = Some("ambient".to_string());
        let action =
            classify_loop_boundary(&chain, &overrides, 0, 0, true, Some(&loaded), 120.0, 0.0);
        match action {
            LoopBoundaryAction::AdvanceTo {
                effective_style, ..
            } => {
                assert_eq!(effective_style.as_deref(), Some("jungle"));
            }
            other => panic!("expected AdvanceTo, got {other:?}"),
        }
    }

    #[test]
    fn loaded_pattern_style_used_when_no_override() {
        let chain = vec![0, 1];
        let overrides = vec![ChainSlotOverride::default(), ChainSlotOverride::default()];
        let mut loaded = SequencerState::default();
        loaded.pattern_style = Some("ambient".to_string());
        let action =
            classify_loop_boundary(&chain, &overrides, 0, 0, true, Some(&loaded), 120.0, 0.0);
        match action {
            LoopBoundaryAction::AdvanceTo {
                effective_style, ..
            } => {
                assert_eq!(effective_style.as_deref(), Some("ambient"));
            }
            other => panic!("expected AdvanceTo, got {other:?}"),
        }
    }

    #[test]
    fn missing_overrides_default_to_repeats_one_and_morph_zero() {
        // A chain whose `overrides` is shorter than `chain` — common
        // for sessions saved before song mode existed.  Should treat
        // missing slots as default (repeats=1, morph_bars=0).
        let chain = vec![0, 1, 2];
        let overrides: Vec<ChainSlotOverride> = vec![]; // empty
        let action = classify_loop_boundary(
            &chain,
            &overrides,
            0,
            0,
            true,
            Some(&SequencerState::default()),
            120.0,
            0.0,
        );
        match action {
            LoopBoundaryAction::AdvanceTo {
                morph_bars,
                next_pos,
                ..
            } => {
                assert_eq!(morph_bars, 0);
                assert_eq!(next_pos, 1);
            }
            other => panic!("expected AdvanceTo, got {other:?}"),
        }
    }

    #[test]
    fn classify_is_deterministic() {
        // Sanity: same inputs → same output, every time.
        let chain = vec![0, 1, 2];
        let overrides = vec![ChainSlotOverride::default(); 3];
        let inputs = (
            chain,
            overrides,
            1usize,
            0u8,
            true,
            SequencerState::default(),
            128.0_f32,
            0.1_f32,
        );
        for _ in 0..3 {
            let action = classify_loop_boundary(
                &inputs.0,
                &inputs.1,
                inputs.2,
                inputs.3,
                inputs.4,
                Some(&inputs.5),
                inputs.6,
                inputs.7,
            );
            assert!(matches!(
                action,
                LoopBoundaryAction::AdvanceTo { next_pos: 2, .. }
            ));
        }
    }
}

#[cfg(test)]
mod build_target {
    use crate::state::{SequencerState, build_advance_target};

    #[test]
    fn looped_path_replaces_full_sequencer_with_target() {
        let mut prior = SequencerState::default();
        prior.bpm = 120.0;
        let mut loaded = SequencerState::default();
        loaded.bpm = 95.0;
        // chain_loop=true → chain_advance_transport: keeps loaded
        // sequencer; eff_bpm wins.
        let target = build_advance_target(loaded, &prior, true, 142.0, 0.2, true);
        assert!((target.bpm - 142.0).abs() < 1e-6);
        assert!((target.swing - 0.2).abs() < 1e-6);
        assert!(target.running);
    }

    #[test]
    fn one_shot_path_preserves_non_bass_state_from_prior() {
        // chain_loop=false → chain_advance_preserve_non_bass:
        // bass-only swap, drum_steps / hoover etc come from prior.
        let mut prior = SequencerState::default();
        prior.hoover_steps = 7;
        let mut loaded = SequencerState::default();
        loaded.hoover_steps = 16;
        let target = build_advance_target(loaded, &prior, false, 120.0, 0.0, true);
        assert_eq!(
            target.hoover_steps, 7,
            "preserve_non_bass should restore hoover_steps from prior"
        );
    }

    #[test]
    fn pattern_bpm_apply_is_set_to_true_on_target() {
        // Both paths must set pattern_bpm_apply=true on the target —
        // the resolved BPM has already been baked in, so future code
        // shouldn't re-derive it from the slot's `pattern_bpm_apply`.
        let prior = SequencerState::default();
        let loaded = SequencerState::default();
        let target = build_advance_target(loaded, &prior, true, 130.0, 0.0, true);
        assert!(target.pattern_bpm_apply);
    }
}
