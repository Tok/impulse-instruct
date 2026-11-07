// ─── tests/fx_spectral_gate_tests.rs ─────────────────────────────────────────
// State-level tests for the spectral-gate FX — defaults,
// apply, lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, threshold=0 transparency,
// high-threshold silences low-level signal, loud signal passes
// when threshold is below, output bounded) live in
// `audio/dsp/fx_spectral_gate.rs`.

#[cfg(test)]
mod spectral_gate_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_transparent_with_mix_zero() {
        let fx = FxState::default();
        // Threshold at 0 keeps every band open; freshly-inserted
        // FX is transparent until the user dials threshold up.
        assert_eq!(fx.spec_thresh, 0.0);
        assert!((fx.spec_release - 0.4).abs() < 1e-5);
        // Tilt at 0.5 = uniform across bands.
        assert!((fx.spec_tilt - 0.5).abs() < 1e-5);
        assert_eq!(fx.spec_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "spec_thresh": 0.6,
                "spec_release": 0.8,
                "spec_tilt": 0.2,
                "spec_mix": 0.85,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.spec_thresh - 0.6).abs() < 1e-5);
        assert!((s1.fx.spec_release - 0.8).abs() < 1e-5);
        assert!((s1.fx.spec_tilt - 0.2).abs() < 1e-5);
        assert!((s1.fx.spec_mix - 0.85).abs() < 1e-5);
    }

    #[test]
    fn locked_spec_thresh_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "spec_thresh": 0.9 } });
        let locked = ["fx.spec_thresh".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default threshold is 0.0; locked apply must keep it.
        assert_eq!(s1.fx.spec_thresh, 0.0);
    }
}

#[cfg(test)]
mod spectral_gate_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxspectralgate_maps_to_spectralgate_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxSpectralGate),
            Some(FxStep::SpectralGate),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_spec_gate() {
        assert_eq!(ModuleKind::FxSpectralGate.label(), "SPEC GATE");
    }

    #[test]
    fn parses_from_spectral_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "spectralgate",
            "spectral_gate",
            "specgate",
            "fxspectralgate",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxSpectralGate),
                "alias `{alias}` should parse"
            );
        }
    }
}
