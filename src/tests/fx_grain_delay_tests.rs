// ─── tests/fx_grain_delay_tests.rs ───────────────────────────────────────────
// State-level tests for the grain-delay FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, audible output, bounded,
// scatter=0 deterministic) live in `audio/dsp/fx_grain_delay.rs`.

#[cfg(test)]
mod grain_delay_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_audible_chorus_cloud_with_mix_zero() {
        let fx = FxState::default();
        // ~200 ms baseline (knob 0.4 → 50 * 20^0.4 ≈ 167 ms).
        assert!((fx.grain_delay - 0.4).abs() < 1e-5);
        // ~90 ms grain length (knob 0.4 → 20 + 180*0.4 = 92 ms).
        assert!((fx.grain_size - 0.4).abs() < 1e-5);
        // Audible jitter on first engagement.
        assert!((fx.grain_scatter - 0.4).abs() < 1e-5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.grain_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "grain_delay": 0.7,
                "grain_size": 0.85,
                "grain_scatter": 1.0,
                "grain_mix": 0.5,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.grain_delay - 0.7).abs() < 1e-5);
        assert!((s1.fx.grain_size - 0.85).abs() < 1e-5);
        assert!((s1.fx.grain_scatter - 1.0).abs() < 1e-5);
        assert!((s1.fx.grain_mix - 0.5).abs() < 1e-5);
    }

    #[test]
    fn locked_grain_scatter_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "grain_scatter": 0.0 } });
        let locked = ["fx.grain_scatter".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default scatter is 0.4; locked apply must keep it.
        assert!((s1.fx.grain_scatter - 0.4).abs() < 1e-5);
    }
}

#[cfg(test)]
mod grain_delay_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxgraindelay_maps_to_graindelay_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxGrainDelay),
            Some(FxStep::GrainDelay),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_grain_del() {
        assert_eq!(ModuleKind::FxGrainDelay.label(), "GRAIN DEL");
    }

    #[test]
    fn parses_from_grain_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "graindelay",
            "grain_delay",
            "fxgraindelay",
            "granulardelay",
            "graincloud",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxGrainDelay),
                "alias `{alias}` should parse"
            );
        }
    }
}
