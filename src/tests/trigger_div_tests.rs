// ─── tests/trigger_div_tests.rs ──────────────────────────────────────────────
// State-side tests for the TriggerDiv CV utility — defaults, label /
// alias parsing, ratio snap helper, slot round-trip.  The audio-thread
// edge-detect + division pipeline is exercised indirectly when the
// rack carries a TriggerDiv slot through a process_block.

#[cfg(test)]
mod trigger_div_state_tests {
    use crate::state::{
        AppState, TRIGGER_DIV_RATIOS, TRIGGER_DIV_SLOTS, TriggerDivSlot,
        trigger_div::nearest_trigger_div_ratio,
    };

    #[test]
    fn defaults_disabled_with_ratio_two() {
        let s = TriggerDivSlot::default();
        assert!(!s.enabled);
        assert_eq!(s.ratio, 2);
    }

    #[test]
    fn slot_array_round_trips_through_app_state() {
        let mut s = AppState::default();
        assert_eq!(s.trigger_div.len(), TRIGGER_DIV_SLOTS);
        s.trigger_div[1].enabled = true;
        s.trigger_div[1].ratio = 5;
        assert!(s.trigger_div[1].enabled);
        assert_eq!(s.trigger_div[1].ratio, 5);
    }

    #[test]
    fn nearest_ratio_snaps_to_table_member() {
        // Every output of nearest_trigger_div_ratio must be a valid
        // ratio — the audio thread's `% ratio` arithmetic relies on
        // this contract.
        for r in 0u8..=12 {
            let snapped = nearest_trigger_div_ratio(r);
            assert!(
                TRIGGER_DIV_RATIOS.contains(&snapped),
                "input {r} → snapped {snapped} not in TRIGGER_DIV_RATIOS"
            );
        }
    }

    #[test]
    fn nearest_ratio_returns_exact_for_table_members() {
        for &r in TRIGGER_DIV_RATIOS.iter() {
            assert_eq!(nearest_trigger_div_ratio(r), r);
        }
    }

    #[test]
    fn nearest_ratio_picks_closest_for_off_table_input() {
        // 6 is between 5 and 7; both are equidistant.  The chosen
        // tiebreaker (whichever comes first in the table) doesn't
        // matter — what matters is the snapped value is *one of* them.
        let snapped = nearest_trigger_div_ratio(6);
        assert!(snapped == 5 || snapped == 7);
        // 8 → closest is 7.
        assert_eq!(nearest_trigger_div_ratio(8), 7);
    }
}

#[cfg(test)]
mod trigger_div_module_tests {
    use crate::state::ModuleKind;

    #[test]
    fn label_is_trig_div() {
        assert_eq!(ModuleKind::TriggerDiv.label(), "TRIG DIV");
    }

    #[test]
    fn parses_from_trigger_div_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "triggerdiv",
            "trigger_div",
            "trigdiv",
            "clockdivider",
            "clockdiv",
            "divider",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::TriggerDiv),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn lives_in_fxmod_zone() {
        assert_eq!(
            ModuleKind::TriggerDiv.default_zone(),
            crate::state::Zone::FxMod
        );
    }

    #[test]
    fn allows_multiple_for_polyrhythmic_patches() {
        // Multiple TriggerDivs is the whole point — 3-against-4
        // polyrhythms need at least two divider instances on
        // different ratios.
        assert!(ModuleKind::TriggerDiv.allows_multiple());
    }
}
