// ─── tests/voice_meter_strip_tests.rs ────────────────────────────────────────
// State-side tests for the VoiceMeterStrip viz module — ModuleKind
// label, alias parsing, and the slot-mapping helpers in
// `audio::voice_meters`.  The DSP-side write/read round-trip and
// slot exhaustion live in `audio/voice_meters.rs`.

#[cfg(test)]
mod voice_meter_strip_module_tests {
    use crate::state::ModuleKind;

    #[test]
    fn label_is_voice_levels() {
        assert_eq!(ModuleKind::VoiceMeterStrip.label(), "VOICE LEVELS");
    }

    #[test]
    fn parses_from_voice_meter_aliases() {
        use crate::state::rack_scope::parse_module_kind;
        for alias in [
            "voicemeter",
            "voicemeterstrip",
            "voice_meter",
            "voicemeters",
            "voicelevels",
            "voice_levels",
            "levels",
        ] {
            assert_eq!(
                parse_module_kind(alias),
                Some(ModuleKind::VoiceMeterStrip),
                "alias `{alias}` should parse"
            );
        }
    }

    #[test]
    fn lives_in_fxmod_zone() {
        // VoiceMeterStrip is a viz module, so it shares the FxMod zone
        // with the other meter / scope viz modules.
        assert_eq!(
            ModuleKind::VoiceMeterStrip.default_zone(),
            crate::state::Zone::FxMod
        );
    }

    #[test]
    fn has_no_audio_output() {
        // Pure UI module — never produces audio, never participates in
        // the FX chain or master bus.
        assert!(!ModuleKind::VoiceMeterStrip.has_audio_output());
    }

    #[test]
    fn is_singleton() {
        // Multiple meter strips would be redundant — they'd all read
        // from the same shared atomic array.
        assert!(!ModuleKind::VoiceMeterStrip.allows_multiple());
    }
}
