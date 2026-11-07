// ─── tests/pattern_heatmap_tests.rs ──────────────────────────────────────────
// State-level tests for the pattern density heatmap viz —
// ModuleKind metadata + alias parsing.  The panel itself is pure
// UI; rendering paths are exercised through the existing
// rack-canvas test harness, but the metadata wiring needs
// dedicated coverage so renames don't silently desync.

#[cfg(test)]
mod pattern_heatmap_module_tests {
    use crate::state::{ModuleKind, Zone, rack_scope::parse_module_kind};

    #[test]
    fn label_is_pattern_map() {
        assert_eq!(ModuleKind::PatternHeatmap.label(), "PATTERN MAP");
    }

    #[test]
    fn lives_in_fxmod_zone_with_no_audio_io() {
        let k = ModuleKind::PatternHeatmap;
        // Visualization-only — no audio output, no audio input.
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
    }

    #[test]
    fn parses_from_aliases() {
        for alias in [
            "patternheatmap",
            "patternmap",
            "heatmap",
            "patterndensity",
            "patterns",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::PatternHeatmap),
                "alias `{alias}` should parse"
            );
        }
    }
}
