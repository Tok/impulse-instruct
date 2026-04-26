// ─── tests/gr_history_tests.rs ───────────────────────────────────────────────
// State-side tests for the GrHistory viz module.  Atomic round-trip
// + linear-to-dB conversion covered in `audio/gr_levels.rs`; the
// audio-thread snap-down + release pipeline is exercised indirectly
// by the existing FX tests.

#[cfg(test)]
mod gr_history_module_tests {
    use crate::state::ModuleKind;

    #[test]
    fn label_is_gr_history() {
        assert_eq!(ModuleKind::GrHistory.label(), "GR HISTORY");
    }

    #[test]
    fn parses_from_gr_history_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "grhistory",
            "gr_history",
            "gainreduction",
            "gain_reduction",
            "gr",
            "grscope",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::GrHistory),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn lives_in_fxmod_zone() {
        assert_eq!(
            ModuleKind::GrHistory.default_zone(),
            crate::state::Zone::FxMod
        );
    }

    #[test]
    fn has_no_audio_output() {
        assert!(!ModuleKind::GrHistory.has_audio_output());
    }

    #[test]
    fn is_singleton() {
        // Multiple history scopes would all paint the same atomic
        // snapshot — singleton keeps the rack uncluttered.
        assert!(!ModuleKind::GrHistory.allows_multiple());
    }
}
