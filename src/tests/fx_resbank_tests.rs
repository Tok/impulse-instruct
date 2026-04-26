// ─── tests/fx_resbank_tests.rs ───────────────────────────────────────────────
// State-level tests for the resonator-bank FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (impulse → ring, every chord preset rings,
// boundedness) live in `audio/dsp/fx_resbank.rs`.

#[cfg(test)]
mod resbank_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_middle_c_minor7_with_mix_zero() {
        let fx = FxState::default();
        // Root knob 0.5 → MIDI 60 (middle C).
        assert!((fx.resbank_root - 0.5).abs() < 1e-5);
        // Chord knob 0 → preset 0 (minor 7).
        assert_eq!(fx.resbank_chord, 0.0);
        assert!((fx.resbank_resonance - 0.6).abs() < 1e-5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.resbank_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "resbank_root": 0.7,
                "resbank_chord": 0.5,
                "resbank_resonance": 0.9,
                "resbank_mix": 0.4,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.resbank_root - 0.7).abs() < 1e-5);
        assert!((s1.fx.resbank_chord - 0.5).abs() < 1e-5);
        assert!((s1.fx.resbank_resonance - 0.9).abs() < 1e-5);
        assert!((s1.fx.resbank_mix - 0.4).abs() < 1e-5);
    }

    #[test]
    fn locked_resbank_root_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "resbank_root": 0.0 } });
        let locked = ["fx.resbank_root".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default root is 0.5; locked apply must keep it.
        assert!((s1.fx.resbank_root - 0.5).abs() < 1e-5);
    }
}

#[cfg(test)]
mod resbank_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxresbank_maps_to_resbank_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxResBank),
            Some(FxStep::ResBank),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_res_bank() {
        assert_eq!(ModuleKind::FxResBank.label(), "RES BANK");
    }

    #[test]
    fn parses_from_resbank_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "resbank",
            "fxresbank",
            "resonatorbank",
            "resonators",
            "chordres",
            "chordresonator",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxResBank),
                "alias `{alias}` should parse"
            );
        }
    }
}
