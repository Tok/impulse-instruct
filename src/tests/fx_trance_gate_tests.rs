// ─── tests/fx_trance_gate_tests.rs ───────────────────────────────────────────
// State-level tests for the trance-gate FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, all-on transparency, all-off
// silences, alternating pattern visits both states, output
// boundedness) live in `audio/dsp/fx_trance_gate.rs`.

#[cfg(test)]
mod trance_gate_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_alternating_pattern_with_mix_zero() {
        let fx = FxState::default();
        // 0xAAAA = alternating odd cells active.
        assert_eq!(fx.tg_pattern, 0xAAAA);
        // Default rate 1 = 1/8 (eighth-note cells).
        assert_eq!(fx.tg_rate, 1);
        // Default smooth 0.2 = ~10 ms cell-edge ramp.
        assert!((fx.tg_smooth - 0.2).abs() < 1e-5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.tg_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "tg_pattern": 0xCCCC,
                "tg_rate": 2,
                "tg_smooth": 0.5,
                "tg_mix": 0.7,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert_eq!(s1.fx.tg_pattern, 0xCCCC);
        assert_eq!(s1.fx.tg_rate, 2);
        assert!((s1.fx.tg_smooth - 0.5).abs() < 1e-5);
        assert!((s1.fx.tg_mix - 0.7).abs() < 1e-5);
    }

    #[test]
    fn llm_apply_clamps_rate_above_max() {
        // tg_rate is 0..=3; values above must clamp to 3 (1/32) so
        // an LLM that writes a number out of range doesn't index past
        // the rate table on the audio thread.
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "tg_rate": 99 } });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert_eq!(s1.fx.tg_rate, 3);
    }

    #[test]
    fn locked_tg_pattern_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "tg_pattern": 0xFFFF } });
        let locked = ["fx.tg_pattern".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default pattern is 0xAAAA; locked apply must keep it.
        assert_eq!(s1.fx.tg_pattern, 0xAAAA);
    }
}

#[cfg(test)]
mod trance_gate_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxtrancegate_maps_to_trancegate_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxTranceGate),
            Some(FxStep::TranceGate),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_trance_gate() {
        assert_eq!(ModuleKind::FxTranceGate.label(), "TRANCE GATE");
    }

    #[test]
    fn parses_from_trance_gate_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "trancegate",
            "trance_gate",
            "fxtrancegate",
            "trance",
            "patterngate",
            "stepgate",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxTranceGate),
                "alias `{alias}` should parse"
            );
        }
    }
}
