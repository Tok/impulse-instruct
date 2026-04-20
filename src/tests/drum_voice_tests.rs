// ─── tests/drum_voice_tests.rs ───────────────────────────────────────────────
// Covers three `DrumVoice` methods: `label`, `schema_key`, `module_kind`.
// Each is a pure enum dispatch used at prompt-build / schema-walk time —
// getting any one of them wrong silently sends the LLM's kick updates to
// the wrong voice, or leaves an LLM-visible key unmapped.

use crate::state::{DrumVoice, ModuleKind};

// ─── label ──────────────────────────────────────────────────────────────────

#[test]
fn every_drum_voice_has_a_non_empty_display_label() {
    // UI panels show `label()` on the voice row — an empty string would
    // render as a blank row.
    for voice in DrumVoice::ALL {
        let l = voice.label();
        assert!(!l.is_empty(), "{voice:?} label must not be empty");
        assert!(
            l.chars().any(|c| c.is_ascii_alphabetic()),
            "{voice:?} label should contain letters, got {l:?}",
        );
    }
}

#[test]
fn labels_are_distinct_across_voices() {
    // Each voice needs a distinguishable display name — duplicates
    // would make it impossible to tell which row you're editing.
    let labels: std::collections::HashSet<&'static str> =
        DrumVoice::ALL.iter().map(|v| v.label()).collect();
    assert_eq!(
        labels.len(),
        DrumVoice::ALL.len(),
        "all DrumVoice labels must be unique",
    );
}

// ─── schema_key ─────────────────────────────────────────────────────────────

#[test]
fn schema_keys_match_llm_contract() {
    // The LLM schema uses these exact strings — the pipeline / filter
    // layer reads them to route updates.  Changing any of these breaks
    // existing prompts / sessions silently.
    assert_eq!(DrumVoice::Kick808.schema_key(), Some("kick_a"));
    assert_eq!(DrumVoice::Snare808.schema_key(), Some("snare_a"));
    assert_eq!(DrumVoice::HihatClosed808.schema_key(), Some("hihat_a"));
    assert_eq!(DrumVoice::HihatOpen808.schema_key(), Some("hihat_a_open"));

    assert_eq!(DrumVoice::Kick909.schema_key(), Some("kick_b"));
    assert_eq!(DrumVoice::Snare909.schema_key(), Some("snare_b"));
    assert_eq!(DrumVoice::HihatClosed909.schema_key(), Some("hihat_b"));
    assert_eq!(DrumVoice::HihatOpen909.schema_key(), Some("hihat_b_open"));
    assert_eq!(DrumVoice::Clap909.schema_key(), Some("clap_b"));

    assert_eq!(DrumVoice::GabberKick.schema_key(), Some("gabber_kick"));
}

#[test]
fn schema_keys_are_distinct_where_defined() {
    // Voices that opt-in to the LLM schema must use distinct keys —
    // two voices sharing a key would leave one unreachable by the LLM.
    let mut seen = std::collections::HashSet::new();
    for voice in DrumVoice::ALL {
        if let Some(k) = voice.schema_key() {
            assert!(
                seen.insert(k),
                "{voice:?} re-uses schema key {k:?} already claimed by another voice",
            );
        }
    }
}

#[test]
fn schema_keys_omit_voices_not_exposed_to_llm() {
    // Toms, rim shot, and amen don't map cleanly onto the LLM's
    // kick/snare/hihat/clap vocabulary — they must return None so the
    // schema builder skips them.
    for voice in [
        DrumVoice::TomHi808,
        DrumVoice::TomMid808,
        DrumVoice::TomLo808,
        DrumVoice::Rim909,
        DrumVoice::Amen,
    ] {
        assert_eq!(
            voice.schema_key(),
            None,
            "{voice:?} should NOT have a schema key",
        );
    }
}

// ─── module_kind ────────────────────────────────────────────────────────────

#[test]
fn module_kind_routes_808_family_to_drum_kit_808() {
    for voice in [
        DrumVoice::Kick808,
        DrumVoice::Snare808,
        DrumVoice::HihatClosed808,
        DrumVoice::HihatOpen808,
        DrumVoice::TomHi808,
        DrumVoice::TomMid808,
        DrumVoice::TomLo808,
    ] {
        assert_eq!(
            voice.module_kind(),
            ModuleKind::DrumKit808,
            "{voice:?} must belong to DrumKit808",
        );
    }
}

#[test]
fn module_kind_routes_909_family_to_drum_kit_909() {
    for voice in [
        DrumVoice::Kick909,
        DrumVoice::Snare909,
        DrumVoice::HihatClosed909,
        DrumVoice::HihatOpen909,
        DrumVoice::Clap909,
        DrumVoice::Rim909,
    ] {
        assert_eq!(
            voice.module_kind(),
            ModuleKind::DrumKit909,
            "{voice:?} must belong to DrumKit909",
        );
    }
}

#[test]
fn module_kind_routes_amen_and_gabber_to_their_own_modules() {
    // Standalone drum voices each live in their own rack module.
    assert_eq!(DrumVoice::Amen.module_kind(), ModuleKind::AmenSampler);
    assert_eq!(DrumVoice::GabberKick.module_kind(), ModuleKind::GabberKick);
}

#[test]
fn every_drum_voice_maps_to_a_drum_shaped_module() {
    // Exhaustive check: every DrumVoice::ALL entry must resolve to one
    // of the four drum-shaped rack modules.  If a new voice lands
    // without wiring into module_kind, this test lights up.
    let drum_kinds = [
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::AmenSampler,
        ModuleKind::GabberKick,
    ];
    for voice in DrumVoice::ALL {
        let k = voice.module_kind();
        assert!(
            drum_kinds.contains(&k),
            "{voice:?} maps to non-drum ModuleKind {k:?}",
        );
    }
}
