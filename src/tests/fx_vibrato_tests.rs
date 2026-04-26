// ─── tests/fx_vibrato_tests.rs ───────────────────────────────────────────────
// State-level tests for the Vibrato FX — defaults, apply_llm_update
// path, lock honouring, and the ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, depth=0 transparency, audible
// pitch modulation, boundedness) live in `audio/dsp/fx_vibrato.rs`.

#[cfg(test)]
mod vibrato_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_natural_vibrato_neutral() {
        let fx = FxState::default();
        // Rate ≈ 5 Hz (natural-sounding vocal / string vibrato)
        // and a moderate depth (~25 cents peak swing) so engaging
        // the FX is musically obvious without sounding seasick.
        assert!((fx.vibrato_rate - 0.45).abs() < 1e-5);
        assert!((fx.vibrato_depth - 0.5).abs() < 1e-5);
        assert_eq!(fx.vibrato_shape, 0.0);
        assert_eq!(fx.vibrato_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "vibrato_rate": 0.7,
                "vibrato_depth": 0.85,
                "vibrato_shape": 1.0,
                "vibrato_mix": 0.5,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.vibrato_rate - 0.7).abs() < 1e-5);
        assert!((s1.fx.vibrato_depth - 0.85).abs() < 1e-5);
        assert!((s1.fx.vibrato_shape - 1.0).abs() < 1e-5);
        assert!((s1.fx.vibrato_mix - 0.5).abs() < 1e-5);
    }

    #[test]
    fn locked_vibrato_mix_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "vibrato_mix": 0.9 } });
        let locked = ["fx.vibrato_mix".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        assert_eq!(s1.fx.vibrato_mix, 0.0);
    }
}

#[cfg(test)]
mod vibrato_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxvibrato_maps_to_vibrato_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxVibrato),
            Some(FxStep::Vibrato),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_vibrato() {
        assert_eq!(ModuleKind::FxVibrato.label(), "VIBRATO");
    }

    #[test]
    fn parses_from_vibrato_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in ["vibrato", "vib", "fxvibrato", "pitchmod", "pitchwobble"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxVibrato),
                "alias `{alias}` should parse"
            );
        }
    }
}
