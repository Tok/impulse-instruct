// ─── tests/morph_tests.rs ────────────────────────────────────────────────────
// Pattern morph on chain advance — bit-reversal dispersal, threshold
// gating, per-tick step replacement, completion semantics.

#[cfg(test)]
mod bit_reverse_rank_tests {
    use crate::state::bit_reverse_rank;

    #[test]
    fn power_of_two_disperses_evenly() {
        // For len=16, the first four ranks should be evenly dispersed:
        // bit_reverse(0..16) over 4-bit indices = [0,8,4,12,2,10,6,14, ...]
        let expected = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
        for (i, &want) in expected.iter().enumerate() {
            assert_eq!(
                bit_reverse_rank(i, 16),
                want,
                "rank({i}, 16) — expected {want}"
            );
        }
    }

    #[test]
    fn len_one_or_zero_returns_zero() {
        assert_eq!(bit_reverse_rank(0, 0), 0);
        assert_eq!(bit_reverse_rank(0, 1), 0);
    }

    #[test]
    fn ranks_stay_in_range() {
        // Even non-power-of-two lengths must produce ranks in [0, len).
        for len in [3, 5, 7, 9, 17, 32, 33, 60] {
            for i in 0..len {
                let r = bit_reverse_rank(i, len);
                assert!(r < len, "rank({i}, {len}) = {r} out of range");
            }
        }
    }
}

#[cfg(test)]
mod step_swapped_tests {
    use crate::state::step_swapped;

    #[test]
    fn no_swap_at_zero_progress() {
        for i in 0..16 {
            assert!(!step_swapped(i, 16, 0, 4));
        }
    }

    #[test]
    fn full_swap_at_full_progress() {
        for i in 0..16 {
            assert!(step_swapped(i, 16, 4, 4));
        }
    }

    #[test]
    fn quarter_progress_swaps_a_quarter() {
        // bars_done=1, bars_total=4 → threshold = 16 * 1 / 4 = 4
        // Indices whose rank < 4 get swapped.
        let count = (0..16).filter(|&i| step_swapped(i, 16, 1, 4)).count();
        assert_eq!(count, 4, "expected 4 indices swapped at 1/4 progress");
    }

    #[test]
    fn dispersal_swaps_index_zero_first() {
        // rank(0)=0 always — so index 0 is first to swap.
        assert!(step_swapped(0, 16, 1, 16));
        // And index 8 (rank 1) is second.
        assert!(step_swapped(8, 16, 2, 16));
        // Index 1 (rank 8 in 16-step bit-reverse) is later.
        assert!(!step_swapped(1, 16, 4, 16));
    }
}

#[cfg(test)]
mod morph_tick_tests {
    use crate::state::{ChainMorph, DrumVoice, SequencerState, Step, TB303Step, morph_tick};

    fn seq_with_kick_at(indices: &[usize]) -> SequencerState {
        let mut s = SequencerState::default();
        if let Some(p) = s.drum_patterns.get_mut(&DrumVoice::Kick808) {
            for st in p.iter_mut() {
                *st = Step::default();
            }
            for &i in indices {
                p[i].active = true;
            }
        }
        s
    }

    #[test]
    fn final_tick_returns_target_wholesale() {
        let live = seq_with_kick_at(&[0, 4, 8, 12]);
        let target = seq_with_kick_at(&[2, 6, 10, 14]);
        let mut morph = ChainMorph::new(target.clone(), 1);
        let out = morph_tick(live, &mut morph);
        assert!(morph.is_complete());
        let active: Vec<_> = out.drum_patterns[&DrumVoice::Kick808]
            .iter()
            .enumerate()
            .filter(|(_, s)| s.active)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(active, vec![2, 6, 10, 14]);
    }

    #[test]
    fn intermediate_tick_replaces_only_dispersed_indices() {
        // Live: kick at every step (full pattern). Target: no kicks
        // anywhere.  Morph_total=4 over a 64-long pattern → after one
        // tick the threshold is 64*1/4=16 indices, ordered by
        // bit_reverse_rank.  Those 16 get sourced from target (= cleared);
        // the rest keep the live state.  Expect exactly 64-16 = 48
        // active indices remaining.
        let live = seq_with_kick_at(&(0..64).collect::<Vec<_>>());
        let target = seq_with_kick_at(&[]);
        let mut morph = ChainMorph::new(target, 4);
        let out = morph_tick(live, &mut morph);
        assert_eq!(morph.bars_done, 1);
        assert!(!morph.is_complete());
        let active = out.drum_patterns[&DrumVoice::Kick808]
            .iter()
            .filter(|s| s.active)
            .count();
        assert_eq!(active, 48, "expected 64 - 16 = 48 indices to remain active");
        // Index 0 (rank 0 in 64-step bit-reverse) is always swapped first.
        assert!(!out.drum_patterns[&DrumVoice::Kick808][0].active);
        // Index 1 has rank 32 in 64-step bit-reverse (>= threshold 16),
        // so it must keep the live state (active).
        assert!(out.drum_patterns[&DrumVoice::Kick808][1].active);
    }

    #[test]
    fn morph_does_not_advance_step_count_or_tempo() {
        // Steps and BPM live on SequencerState too, but morph_tick
        // shouldn't touch them mid-morph — only the per-step pattern
        // arrays evolve.  (Final tick replaces wholesale, so those
        // do swap on completion.)
        let mut live = SequencerState::default();
        live.bpm = 100.0;
        live.steps = 32;
        let mut target = SequencerState::default();
        target.bpm = 200.0;
        target.steps = 16;
        let mut morph = ChainMorph::new(target, 4);
        let out = morph_tick(live, &mut morph);
        assert!((out.bpm - 100.0).abs() < 1e-6, "BPM must not morph mid-way");
        assert_eq!(out.steps, 32, "steps must not morph mid-way");
    }

    #[test]
    fn bass_pattern_progressively_replaces() {
        // Voice 0 bass — set live to all-active, target to all-inactive.
        let mut live = SequencerState::default();
        for st in live.bass_patterns[0].iter_mut() {
            *st = TB303Step {
                active: true,
                note: 36,
                ..TB303Step::default()
            };
        }
        let target = SequencerState::default(); // empty
        let mut morph = ChainMorph::new(target, 2);
        let out = morph_tick(live, &mut morph);
        // Half the steps should be swapped (cleared).
        let active = out.bass_patterns[0].iter().filter(|s| s.active).count();
        let len = out.bass_patterns[0].len();
        assert_eq!(active, len - len / 2);
    }

    #[test]
    fn morph_clamps_bars_total_to_eight() {
        let target = SequencerState::default();
        let m = ChainMorph::new(target.clone(), 100);
        assert_eq!(m.bars_total, 8);
        let m = ChainMorph::new(target, 0);
        assert_eq!(m.bars_total, 1, "zero is clamped to 1 (no-op morph)");
    }

    #[test]
    fn override_default_morph_bars_is_zero() {
        let o = crate::state::ChainSlotOverride::default();
        assert_eq!(o.morph_bars, 0);
        assert!(o.is_empty(), "default override is empty");
    }

    #[test]
    fn override_with_morph_is_not_empty() {
        let o = crate::state::ChainSlotOverride {
            morph_bars: 4,
            ..Default::default()
        };
        assert!(!o.is_empty());
    }
}
