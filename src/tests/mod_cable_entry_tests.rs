// ─── tests/mod_cable_entry_tests.rs ──────────────────────────────────────────
// Covers `apply_llm_mod_cable_entry` — the handler that turns a single
// `rack.mod_cable` JSON entry from the LLM (`{from_lfo, to, slot, depth?,
// targets?}`) into an actual mod-cable, depth setting, and selector
// target list on the rack.  The LLM emits these as an array in the
// `rack.mod_cable` key; this function is called per-entry.

use crate::state::modulation::apply_llm_mod_cable_entry;
use crate::state::{LfoTarget, ModuleKind, PortKind, RackState};

/// Build a rack with exactly one AcidBass + one LfoModule.  Default
/// rack already seeds an AcidBass — we start from default, add our LFO,
/// and look up the existing bass id by kind (rack_kind_name_matches is
/// "first-match-wins", so the test fixture must mirror that semantics).
fn rack_with_lfo_and_bass() -> (RackState, u32, u32) {
    let mut rack = RackState::default();
    let lfo = rack.add_module(ModuleKind::LfoModule);
    let bass = rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::AcidBass)
        .map(|m| m.id)
        .expect("default rack seeds an AcidBass");
    (rack, lfo, bass)
}

#[test]
fn connects_lfo_out_to_bass_mod_input_slot() {
    let (mut rack, _lfo, _bass) = rack_with_lfo_and_bass();
    let entry = serde_json::json!({
        "from_lfo": 0,
        "to": "bass",
        "slot": 1,
    });
    apply_llm_mod_cable_entry(&mut rack, &entry);
    // Mod cable: source kind=Cv (LFO output), dest kind=Mod slot 1.
    let cable = rack
        .cables
        .iter()
        .find(|c| c.to.kind == PortKind::Mod)
        .expect("mod cable should be patched");
    assert_eq!(cable.from.kind, PortKind::Cv);
    assert_eq!(cable.to.index, 1);
}

#[test]
fn unknown_target_name_is_silent_noop() {
    // `to: "nonexistent_module"` — the name doesn't match any rack kind
    // so apply_llm_mod_cable_entry must silently bail and leave the
    // cable list untouched.  LLM hallucination shouldn't crash the app.
    let (mut rack, _lfo, _bass) = rack_with_lfo_and_bass();
    let before = rack.cables.len();
    let entry = serde_json::json!({
        "from_lfo": 0,
        "to": "nonexistent_module",
        "slot": 0,
    });
    apply_llm_mod_cable_entry(&mut rack, &entry);
    assert_eq!(rack.cables.len(), before, "unknown target must no-op");
}

#[test]
fn depth_is_written_and_clamped_to_unit_range() {
    let (mut rack, _lfo, bass) = rack_with_lfo_and_bass();
    // Depth above 1 should clamp to 1.
    apply_llm_mod_cable_entry(
        &mut rack,
        &serde_json::json!({"from_lfo": 0, "to": "bass", "slot": 0, "depth": 1.5}),
    );
    let m = rack.modules.iter().find(|m| m.id == bass).unwrap();
    assert_eq!(m.mod_input_depths[0], 1.0, "depth > 1 must clamp to 1");
    // Depth below 0 should clamp to 0.
    apply_llm_mod_cable_entry(
        &mut rack,
        &serde_json::json!({"from_lfo": 0, "to": "bass", "slot": 0, "depth": -0.2}),
    );
    let m = rack.modules.iter().find(|m| m.id == bass).unwrap();
    assert_eq!(m.mod_input_depths[0], 0.0, "depth < 0 must clamp to 0");
}

#[test]
fn depth_vec_grows_when_slot_is_beyond_current_len() {
    let (mut rack, _lfo, bass) = rack_with_lfo_and_bass();
    // Write to slot 3 — if mod_input_depths was shorter, the fn must
    // grow it with unity (1.0) defaults so prior slots aren't accidentally
    // zeroed out.
    apply_llm_mod_cable_entry(
        &mut rack,
        &serde_json::json!({"from_lfo": 0, "to": "bass", "slot": 3, "depth": 0.4}),
    );
    let m = rack.modules.iter().find(|m| m.id == bass).unwrap();
    assert!(m.mod_input_depths.len() >= 4);
    assert!((m.mod_input_depths[3] - 0.4).abs() < 1e-4);
    // Slots 0..=2 must be 1.0 (unity default), NOT 0.
    for i in 0..3 {
        assert_eq!(
            m.mod_input_depths[i], 1.0,
            "slot {i} must retain unity when growing for a later slot",
        );
    }
}

#[test]
fn targets_are_parsed_and_stored_in_selector_slot() {
    let (mut rack, _lfo, bass) = rack_with_lfo_and_bass();
    let entry = serde_json::json!({
        "from_lfo": 0,
        "to": "bass",
        "slot": 0,
        "targets": ["BassCutoff", "BassResonance", "NotARealTarget"],
    });
    apply_llm_mod_cable_entry(&mut rack, &entry);
    let m = rack.modules.iter().find(|m| m.id == bass).unwrap();
    let sel = &m.mod_selectors[0];
    // Valid names in; unknown filtered out.
    assert_eq!(sel.len(), 2);
    assert!(sel.contains(&LfoTarget::BassCutoff));
    assert!(sel.contains(&LfoTarget::BassResonance));
}

#[test]
fn targets_overwrite_rather_than_append_on_repeat_entry() {
    let (mut rack, _lfo, bass) = rack_with_lfo_and_bass();
    apply_llm_mod_cable_entry(
        &mut rack,
        &serde_json::json!({"from_lfo": 0, "to": "bass", "slot": 0, "targets": ["BassCutoff"]}),
    );
    apply_llm_mod_cable_entry(
        &mut rack,
        &serde_json::json!({"from_lfo": 0, "to": "bass", "slot": 0, "targets": ["BassResonance"]}),
    );
    let m = rack.modules.iter().find(|m| m.id == bass).unwrap();
    let sel = &m.mod_selectors[0];
    assert_eq!(sel, &vec![LfoTarget::BassResonance]);
}

#[test]
fn out_of_range_lfo_index_is_silent_noop() {
    // Default rack seeds 4 LFOs; from_lfo=99 is past the end.  The
    // function must silently bail rather than indexing a missing module.
    let (mut rack, _lfo, _bass) = rack_with_lfo_and_bass();
    let before = rack.cables.len();
    apply_llm_mod_cable_entry(
        &mut rack,
        &serde_json::json!({"from_lfo": 99, "to": "bass", "slot": 0}),
    );
    assert_eq!(rack.cables.len(), before);
}

#[test]
fn from_lfo_default_is_zero_when_key_absent() {
    // `from_lfo` is read with `.unwrap_or(0)` — entries that omit it
    // must still patch using the first LFO.
    let (mut rack, _lfo, _bass) = rack_with_lfo_and_bass();
    let entry = serde_json::json!({"to": "bass", "slot": 0});
    apply_llm_mod_cable_entry(&mut rack, &entry);
    assert!(
        rack.cables.iter().any(|c| c.to.kind == PortKind::Mod),
        "missing from_lfo should default to 0 and still patch",
    );
}
