// ─── tests/fx_wavefolder_tests.rs ────────────────────────────────────────────
// State-level tests for the wavefolder FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, output bounded by threshold,
// drive sweep produces wide range, low-drive passthrough,
// bias offset changes output) live in
// `audio/dsp/fx_wavefolder.rs`.

#[cfg(test)]
mod wavefolder_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_engaged_with_mix_zero() {
        let fx = FxState::default();
        // Drive 0.4 → ~4× input gain on insert, immediate fold once
        // user dials mix > 0.
        assert!((fx.wf_drive - 0.4).abs() < 1e-5);
        // Bias 0.5 = symmetric (centred).
        assert!((fx.wf_bias - 0.5).abs() < 1e-5);
        // Symmetry 0.5 = blend of sine and triangle fold curves.
        assert!((fx.wf_symmetry - 0.5).abs() < 1e-5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.wf_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "wf_drive": 0.9,
                "wf_bias": 0.2,
                "wf_symmetry": 1.0,
                "wf_mix": 0.7,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.wf_drive - 0.9).abs() < 1e-5);
        assert!((s1.fx.wf_bias - 0.2).abs() < 1e-5);
        assert!((s1.fx.wf_symmetry - 1.0).abs() < 1e-5);
        assert!((s1.fx.wf_mix - 0.7).abs() < 1e-5);
    }

    #[test]
    fn locked_wf_drive_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "wf_drive": 0.0 } });
        let locked = ["fx.wf_drive".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default drive is 0.4; locked apply must keep it.
        assert!((s1.fx.wf_drive - 0.4).abs() < 1e-5);
    }
}

#[cfg(test)]
mod wavefolder_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxwavefolder_maps_to_wavefolder_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxWaveFolder),
            Some(FxStep::WaveFolder),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_wavefolder() {
        assert_eq!(ModuleKind::FxWaveFolder.label(), "WAVEFOLDER");
    }

    #[test]
    fn parses_from_wavefolder_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "wavefolder",
            "wave_folder",
            "fxwavefolder",
            "fold",
            "westcoast",
            "buchla",
            "serge",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxWaveFolder),
                "alias `{alias}` should parse"
            );
        }
    }
}
