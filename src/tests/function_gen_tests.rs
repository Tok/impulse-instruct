// ─── tests/function_gen_tests.rs ─────────────────────────────────────────────
// State-side tests for the FunctionGen CV utility.

#[cfg(test)]
mod function_gen_state_tests {
    use crate::state::{AppState, FUNCTION_GEN_SLOTS, FunctionGenSlot};

    #[test]
    fn defaults_disabled_with_pluck_attack() {
        let s = FunctionGenSlot::default();
        assert!(!s.enabled);
        // ~100 ms attack — pluck-style.
        assert!((s.attack - 0.1).abs() < 1e-5);
        // ~1.2 s release.
        assert!((s.release - 0.4).abs() < 1e-5);
        // Linear curve.
        assert!((s.curve - 0.5).abs() < 1e-5);
    }

    #[test]
    fn slot_array_round_trips_through_app_state() {
        let mut s = AppState::default();
        assert_eq!(s.function_gen.len(), FUNCTION_GEN_SLOTS);
        s.function_gen[1].enabled = true;
        s.function_gen[1].attack = 0.05;
        s.function_gen[1].release = 0.9;
        s.function_gen[1].curve = 0.8;
        assert!(s.function_gen[1].enabled);
        assert!((s.function_gen[1].attack - 0.05).abs() < 1e-5);
        assert!((s.function_gen[1].release - 0.9).abs() < 1e-5);
        assert!((s.function_gen[1].curve - 0.8).abs() < 1e-5);
    }
}

#[cfg(test)]
mod function_gen_module_tests {
    use crate::state::ModuleKind;

    #[test]
    fn label_is_func_gen() {
        assert_eq!(ModuleKind::FunctionGen.label(), "FUNC GEN");
    }

    #[test]
    fn parses_from_function_gen_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "functiongen",
            "function_gen",
            "funcgen",
            "ar",
            "ad",
            "envelope",
            "maths",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FunctionGen),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn lives_in_fxmod_zone() {
        assert_eq!(
            ModuleKind::FunctionGen.default_zone(),
            crate::state::Zone::FxMod
        );
    }

    #[test]
    fn allows_multiple_for_layered_envelopes() {
        // Multiple function generators is the whole point — chain
        // separate envelopes for amp + filter, or AND two gates
        // through LogicGate then trigger one envelope from each.
        assert!(ModuleKind::FunctionGen.allows_multiple());
    }
}
