// ─── tests/fx_plate_tests.rs ─────────────────────────────────────────────────
// State-level tests for the plate reverb FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, audible tail from impulse,
// boundedness, larger size = longer tail) live in
// `audio/dsp/fx_plate.rs`.

#[cfg(test)]
mod plate_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_medium_plate_with_mix_zero() {
        let fx = FxState::default();
        // Size 0.55 → medium plate.
        assert!((fx.plate_size - 0.55).abs() < 1e-5);
        // Damping 0.4 → modern plate (not too dark).
        assert!((fx.plate_damping - 0.4).abs() < 1e-5);
        // Diffusion 0.7 → dense input network.
        assert!((fx.plate_diffusion - 0.7).abs() < 1e-5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.plate_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "plate_size": 0.9,
                "plate_damping": 0.2,
                "plate_diffusion": 1.0,
                "plate_mix": 0.5,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.plate_size - 0.9).abs() < 1e-5);
        assert!((s1.fx.plate_damping - 0.2).abs() < 1e-5);
        assert!((s1.fx.plate_diffusion - 1.0).abs() < 1e-5);
        assert!((s1.fx.plate_mix - 0.5).abs() < 1e-5);
    }

    #[test]
    fn locked_plate_size_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "plate_size": 0.0 } });
        let locked = ["fx.plate_size".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default size is 0.55; locked apply must keep it.
        assert!((s1.fx.plate_size - 0.55).abs() < 1e-5);
    }
}

#[cfg(test)]
mod plate_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxplate_maps_to_plate_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxPlate),
            Some(FxStep::Plate),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_plate() {
        assert_eq!(ModuleKind::FxPlate.label(), "PLATE");
    }

    #[test]
    fn parses_from_plate_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "plate",
            "fxplate",
            "platereverb",
            "plate_reverb",
            "emt",
            "lexicon",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxPlate),
                "alias `{alias}` should parse"
            );
        }
    }
}
