// ─── tests/fx_mb_comp_tests.rs ───────────────────────────────────────────────
// State-level tests for the multiband compressor — defaults,
// apply, lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, unity-thresholds passthrough,
// bass band compresses without ducking the high band, low band
// gets ducked when threshold is dialed down, output bounded)
// live in `audio/dsp/fx_mb_comp.rs`.

#[cfg(test)]
mod mb_comp_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_uncompressed_passthrough() {
        let fx = FxState::default();
        // Every threshold at unity → no compression engages until
        // the user dials a band down.  Mix at 0 = bypass on
        // insert.
        assert_eq!(fx.mb_low_thresh, 1.0);
        assert_eq!(fx.mb_mid_thresh, 1.0);
        assert_eq!(fx.mb_high_thresh, 1.0);
        assert_eq!(fx.mb_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "mb_low_thresh": 0.2,
                "mb_mid_thresh": 0.5,
                "mb_high_thresh": 0.8,
                "mb_mix": 1.0,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.mb_low_thresh - 0.2).abs() < 1e-5);
        assert!((s1.fx.mb_mid_thresh - 0.5).abs() < 1e-5);
        assert!((s1.fx.mb_high_thresh - 0.8).abs() < 1e-5);
        assert_eq!(s1.fx.mb_mix, 1.0);
    }

    #[test]
    fn locked_mb_low_thresh_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "mb_low_thresh": 0.0 } });
        let locked = ["fx.mb_low_thresh".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default is 1.0; locked apply must keep it.
        assert_eq!(s1.fx.mb_low_thresh, 1.0);
    }
}

#[cfg(test)]
mod mb_comp_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxmultibandcomp_maps_to_multibandcomp_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxMultibandComp),
            Some(FxStep::MultibandComp),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_mb_comp() {
        assert_eq!(ModuleKind::FxMultibandComp.label(), "MB COMP");
    }

    #[test]
    fn parses_from_multiband_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "multiband",
            "mbcomp",
            "mb_comp",
            "mastercomp",
            "mastercompressor",
            "fxmultibandcomp",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxMultibandComp),
                "alias `{alias}` should parse"
            );
        }
    }
}
