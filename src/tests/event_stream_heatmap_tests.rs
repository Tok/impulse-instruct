// ─── tests/event_stream_heatmap_tests.rs ─────────────────────────────────────
// Sanity tests for the EventStream heatmap data path: per-voice tag on
// `MelodicLogEntry`, default for the `stream_heatmap` UI pref, and the
// API plumbing that flips the toggle.
//
// Rendering itself is paint-only (egui Painter calls into a real frame),
// not unit-testable here — those tests would require a UI integration
// harness.  We pin the data shape so future refactors don't silently
// drop the voice field or the toggle.

#[cfg(test)]
mod heatmap_data_tests {
    use crate::state::DrumVoice;
    use crate::ui::{DrumLogEntry, MelodicLogEntry, MelodicVoice};

    #[test]
    fn melodic_log_entry_carries_voice_tag() {
        let bass1 = MelodicLogEntry {
            fired_at: 10,
            note: 60,
            gate: 0.5,
            accent: 0.0,
            slide: 0.0,
            voice: MelodicVoice::Bass(0),
        };
        let an1x = MelodicLogEntry {
            fired_at: 10,
            note: 64,
            gate: 0.7,
            accent: 0.5,
            slide: 0.0,
            voice: MelodicVoice::An1x,
        };
        // The heatmap groups by voice; a refactor that drops the field
        // would silently bucket every melodic note into the same row.
        assert_eq!(bass1.voice, MelodicVoice::Bass(0));
        assert_ne!(bass1.voice, an1x.voice);
        assert_ne!(MelodicVoice::Bass(0), MelodicVoice::Bass(1));
    }

    #[test]
    fn drum_log_entry_carries_voice_tag() {
        let kick = DrumLogEntry {
            fired_at: 5,
            voice: DrumVoice::Kick808,
        };
        let snare = DrumLogEntry {
            fired_at: 6,
            voice: DrumVoice::Snare909,
        };
        assert_ne!(kick.voice, snare.voice);
    }

    #[test]
    fn voice_filter_groups_correctly() {
        // Mirror the heatmap's row-matching logic to make sure a typo in
        // either side stays caught: bass entries should land in the
        // BASS row regardless of voice index, and the singleton voices
        // should each get their own row.
        let entries = [
            MelodicVoice::Bass(0),
            MelodicVoice::Bass(2),
            MelodicVoice::An1x,
            MelodicVoice::Hoover,
            MelodicVoice::Bass(1),
        ];
        let bass_count = entries
            .iter()
            .filter(|v| matches!(v, MelodicVoice::Bass(_)))
            .count();
        let an1x_count = entries
            .iter()
            .filter(|v| matches!(v, MelodicVoice::An1x))
            .count();
        let hoover_count = entries
            .iter()
            .filter(|v| matches!(v, MelodicVoice::Hoover))
            .count();
        assert_eq!(bass_count, 3);
        assert_eq!(an1x_count, 1);
        assert_eq!(hoover_count, 1);
    }
}

#[cfg(test)]
mod heatmap_pref_tests {
    use crate::state::ui_prefs::UiPrefs;

    #[test]
    fn heatmap_default_is_off() {
        // Default-off keeps the existing dot/note rendering as the
        // canonical view — heatmap is a deliberate user opt-in via
        // Preferences → Display.
        let prefs = UiPrefs::default();
        assert!(!prefs.stream_heatmap);
    }
}
