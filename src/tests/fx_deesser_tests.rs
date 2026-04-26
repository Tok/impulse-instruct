// ─── tests/fx_deesser_tests.rs ───────────────────────────────────────────────
// State-level tests for the de-esser FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, amount=0 transparency, sibilant
// ducking, low-freq passthrough, boundedness) live in
// `audio/dsp/fx_deesser.rs`.

#[cfg(test)]
mod deesser_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_centre_a_typical_vocal_de_essing_setup() {
        let fx = FxState::default();
        // Sibilant centre ≈ 6 kHz (knob 0.5 → 3 kHz × 4^0.5 = 6 kHz).
        assert!((fx.deess_freq - 0.5).abs() < 1e-5);
        assert!((fx.deess_threshold - 0.5).abs() < 1e-5);
        // Audible amount on first engagement.
        assert!(fx.deess_amount > 0.5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.deess_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "deess_freq": 0.7,
                "deess_threshold": 0.2,
                "deess_amount": 1.0,
                "deess_mix": 0.85,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.deess_freq - 0.7).abs() < 1e-5);
        assert!((s1.fx.deess_threshold - 0.2).abs() < 1e-5);
        assert!((s1.fx.deess_amount - 1.0).abs() < 1e-5);
        assert!((s1.fx.deess_mix - 0.85).abs() < 1e-5);
    }

    #[test]
    fn locked_deess_amount_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "deess_amount": 0.0 } });
        let locked = ["fx.deess_amount".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default amount is 0.7; locked apply must keep it.
        assert!((s1.fx.deess_amount - 0.7).abs() < 1e-5);
    }
}

#[cfg(test)]
mod deesser_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxdeesser_maps_to_deesser_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxDeEsser),
            Some(FxStep::DeEsser),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_de_esser() {
        assert_eq!(ModuleKind::FxDeEsser.label(), "DE-ESSER");
    }

    #[test]
    fn parses_from_deesser_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in ["deesser", "deess", "fxdeesser", "sibilance", "sibilant"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxDeEsser),
                "alias `{alias}` should parse"
            );
        }
    }
}
