// ─── tests/rack_extras_tests.rs ──────────────────────────────────────────────
// Small-surface tests for rack helpers that didn't have dedicated coverage:
// `is_fx_module`, `zone_modules`, `arrange_canonical`, and `CableColor::egui_color`.
// Keeps each rule boxed-in so a future refactor can't accidentally drift the
// meaning of "FX module" or reshuffle the grid without flagging a test.

use crate::state::{CableColor, ModuleKind, RackState, Zone};

#[test]
fn is_fx_module_classifies_fx_kinds_only() {
    let mut rack = RackState::default();
    let reverb_id = rack.add_module(ModuleKind::FxReverb);
    let delay_id = rack.add_module(ModuleKind::FxDelay);
    let bass_id = rack.add_module(ModuleKind::AcidBass);
    let master_id = rack.add_module(ModuleKind::MasterOutput);
    let agent_id = rack.add_module(ModuleKind::LlmAgent);

    assert!(rack.is_fx_module(reverb_id));
    assert!(rack.is_fx_module(delay_id));
    assert!(!rack.is_fx_module(bass_id));
    assert!(!rack.is_fx_module(master_id));
    assert!(!rack.is_fx_module(agent_id));
}

#[test]
fn is_fx_module_returns_false_for_unknown_ids() {
    let rack = RackState::default();
    // Bogus id — not present in the rack.
    assert!(!rack.is_fx_module(999));
}

#[test]
fn zone_modules_filters_and_sorts_by_slot() {
    let mut rack = RackState::default();
    // Sequencer is pre-seeded in zone MainAudio; add two voices to the Voice zone.
    let bass_id = rack.add_module(ModuleKind::AcidBass);
    let kit_id = rack.add_module(ModuleKind::DrumKit808);

    let voice_ids: Vec<u32> = rack
        .zone_modules(Zone::Voice)
        .iter()
        .map(|m| m.id)
        .collect();
    assert!(voice_ids.contains(&bass_id));
    assert!(voice_ids.contains(&kit_id));
    // Canonical zone arrangement puts kit_a (808) before bass so the "low
    // drums / bass / high drums" ordering holds — we only assert both are
    // present here (slot-sort semantics are covered by the ordering tests
    // in arrange_canonical below).

    // Verify slot ordering: zone_modules sorts by slot, so iterating in
    // order yields monotonically non-decreasing slot numbers.
    let voice_mods = rack.zone_modules(Zone::Voice);
    for pair in voice_mods.windows(2) {
        assert!(pair[0].slot <= pair[1].slot);
    }
}

#[test]
fn arrange_canonical_places_bass_between_808_and_909() {
    // Canonical order inside the Voice zone: 808 → bass → 909. Build the
    // rack out of order and verify arrange_canonical pulls it into canonical
    // layout (bass centred between the low and high drum kits).
    let mut rack = RackState::default();
    let b909 = rack.add_module(ModuleKind::DrumKit909);
    let bass = rack.add_module(ModuleKind::AcidBass);
    let b808 = rack.add_module(ModuleKind::DrumKit808);

    rack.arrange_canonical();

    let voices = rack.zone_modules(Zone::Voice);
    let pos = |id: u32| voices.iter().position(|m| m.id == id).unwrap();
    assert!(pos(b808) < pos(bass), "808 must come before bass");
    assert!(pos(bass) < pos(b909), "bass must come before 909");
}

#[test]
fn cable_color_egui_color_is_grayscale() {
    // Huth notation aside, cable colors live in the grayscale palette —
    // every `CableColor::egui_color` must satisfy r == g == b so the theme
    // rule (no cable tints the UI) can't regress silently.
    for c in [
        CableColor::Gray,
        CableColor::Slate,
        CableColor::Silver,
        CableColor::Ash,
        CableColor::Stone,
        CableColor::Iron,
        CableColor::Pewter,
        CableColor::Smoke,
    ] {
        let col = c.egui_color();
        assert_eq!(col.r(), col.g(), "cable color {c:?} is not grayscale");
        assert_eq!(col.g(), col.b(), "cable color {c:?} is not grayscale");
    }
}
