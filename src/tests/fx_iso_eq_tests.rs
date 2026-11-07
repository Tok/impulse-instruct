// ─── tests/fx_iso_eq_tests.rs ────────────────────────────────────────────────
// State-level tests for the 3-band ISO / kill EQ — defaults,
// apply_llm_update path, lock honouring, ModuleKind ↔ FxStep.
//
// DSP-level tests (passthrough at unity, kill behaviour at 50 Hz
// + 8 kHz, boundedness) live in `audio/dsp/fx_iso_eq.rs`.

#[cfg(test)]
mod iso_eq_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_pass_all_bands_with_mix_zero() {
        let fx = FxState::default();
        // Every band at unity so engaging the FX (mix > 0) is a
        // no-op until the user actively kills a band.
        assert_eq!(fx.iso_low, 1.0);
        assert_eq!(fx.iso_mid, 1.0);
        assert_eq!(fx.iso_high, 1.0);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.iso_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "iso_low": 0.0,
                "iso_mid": 0.5,
                "iso_high": 0.0,
                "iso_mix": 1.0,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert_eq!(s1.fx.iso_low, 0.0);
        assert!((s1.fx.iso_mid - 0.5).abs() < 1e-5);
        assert_eq!(s1.fx.iso_high, 0.0);
        assert_eq!(s1.fx.iso_mix, 1.0);
    }

    #[test]
    fn locked_iso_low_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "iso_low": 0.0 } });
        let locked = ["fx.iso_low".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default low is 1.0 — locked apply must keep it.
        assert_eq!(s1.fx.iso_low, 1.0);
    }
}

#[cfg(test)]
mod iso_eq_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxisoeq_maps_to_isoeq_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxIsoEq),
            Some(FxStep::IsoEq),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_iso_eq() {
        assert_eq!(ModuleKind::FxIsoEq.label(), "ISO EQ");
    }

    #[test]
    fn parses_from_iso_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "iso", "isoeq", "iso_eq", "killeq", "kill", "3band", "fxisoeq",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxIsoEq),
                "alias `{alias}` should parse"
            );
        }
    }
}
