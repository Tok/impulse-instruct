// ─── tests/logic_gate_tests.rs ───────────────────────────────────────────────
// State-side tests for the LogicGate CV utility.

#[cfg(test)]
mod logic_gate_state_tests {
    use crate::state::{AppState, LOGIC_GATE_SLOTS, LogicGateSlot, LogicOp};

    #[test]
    fn defaults_disabled_with_and_op() {
        let s = LogicGateSlot::default();
        assert!(!s.enabled);
        assert_eq!(s.op, LogicOp::And);
    }

    #[test]
    fn slot_array_round_trips_through_app_state() {
        let mut s = AppState::default();
        assert_eq!(s.logic_gate.len(), LOGIC_GATE_SLOTS);
        s.logic_gate[2].enabled = true;
        s.logic_gate[2].op = LogicOp::Xor;
        assert!(s.logic_gate[2].enabled);
        assert_eq!(s.logic_gate[2].op, LogicOp::Xor);
    }

    #[test]
    fn op_next_cycles_three_states() {
        // AND → OR → XOR → AND.
        assert_eq!(LogicOp::And.next(), LogicOp::Or);
        assert_eq!(LogicOp::Or.next(), LogicOp::Xor);
        assert_eq!(LogicOp::Xor.next(), LogicOp::And);
    }

    #[test]
    fn op_names_are_uppercase_letters() {
        assert_eq!(LogicOp::And.name(), "AND");
        assert_eq!(LogicOp::Or.name(), "OR");
        assert_eq!(LogicOp::Xor.name(), "XOR");
    }
}

#[cfg(test)]
mod logic_gate_module_tests {
    use crate::state::ModuleKind;

    #[test]
    fn label_is_logic() {
        assert_eq!(ModuleKind::LogicGate.label(), "LOGIC");
    }

    #[test]
    fn parses_from_logic_gate_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in ["logicgate", "logic_gate", "logic", "boolean", "andorxor"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::LogicGate),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn lives_in_fxmod_zone() {
        assert_eq!(
            ModuleKind::LogicGate.default_zone(),
            crate::state::Zone::FxMod
        );
    }

    #[test]
    fn allows_multiple() {
        // Multiple gates is the whole point — chain AND with XOR for
        // composite rhythmic logic.
        assert!(ModuleKind::LogicGate.allows_multiple());
    }
}
