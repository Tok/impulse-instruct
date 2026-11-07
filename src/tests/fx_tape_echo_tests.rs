// ─── tests/fx_tape_echo_tests.rs ─────────────────────────────────────────────
// State-level tests for the tape-echo FX — defaults, apply,
// lock honouring, ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (mix=0 bypass, audible repeats from impulse,
// boundedness, age=0 = clean digital) live in
// `audio/dsp/fx_tape_echo.rs`.

#[cfg(test)]
mod tape_echo_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_dub_slap_with_mix_zero() {
        let fx = FxState::default();
        // Time knob 0.4 ≈ 250 ms — classic dub slap-back.
        assert!((fx.tape_echo_time - 0.4).abs() < 1e-5);
        // Feedback at 0.4 → ~3-4 audible repeats.
        assert!((fx.tape_echo_feedback - 0.4).abs() < 1e-5);
        // Age at 0.5 → audible character on first engagement.
        assert!((fx.tape_echo_age - 0.5).abs() < 1e-5);
        // Mix at 0 = bypass on insert.
        assert_eq!(fx.tape_echo_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_all_four_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "tape_echo_time": 0.7,
                "tape_echo_feedback": 0.85,
                "tape_echo_age": 1.0,
                "tape_echo_mix": 0.5,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.tape_echo_time - 0.7).abs() < 1e-5);
        assert!((s1.fx.tape_echo_feedback - 0.85).abs() < 1e-5);
        assert!((s1.fx.tape_echo_age - 1.0).abs() < 1e-5);
        assert!((s1.fx.tape_echo_mix - 0.5).abs() < 1e-5);
    }

    #[test]
    fn locked_tape_echo_age_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "tape_echo_age": 0.0 } });
        let locked = ["fx.tape_echo_age".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Default age is 0.5; locked apply must keep it.
        assert!((s1.fx.tape_echo_age - 0.5).abs() < 1e-5);
    }
}

#[cfg(test)]
mod tape_echo_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxtapeecho_maps_to_tapeecho_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxTapeEcho),
            Some(FxStep::TapeEcho),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_tape_echo() {
        assert_eq!(ModuleKind::FxTapeEcho.label(), "TAPE ECHO");
    }

    #[test]
    fn parses_from_tape_echo_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "tapeecho",
            "tape_echo",
            "fxtapeecho",
            "dubecho",
            "spaceecho",
            "echotape",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxTapeEcho),
                "alias `{alias}` should parse"
            );
        }
    }
}
