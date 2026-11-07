// ─── tests/compile_mod_routes_tests.rs ───────────────────────────────────────
// Covers `compile_mod_routes` — walks the rack's Mod cables and emits a
// fixed-size `[ModRouteCopy; MAX_MOD_ROUTES]` array for the audio thread.
// Every cable must resolve its source LFO to a slot index (position in
// the LfoModule sequence), its target module's Mod-In jack to a
// LfoTarget list (Fixed or Selector), and emit one compiled route per
// target entry.
//
// This sits between the rack graph (user-facing) and the audio thread
// (opcode dispatch), so bugs here silently break user cable patches.

use crate::audio::dsp::{compile_mod_routes, lfo_target_to_u8};
use crate::state::{AppState, LfoTarget, ModuleKind, PortDir, PortKind, PortRef, RackState};

fn rack_with_lfo_and_bass() -> (AppState, u32, u32) {
    // Build a minimal rack: one LfoModule, one AcidBass, one Master.
    // Use an empty rack (not default) so we control exactly which modules
    // appear and their IDs don't drift.
    let rack = RackState::default();
    // Default rack already has LfoModule(s) + AcidBass; locate them.
    let lfo_id = rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .expect("default rack must seed at least one LfoModule");
    let bass_id = rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::AcidBass)
        .map(|m| m.id)
        .expect("default rack must seed one AcidBass");
    let mut s = AppState::default();
    s.rack = rack;
    (s, lfo_id, bass_id)
}

fn patch_mod_cable(s: &mut AppState, from_lfo: u32, to_module: u32, slot: u8) {
    s.rack.connect(
        PortRef {
            module_id: from_lfo,
            dir: PortDir::Out,
            kind: PortKind::Cv,
            index: 0,
        },
        PortRef {
            module_id: to_module,
            dir: PortDir::In,
            kind: PortKind::Mod,
            index: slot,
        },
    );
}

// ─── Empty cases ────────────────────────────────────────────────────────────

#[test]
fn default_rack_without_mod_cables_compiles_to_zero_routes() {
    // Default rack has audio cables but no Mod cables.  Must produce
    // count=0 and leave the array at default (zero) values.
    let s = AppState::default();
    let (_routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 0, "no mod cables → no compiled routes");
}

#[test]
fn non_cv_cables_are_skipped() {
    // Only CV-to-Mod cables should compile.  Audio cables must not
    // produce routes even if their shape would otherwise fit.
    let s = AppState::default();
    let (_routes, count) = compile_mod_routes(&s);
    // Default rack has many audio cables; count must still be 0.
    let audio_cable_count = s
        .rack
        .cables
        .iter()
        .filter(|c| c.from.kind == PortKind::Audio)
        .count();
    assert!(
        audio_cable_count > 0,
        "test premise: default rack has audio cables"
    );
    assert_eq!(count, 0, "audio cables must never compile into mod routes");
}

// ─── Single route — Fixed input ────────────────────────────────────────────

#[test]
fn fixed_input_slot_compiles_to_single_route_with_matching_target() {
    // AcidBass's mod_inputs[0] is `Fixed(BassPan)` — patching LFO → Bass
    // Mod slot 0 should produce exactly one route with target=BassPan.
    let (mut s, lfo_id, bass_id) = rack_with_lfo_and_bass();
    patch_mod_cable(&mut s, lfo_id, bass_id, 0);
    let (routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 1, "one Fixed(BassPan) cable → one route");
    let r = routes[0];
    assert_eq!(r.source_buf_idx, 0, "first LFO in rack → buf idx 0");
    assert_eq!(
        r.target_u8,
        lfo_target_to_u8(LfoTarget::BassPan),
        "Fixed(BassPan) must compile to the BassPan opcode",
    );
}

#[test]
fn route_depth_defaults_to_unity_when_not_explicitly_set() {
    // mod_input_depths defaults to empty → the compiler clamps a
    // missing entry to 1.0.  A route with default depth must report
    // 1.0 (not 0.0) so the patched cable is actually audible.
    let (mut s, lfo_id, bass_id) = rack_with_lfo_and_bass();
    patch_mod_cable(&mut s, lfo_id, bass_id, 0);
    let (routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 1);
    assert!(
        (routes[0].depth - 1.0).abs() < 1e-4,
        "default depth must be 1.0 (unity), got {}",
        routes[0].depth,
    );
}

#[test]
fn depth_is_clamped_to_unit_range_and_inverted_by_invert_flag() {
    // Explicit depth outside 0..1 must clamp.  invert=true flips the
    // sign (bipolar routing), so the compiled depth is negative.
    let (mut s, lfo_id, bass_id) = rack_with_lfo_and_bass();
    patch_mod_cable(&mut s, lfo_id, bass_id, 0);
    // Poke the bass module's depth + invert for slot 0.
    let bass = s.rack.modules.iter_mut().find(|m| m.id == bass_id).unwrap();
    bass.mod_input_depths = vec![1.5]; // clamps to 1.0
    bass.mod_input_invert = vec![true];
    let (routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 1);
    assert!(
        (routes[0].depth + 1.0).abs() < 1e-4,
        "clamped-to-1 + invert → -1.0, got {}",
        routes[0].depth,
    );
    // Negative depth clamps to 0 pre-invert → 0 post-invert.
    let bass = s.rack.modules.iter_mut().find(|m| m.id == bass_id).unwrap();
    bass.mod_input_depths = vec![-0.5];
    bass.mod_input_invert = vec![false];
    let (routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 1);
    assert!(
        routes[0].depth.abs() < 1e-4,
        "negative depth clamps to 0, got {}",
        routes[0].depth,
    );
}

// ─── Selector inputs — multi-target ────────────────────────────────────────

#[test]
fn selector_input_emits_one_route_per_target() {
    // DrumKit808's mod_inputs are three Selectors.  Patching an LFO
    // into slot 0 with a 3-item selector list must emit 3 routes
    // (one per selected target) — not one.
    let mut s = AppState::default();
    let lfo_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .unwrap();
    let kit_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::DrumKit808)
        .map(|m| m.id)
        .unwrap();
    patch_mod_cable(&mut s, lfo_id, kit_id, 0);
    // Populate the selector list with three distinct kit targets.
    let kit = s.rack.modules.iter_mut().find(|m| m.id == kit_id).unwrap();
    kit.mod_selectors = vec![vec![
        LfoTarget::Kick808Pitch,
        LfoTarget::Snare808Tone,
        LfoTarget::Hihat808Pan,
    ]];
    let (routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 3, "one selector cable with 3 targets → 3 routes");
    let targets: Vec<u8> = (0..count as usize).map(|i| routes[i].target_u8).collect();
    for expected in [
        LfoTarget::Kick808Pitch,
        LfoTarget::Snare808Tone,
        LfoTarget::Hihat808Pan,
    ] {
        let opcode = lfo_target_to_u8(expected);
        assert!(
            targets.contains(&opcode),
            "expected opcode for {expected:?} missing from compiled routes",
        );
    }
}

#[test]
fn selector_with_none_target_is_filtered_out() {
    // LfoTarget::None is a no-op sentinel; selector lists containing it
    // must drop that entry instead of burning a dispatch slot.
    let mut s = AppState::default();
    let lfo_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .unwrap();
    let kit_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::DrumKit808)
        .map(|m| m.id)
        .unwrap();
    patch_mod_cable(&mut s, lfo_id, kit_id, 0);
    let kit = s.rack.modules.iter_mut().find(|m| m.id == kit_id).unwrap();
    // Two Nones + one real target → one route.
    kit.mod_selectors = vec![vec![
        LfoTarget::None,
        LfoTarget::Kick808Pitch,
        LfoTarget::None,
    ]];
    let (_routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 1, "None targets must not compile");
}

#[test]
fn empty_selector_list_emits_zero_routes_without_panicking() {
    // An unpopulated selector (empty Vec) must compile to no routes —
    // not panic, not default to "every target".  A common case when
    // the user patches a cable but hasn't picked a target yet.
    let mut s = AppState::default();
    let lfo_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .unwrap();
    let kit_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::DrumKit808)
        .map(|m| m.id)
        .unwrap();
    patch_mod_cable(&mut s, lfo_id, kit_id, 0);
    // No mod_selectors set → empty.
    let (_routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 0, "empty selector → no compiled routes");
}

// ─── LFO slot resolution ────────────────────────────────────────────────────

#[test]
fn lfo_slot_index_reflects_lfo_module_order() {
    // The default rack seeds 4 LfoModules.  A cable from the THIRD
    // LfoModule must compile to source_buf_idx = MOD_BUF_LFO_BASE + 2,
    // not the first slot.
    use crate::audio::dsp::MOD_BUF_LFO_BASE;
    let mut s = AppState::default();
    let lfos: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .collect();
    assert!(
        lfos.len() >= 3,
        "test premise: need ≥3 LFOs in default rack"
    );
    let bass_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::AcidBass)
        .map(|m| m.id)
        .unwrap();
    // Patch LFO #3 → bass Mod 0 (Fixed(BassPan)).
    patch_mod_cable(&mut s, lfos[2], bass_id, 0);
    let (routes, count) = compile_mod_routes(&s);
    assert_eq!(count, 1);
    assert_eq!(
        routes[0].source_buf_idx as usize,
        MOD_BUF_LFO_BASE + 2,
        "third LFO must resolve to buf idx LFO_BASE + 2",
    );
}

// ─── CV sequencer source ────────────────────────────────────────────────────

#[test]
fn cv_sequencer_cable_compiles_to_cv_seq_buf_idx() {
    // A cable from CvSequencer → AcidBass.Mod[0] (Fixed(BassPan))
    // must resolve to source_buf_idx = MOD_BUF_CV_SEQ_BASE + (cv-
    // seq module index in rack order).
    use crate::audio::dsp::MOD_BUF_CV_SEQ_BASE;
    let mut s = AppState::default();
    let cv_seq_id = s.rack.add_module(ModuleKind::CvSequencer);
    let bass_id = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::AcidBass)
        .map(|m| m.id)
        .unwrap();
    patch_mod_cable(&mut s, cv_seq_id, bass_id, 0);
    let (routes, count) = compile_mod_routes(&s);
    assert!(count >= 1, "CvSeq → bass cable must compile to a route");
    let cv_route = routes
        .iter()
        .take(count as usize)
        .find(|r| r.source_buf_idx as usize >= MOD_BUF_CV_SEQ_BASE)
        .expect("at least one route should source from a cv-seq slot");
    assert_eq!(
        cv_route.source_buf_idx as usize, MOD_BUF_CV_SEQ_BASE,
        "first CvSequencer in rack → CV_SEQ_BASE + 0",
    );
    assert_eq!(
        cv_route.target_u8,
        lfo_target_to_u8(LfoTarget::BassPan),
        "Fixed(BassPan) opcode must propagate from the cable target",
    );
}
