// ─── tests/crossfader_tests.rs ───────────────────────────────────────────────
// State-side tests for the Crossfader CV utility.

#[cfg(test)]
mod crossfader_state_tests {
    use crate::state::{AppState, CROSSFADER_SLOTS, CrossfaderSlot};

    #[test]
    fn defaults_disabled_with_centered_mix() {
        let s = CrossfaderSlot::default();
        assert!(!s.enabled);
        assert!((s.mix - 0.5).abs() < 1e-5);
    }

    #[test]
    fn slot_array_round_trips_through_app_state() {
        let mut s = AppState::default();
        assert_eq!(s.crossfader.len(), CROSSFADER_SLOTS);
        s.crossfader[3].enabled = true;
        s.crossfader[3].mix = 0.85;
        assert!(s.crossfader[3].enabled);
        assert!((s.crossfader[3].mix - 0.85).abs() < 1e-5);
    }
}

#[cfg(test)]
mod crossfader_module_tests {
    use crate::state::ModuleKind;

    #[test]
    fn label_is_xfade() {
        assert_eq!(ModuleKind::Crossfader.label(), "XFADE");
    }

    #[test]
    fn parses_from_crossfader_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in ["crossfader", "xfade", "xfader", "ablend", "abblend", "ab"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::Crossfader),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn lives_in_fxmod_zone() {
        assert_eq!(
            ModuleKind::Crossfader.default_zone(),
            crate::state::Zone::FxMod
        );
    }

    #[test]
    fn allows_multiple() {
        assert!(ModuleKind::Crossfader.allows_multiple());
    }
}
