// ─── tests/ui_prefs_tests.rs ─────────────────────────────────────────────────
// Covers the non-trivial persistent prefs rules:
//   • `AutosaveInterval::label` + `::duration` (what the settings tab reads)
//   • `HuthStyle` serde aliases (older sessions used `PianoOnly` / `Full` —
//     both must still deserialise into `On` so those saves don't lose
//     colour).
//   • `Zone::scroll_name` (every zone maps to the canonical lowercase
//     scroll-target string used by the /api/scroll endpoint).

use crate::state::{AutosaveInterval, HuthStyle, Zone};

// ─── AutosaveInterval ───────────────────────────────────────────────────────

#[test]
fn autosave_interval_label_is_human_readable() {
    assert_eq!(AutosaveInterval::Immediate.label(), "Immediate");
    assert_eq!(AutosaveInterval::FiveSec.label(), "5 seconds");
    assert_eq!(AutosaveInterval::ThirtySec.label(), "30 seconds");
    assert_eq!(AutosaveInterval::Manual.label(), "Manual only");
}

#[test]
fn autosave_interval_duration_only_returns_some_for_throttled_modes() {
    // Immediate and Manual both return None — the outer autosave loop
    // handles those specially (fire-every-change vs never-fire).  Only
    // the throttled modes carry a Duration.
    assert!(AutosaveInterval::Immediate.duration().is_none());
    assert!(AutosaveInterval::Manual.duration().is_none());

    assert_eq!(
        AutosaveInterval::FiveSec.duration(),
        Some(std::time::Duration::from_secs(5)),
    );
    assert_eq!(
        AutosaveInterval::ThirtySec.duration(),
        Some(std::time::Duration::from_secs(30)),
    );
}

#[test]
fn autosave_interval_default_is_immediate() {
    // Fresh preferences default to Immediate so new installs get the
    // pre-throttling save behaviour without configuring anything.
    assert_eq!(AutosaveInterval::default(), AutosaveInterval::Immediate);
}

// ─── HuthStyle serde aliases ────────────────────────────────────────────────

#[test]
fn huth_style_piano_only_alias_deserialises_to_on() {
    // Older saves wrote "PianoOnly" — deserialising must produce On so
    // users don't lose Huth note colouring on upgrade.
    let v: HuthStyle = serde_json::from_str("\"PianoOnly\"").unwrap();
    assert_eq!(v, HuthStyle::On);
}

#[test]
fn huth_style_full_alias_deserialises_to_on() {
    // Same invariant for the "Full" variant name from the three-state era.
    let v: HuthStyle = serde_json::from_str("\"Full\"").unwrap();
    assert_eq!(v, HuthStyle::On);
}

#[test]
fn huth_style_current_names_round_trip() {
    for v in [HuthStyle::Off, HuthStyle::On] {
        let json = serde_json::to_string(&v).unwrap();
        let parsed: HuthStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(v, parsed);
    }
}

// ─── Zone::scroll_name ──────────────────────────────────────────────────────

#[test]
fn zone_scroll_names_match_api_contract() {
    // The /api/scroll endpoint contract (documented in CLAUDE.md) lists
    // these four strings as the zone scroll targets.  Any rename here
    // silently breaks demo scripts and external controllers.
    assert_eq!(Zone::Ai.scroll_name(), "ai");
    assert_eq!(Zone::Global.scroll_name(), "global");
    assert_eq!(Zone::Voice.scroll_name(), "voice");
    assert_eq!(Zone::FxMod.scroll_name(), "fxmod");
}
