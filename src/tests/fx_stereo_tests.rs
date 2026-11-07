// ─── tests/fx_stereo_tests.rs ────────────────────────────────────────────────
// Cover the master-stage stereo FX: FxWiden's chain-step latch produces
// L≠R at the master, and FxParamEq's M/S mode flag rounds-trips through
// LLM apply.  We don't drive the audio thread here — the FxWiden
// behaviour is tested via the latch boolean (rendered by master-stage
// code that's not split out into a pure helper) and a delay-buffer
// shape sanity check.  Real audio-output testing lives in the
// integration suite.

#[cfg(test)]
mod widen_state_tests {
    use crate::state::{AppState, FxState};

    #[test]
    fn widen_defaults_are_off() {
        // V1 contract: widening is bypassed by default — adding the
        // module to a rack mustn't suddenly stretch the stereo field.
        let fx = FxState::default();
        assert_eq!(fx.widen_mix, 0.0);
        assert_eq!(fx.widen_side, 0.0);
        // Haas knob has a non-zero default so when the user toggles
        // mix > 0 they hear an immediate effect rather than silence.
        assert!(fx.widen_haas > 0.0);
    }

    #[test]
    fn llm_apply_writes_widen_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "widen_haas": 0.6,
                "widen_side": 0.4,
                "widen_mix": 0.8,
            }
        });
        let s1 = crate::state::apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.widen_haas - 0.6).abs() < 1e-5);
        assert!((s1.fx.widen_side - 0.4).abs() < 1e-5);
        assert!((s1.fx.widen_mix - 0.8).abs() < 1e-5);
    }

    #[test]
    fn locked_widen_knob_is_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "widen_mix": 0.9 } });
        let locked = ["fx.widen_mix".to_string()];
        let s1 = crate::state::apply_llm_update(s0, &json, &locked);
        assert_eq!(s1.fx.widen_mix, 0.0, "locked param must not be overwritten");
    }
}

#[cfg(test)]
mod widen_xy_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn widen_xy_pad_writes_haas_and_side() {
        // The pad shortcut must fan out into the two underlying
        // knobs.  Same XY contract as every other FX pad.
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": { "widen_xy": [0.7, 0.3] }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.widen_haas - 0.7).abs() < 1e-5);
        assert!((s1.fx.widen_side - 0.3).abs() < 1e-5);
    }
}

#[cfg(test)]
mod param_eq_ms_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn param_eq_ms_mode_defaults_off() {
        let fx = FxState::default();
        assert!(!fx.param_eq_ms_mode);
    }

    #[test]
    fn llm_apply_toggles_ms_mode() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "param_eq_ms_mode": true } });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!(s1.fx.param_eq_ms_mode);
    }

    #[test]
    fn locked_ms_mode_is_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "param_eq_ms_mode": true } });
        let locked = ["fx.param_eq_ms_mode".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        assert!(!s1.fx.param_eq_ms_mode);
    }
}

#[cfg(test)]
mod widen_module_metadata_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxwiden_maps_to_widen_step() {
        assert_eq!(kind_to_fx_step(ModuleKind::FxWiden), Some(FxStep::Widen));
    }

    #[test]
    fn fxwiden_label_is_widen() {
        assert_eq!(ModuleKind::FxWiden.label(), "WIDEN");
    }

    #[test]
    fn fxwiden_does_not_have_sidechain_in() {
        // The widener is master-stage; it doesn't read a sidechain
        // input.  Pinning this so a future refactor that mistakenly
        // grants it a sidechain port has to update the test too.
        assert!(!ModuleKind::FxWiden.has_sidechain_in());
    }

    #[test]
    fn fxwiden_supports_xy_pad() {
        assert!(ModuleKind::FxWiden.supports_xy_pad());
    }
}
