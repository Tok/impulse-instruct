// ─── tests/fx_djfilter_tests.rs ──────────────────────────────────────────────
// State-level tests for the DJ filter FX — defaults, apply_llm_update
// path, lock honouring, and the ModuleKind ↔ FxStep mapping.
//
// DSP-level tests (LP/BP/HP behaviour, resonance peak emphasis, output
// boundedness) live in `audio/dsp/fx_djfilter.rs` so they can poke the
// voice's private fields directly.

#[cfg(test)]
mod dj_filter_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_centre_morph_and_zero_mix() {
        let fx = FxState::default();
        // morph 0.5 = BP at the resonance crossover — neutral
        // visual position before the user sweeps.  Mix is 0 so a
        // freshly inserted module doesn't colour the audio until
        // the user dials it in.
        assert!((fx.dj_filter_morph - 0.5).abs() < 1e-5);
        assert!((fx.dj_filter_resonance - 0.4).abs() < 1e-5);
        assert_eq!(fx.dj_filter_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_dj_filter_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "dj_filter_morph": 0.2,
                "dj_filter_resonance": 0.7,
                "dj_filter_mix": 0.85,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.dj_filter_morph - 0.2).abs() < 1e-5);
        assert!((s1.fx.dj_filter_resonance - 0.7).abs() < 1e-5);
        assert!((s1.fx.dj_filter_mix - 0.85).abs() < 1e-5);
    }

    #[test]
    fn locked_dj_filter_morph_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "dj_filter_morph": 0.9 } });
        let locked = ["fx.dj_filter_morph".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        // Locked: morph should not have changed from its default.
        assert!((s1.fx.dj_filter_morph - 0.5).abs() < 1e-5);
    }
}

#[cfg(test)]
mod dj_filter_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxdjfilter_maps_to_djfilter_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxDjFilter),
            Some(FxStep::DjFilter),
            "module → step mapping must round-trip so the rack and the audio thread agree on the chain order"
        );
    }

    #[test]
    fn label_is_dj_filter() {
        // The card title is what users read when they open the
        // rack add-module menu — guard against an accidental
        // rename that would break the LLM scope match below.
        assert_eq!(ModuleKind::FxDjFilter.label(), "DJ FILTER");
    }

    #[test]
    fn parses_from_dj_filter_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        // Each alias the rack-scope parser is supposed to accept.
        for alias in [
            "djfilter",
            "dj_filter",
            "dj filter",
            "fxdjfilter",
            "morphfilter",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::FxDjFilter),
                "alias `{alias}` should parse"
            );
        }
    }
}
