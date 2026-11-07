// ─── tests/fx_tremolo_tests.rs ───────────────────────────────────────────────
// State-level tests for the Tremolo FX — defaults, apply_llm_update
// path, lock honouring, and the ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (sine vs square shape, depth swing, boundedness)
// live in `audio/dsp/fx_tremolo.rs` so they can poke the FX's
// private state directly.

#[cfg(test)]
mod tremolo_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_engaged_neutral() {
        let fx = FxState::default();
        // Rate ~3 Hz (knob ≈ 0.4) — classic guitar-amp tremolo
        // feel; depth audible (~0.6) so the first engagement has
        // an unmistakable swell.  Shape sine; mix 0 so a freshly
        // inserted module bypasses cleanly until the user dials it.
        assert!((fx.tremolo_rate - 0.4).abs() < 1e-5);
        assert!((fx.tremolo_depth - 0.6).abs() < 1e-5);
        assert_eq!(fx.tremolo_shape, 0.0);
        assert_eq!(fx.tremolo_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "tremolo_rate": 0.7,
                "tremolo_depth": 0.85,
                "tremolo_shape": 1.0,
                "tremolo_mix": 0.5,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.tremolo_rate - 0.7).abs() < 1e-5);
        assert!((s1.fx.tremolo_depth - 0.85).abs() < 1e-5);
        assert!((s1.fx.tremolo_shape - 1.0).abs() < 1e-5);
        assert!((s1.fx.tremolo_mix - 0.5).abs() < 1e-5);
    }

    #[test]
    fn locked_tremolo_mix_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "tremolo_mix": 0.9 } });
        let locked = ["fx.tremolo_mix".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default mix is 0; locked apply must keep it.
        assert_eq!(s1.fx.tremolo_mix, 0.0);
    }
}

#[cfg(test)]
mod tremolo_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxtremolo_maps_to_tremolo_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxTremolo),
            Some(FxStep::Tremolo),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_tremolo() {
        assert_eq!(ModuleKind::FxTremolo.label(), "TREMOLO");
    }

    #[test]
    fn parses_from_tremolo_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in ["tremolo", "trem", "fxtremolo", "ampmod", "ampl_mod"] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxTremolo),
                "alias `{alias}` should parse"
            );
        }
    }
}
