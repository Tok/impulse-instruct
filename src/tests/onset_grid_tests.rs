// ─── tests/onset_grid_tests.rs ───────────────────────────────────────────────
// State-level tests for the onset / beat-grid overlay viz —
// ModuleKind metadata + alias parsing.  DSP-side tests
// (envelope tracking, peak picking, empty-buffer guard) live
// next to the panel in `ui/panels/onset_grid.rs` since the
// helpers are intentionally private to that module.

#[cfg(test)]
mod onset_grid_module_tests {
    use crate::state::{ModuleKind, Zone, rack_scope::parse_module_kind};

    #[test]
    fn label_is_onset_grid() {
        assert_eq!(ModuleKind::OnsetGrid.label(), "ONSET GRID");
    }

    #[test]
    fn lives_in_fxmod_zone_with_no_audio_io() {
        let k = ModuleKind::OnsetGrid;
        // Visualization-only — no audio output, no audio input.
        assert!(!k.has_audio_output());
        assert_eq!(k.default_zone(), Zone::FxMod);
    }

    #[test]
    fn parses_from_aliases() {
        for alias in [
            "onsetgrid",
            "onset_grid",
            "onsetoverlay",
            "groove",
            "groovecheck",
            "onsets",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::OnsetGrid),
                "alias `{alias}` should parse"
            );
        }
    }
}
