// ─── tests/song_mode_tests.rs ────────────────────────────────────────────────
// Song-mode / chain-override transition coverage.  Split out of
// state_tests.rs to keep both files under the 1000-line hook limit.

#[cfg(test)]
mod song_mode_tests {
    use crate::state::{
        AppState, ChainSlotOverride, chain_push, clear_chain_slot_override, set_chain_slot_bpm,
        set_chain_slot_repeats, set_chain_slot_style, set_song,
    };

    #[test]
    fn set_chain_slot_style_pads_overrides_and_writes() {
        // Starting with chain=[0,1,2] and no overrides, writing to slot 2
        // should grow the overrides vec to 3 with defaults in positions 0/1.
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = chain_push(s, 1);
        let s = chain_push(s, 2);
        let s = set_chain_slot_style(s, 2, Some("acid_classic".to_string()));
        assert_eq!(s.chain_overrides.len(), 3);
        assert_eq!(s.chain_overrides[0].style, None);
        assert_eq!(s.chain_overrides[1].style, None);
        assert_eq!(s.chain_overrides[2].style, Some("acid_classic".to_string()));
        // Repeats default stays 1 on all slots.
        for o in &s.chain_overrides {
            assert_eq!(o.repeats, 1);
        }
    }

    #[test]
    fn set_chain_slot_repeats_clamps_to_1_to_64() {
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = set_chain_slot_repeats(s, 0, 0); // too low
        assert_eq!(s.chain_overrides[0].repeats, 1);
        let s = set_chain_slot_repeats(s, 0, 99); // too high
        assert_eq!(s.chain_overrides[0].repeats, 64);
        let s = set_chain_slot_repeats(s, 0, 8); // legit
        assert_eq!(s.chain_overrides[0].repeats, 8);
    }

    #[test]
    fn set_chain_slot_bpm_clamps_to_valid_range() {
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = set_chain_slot_bpm(s, 0, Some(1.0));
        assert_eq!(s.chain_overrides[0].bpm, Some(crate::state::BPM_MIN));
        let s = set_chain_slot_bpm(s, 0, Some(9999.0));
        assert_eq!(s.chain_overrides[0].bpm, Some(crate::state::BPM_MAX));
        let s = set_chain_slot_bpm(s, 0, None);
        assert_eq!(s.chain_overrides[0].bpm, None);
    }

    #[test]
    fn set_chain_slot_is_noop_beyond_chain_length() {
        // Writing to a slot past the chain's end must not grow
        // `chain_overrides` — keeps the override vec aligned with chain.
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = set_chain_slot_style(s, 5, Some("acid".to_string()));
        assert_eq!(s.chain_overrides.len(), 0);
    }

    #[test]
    fn clear_chain_slot_override_resets_to_default() {
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = set_chain_slot_repeats(s, 0, 4);
        let s = set_chain_slot_style(s, 0, Some("deep_house".into()));
        let s = clear_chain_slot_override(s, 0);
        assert_eq!(s.chain_overrides[0].repeats, 1);
        assert_eq!(s.chain_overrides[0].style, None);
        assert_eq!(s.chain_overrides[0].bpm, None);
    }

    #[test]
    fn set_song_replaces_everything_atomically() {
        // Initial state with a chain + some overrides; set_song should
        // overwrite cleanly and reset chain_pos / chain_repeat_count.
        let s = AppState {
            chain_pos: 3,
            chain_repeat_count: 2,
            ..AppState::default()
        };
        let s = chain_push(s, 0);
        let s = chain_push(s, 1);
        let s = set_chain_slot_repeats(s, 1, 4);
        let overrides = vec![
            ChainSlotOverride {
                bpm: Some(160.0),
                style: Some("jungle".into()),
                repeats: 2,
                morph_bars: 0,
            },
            ChainSlotOverride::default(),
        ];
        let s = set_song(s, vec![3, 4], overrides, false);
        assert_eq!(s.chain, vec![3, 4]);
        assert_eq!(s.chain_overrides.len(), 2);
        assert_eq!(s.chain_overrides[0].style, Some("jungle".into()));
        assert_eq!(s.chain_overrides[0].repeats, 2);
        assert!(!s.chain_enabled);
        assert_eq!(s.chain_pos, 0); // reset when enabled=false
        assert_eq!(s.chain_repeat_count, 0);
    }

    #[test]
    fn swap_chain_slots_swaps_chain_and_overrides_together() {
        use crate::state::swap_chain_slots;
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = chain_push(s, 3);
        let s = chain_push(s, 5);
        let s = set_chain_slot_style(s, 0, Some("acid_classic".into()));
        let s = set_chain_slot_repeats(s, 2, 4);
        // Swap positions 0 and 2 — chain and overrides move as a unit.
        let s = swap_chain_slots(s, 0, 2);
        assert_eq!(s.chain, vec![5, 3, 0]);
        // Slot 0 now holds what was at slot 2 → repeats=4.
        assert_eq!(s.chain_overrides[0].repeats, 4);
        // Slot 2 now holds what was at slot 0 → style=acid_classic.
        assert_eq!(s.chain_overrides[2].style, Some("acid_classic".to_string()));
    }

    #[test]
    fn swap_chain_slots_out_of_bounds_is_noop() {
        use crate::state::swap_chain_slots;
        let s = AppState::default();
        let s = chain_push(s, 0);
        let s = chain_push(s, 3);
        let orig_chain = s.chain.clone();
        let s = swap_chain_slots(s, 0, 99);
        assert_eq!(s.chain, orig_chain);
        let s = swap_chain_slots(s, 5, 0);
        assert_eq!(s.chain, orig_chain);
        let s = swap_chain_slots(s, 1, 1);
        assert_eq!(s.chain, orig_chain);
    }

    #[test]
    fn chain_slot_override_is_empty_helper() {
        let mut o = ChainSlotOverride::default();
        assert!(o.is_empty(), "pure default is empty");
        o.repeats = 1;
        assert!(o.is_empty(), "repeats=1 still empty (= default)");
        o.repeats = 2;
        assert!(!o.is_empty());
        o.repeats = 1;
        o.style = Some("x".into());
        assert!(!o.is_empty());
    }
}
