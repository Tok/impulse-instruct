// ─── tests/rack_reach_tests.rs ───────────────────────────────────────────────
// `reaches_master` reachability tests + the XY-pad expansion tests that
// depend on the same empty-rack helpers.  Split out of rack_tests.rs to
// stay under the 1000-line cap; together the reach-master cases and the
// pad tests were pushing that file past ~960 lines.

use crate::state::rack::*;

/// Empty rack with no modules or cables — avoids default rack's 50+ cables.
fn empty_rack() -> RackState {
    RackState {
        modules: Vec::new(),
        cables: Vec::new(),
        next_id: 1,
        dyn_sequencer_rows: None,
    }
}

/// Force every FX module in the rack to `enabled = true`.
/// `RackModule::new` adds FX modules in a disabled state so freshly-
/// added effects can't click the signal at their default wet mix
/// (see the top of `state/rack.rs`).  These reachability tests
/// assert on plan-shape against an all-active rack, so they flip
/// the enabled bit back on before checking.
fn enable_all_fx(rack: &mut RackState) {
    for m in rack.modules.iter_mut() {
        if crate::state::fx_plan::kind_to_fx_step(m.kind).is_some() {
            m.enabled = true;
        }
    }
}

// ── reaches_master ──────────────────────────────────────────────────────────

fn audio_cable(from_id: u32, to_id: u32) -> Cable {
    Cable {
        from: PortRef {
            module_id: from_id,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        to: PortRef {
            module_id: to_id,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
        color: CableColor::Gray,
        audio_gain: 1.0,
    }
}

#[test]
fn reaches_master_no_master_module_is_false() {
    let mut rack = empty_rack();
    let bass = rack.add_module(ModuleKind::AcidBass);
    assert!(!rack.reaches_master(bass));
}

#[test]
fn reaches_master_audio_less_modules_always_true() {
    // Sequencer / LFO / agents have no audio bus and are never the
    // subject of "reaches MASTER" gating — they always pass.
    let mut rack = empty_rack();
    let _master = rack.add_module(ModuleKind::MasterOutput);
    let seq = rack.add_module(ModuleKind::StepSequencer);
    let lfo = rack.add_module(ModuleKind::LfoModule);
    let agent = rack.add_module(ModuleKind::LlmAgent);
    let console = rack.add_module(ModuleKind::LlmConsole);
    let meter = rack.add_module(ModuleKind::SpectrumAnalyzer);
    for id in [seq, lfo, agent, console, meter] {
        assert!(rack.reaches_master(id), "id {id} expected reachable");
    }
}

#[test]
fn reaches_master_master_itself_is_true() {
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    assert!(rack.reaches_master(master));
}

#[test]
fn reaches_master_direct_voice_to_master() {
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    rack.cables.push(audio_cable(bass, master));
    assert!(rack.reaches_master(bass));
}

#[test]
fn reaches_master_unconnected_voice_is_false() {
    let mut rack = empty_rack();
    let _master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    assert!(!rack.reaches_master(bass));
}

#[test]
fn reaches_master_through_fx_chain() {
    // bass → reverb → delay → master   ⇒ all three reach master.
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    let reverb = rack.add_module(ModuleKind::FxReverb);
    let delay = rack.add_module(ModuleKind::FxDelay);
    enable_all_fx(&mut rack);
    rack.cables.push(audio_cable(bass, reverb));
    rack.cables.push(audio_cable(reverb, delay));
    rack.cables.push(audio_cable(delay, master));
    assert!(rack.reaches_master(bass));
    assert!(rack.reaches_master(reverb));
    assert!(rack.reaches_master(delay));
}

#[test]
fn reaches_master_orphan_fx_chain_is_false() {
    // bass → master (direct), reverb → delay → compressor (chain ends in air).
    // Chain members must report false even though they're "connected" to
    // each other — they don't reach MASTER.
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    let reverb = rack.add_module(ModuleKind::FxReverb);
    let delay = rack.add_module(ModuleKind::FxDelay);
    let comp = rack.add_module(ModuleKind::FxCompressor);
    rack.cables.push(audio_cable(bass, master));
    rack.cables.push(audio_cable(reverb, delay));
    rack.cables.push(audio_cable(delay, comp));
    assert!(rack.reaches_master(bass));
    assert!(!rack.reaches_master(reverb), "orphan reverb should be off");
    assert!(!rack.reaches_master(delay), "orphan delay should be off");
    assert!(
        !rack.reaches_master(comp),
        "orphan compressor should be off"
    );
}

#[test]
fn reaches_master_disabled_intermediate_blocks_path() {
    // bass → reverb (disabled) → master.  bass should NOT reach master
    // because the path goes through a disabled module.  reverb itself
    // still has a direct cable to master, so its own walk would succeed
    // — the call site combines `enabled && reaches_master` to gate the
    // LED on the module's own enabled flag.
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    let reverb = rack.add_module(ModuleKind::FxReverb);
    rack.cables.push(audio_cable(bass, reverb));
    rack.cables.push(audio_cable(reverb, master));
    if let Some(m) = rack.modules.iter_mut().find(|m| m.id == reverb) {
        m.enabled = false;
    }
    assert!(!rack.reaches_master(bass));
    // reverb's own outgoing cable still terminates at master.
    assert!(rack.reaches_master(reverb));
}

#[test]
fn reaches_master_disabled_master_still_reachable() {
    // The MasterOutput's own enabled flag shouldn't gate reachability —
    // it always counts as the terminus.
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    rack.cables.push(audio_cable(bass, master));
    if let Some(m) = rack.modules.iter_mut().find(|m| m.id == master) {
        m.enabled = false;
    }
    assert!(rack.reaches_master(bass));
}

// ── Sequencer lane visibility ──────────────────────────────────────────────
//
// The sequencer panel uses the same predicate the front-panel LED does to
// decide whether to render a lane: `module is enabled AND reaches_master`.
// These tests pin that contract so a regression in either piece (the
// reachability walker or the lane filter) shows up here.

/// Replicates the closure used by `draw_sequencer` (and the height calc).
fn lane_visible(rack: &RackState, kind: ModuleKind) -> bool {
    rack.modules
        .iter()
        .any(|m| m.kind == kind && m.enabled && rack.reaches_master(m.id))
}

#[test]
fn lane_hidden_when_voice_unwired() {
    let mut rack = empty_rack();
    let _master = rack.add_module(ModuleKind::MasterOutput);
    let _bass = rack.add_module(ModuleKind::AcidBass);
    // No cable — bass doesn't reach master, so its sequencer lane stays hidden.
    assert!(!lane_visible(&rack, ModuleKind::AcidBass));
}

#[test]
fn lane_visible_when_voice_cabled_to_master() {
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    rack.cables.push(audio_cable(bass, master));
    assert!(lane_visible(&rack, ModuleKind::AcidBass));
}

#[test]
fn lane_visible_when_voice_reaches_via_fx() {
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    let reverb = rack.add_module(ModuleKind::FxReverb);
    enable_all_fx(&mut rack);
    rack.cables.push(audio_cable(bass, reverb));
    rack.cables.push(audio_cable(reverb, master));
    assert!(lane_visible(&rack, ModuleKind::AcidBass));
}

#[test]
fn lane_hidden_when_voice_disabled() {
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let bass = rack.add_module(ModuleKind::AcidBass);
    rack.cables.push(audio_cable(bass, master));
    if let Some(m) = rack.modules.iter_mut().find(|m| m.id == bass) {
        m.enabled = false;
    }
    assert!(!lane_visible(&rack, ModuleKind::AcidBass));
}

#[test]
fn drum_lane_filter_skips_unpatched_kits() {
    use crate::state::DrumVoice;
    let mut rack = empty_rack();
    let master = rack.add_module(ModuleKind::MasterOutput);
    let kit808 = rack.add_module(ModuleKind::DrumKit808);
    let _gabber = rack.add_module(ModuleKind::GabberKick);
    rack.cables.push(audio_cable(kit808, master));
    // 808 is wired → its drum voices should appear.
    let active: Vec<DrumVoice> = DrumVoice::ALL
        .iter()
        .filter(|v| lane_visible(&rack, v.module_kind()))
        .copied()
        .collect();
    assert!(active.contains(&DrumVoice::Kick808));
    // GabberKick has no cable to master → its lane stays hidden.
    assert!(!active.contains(&DrumVoice::GabberKick));
}

#[test]
fn lane_visible_for_default_preset() {
    // Every voice in every preset should drive a visible sequencer lane —
    // the same contract `reaches_master_default_preset_voices_all_reach`
    // covers, but expressed in lane-visibility terms.
    for preset in crate::state::RACK_PRESETS {
        let rack = RackState::from_preset(preset);
        for kind in [
            ModuleKind::AcidBass,
            ModuleKind::HooverLead,
            ModuleKind::An1xVoice,
            ModuleKind::DrumKit808,
            ModuleKind::DrumKit909,
            ModuleKind::AmenSampler,
            ModuleKind::GranularTexture,
            ModuleKind::GabberKick,
            ModuleKind::NoiseVoice,
        ] {
            // Skip kinds not present in this preset.
            if !rack.modules.iter().any(|m| m.kind == kind) {
                continue;
            }
            assert!(
                lane_visible(&rack, kind),
                "preset '{}' kind {:?} expected to drive a visible lane",
                preset.name,
                kind,
            );
        }
    }
}

#[test]
fn reaches_master_default_preset_voices_all_reach() {
    // All voices in every preset should reach MASTER through the default
    // wiring.  Catches regressions in `wire_default_cables`.  FX
    // modules are created disabled by default — enable them here so
    // "voice → FX → master" reachability walks aren't blocked by the
    // disabled bit.
    for preset in crate::state::RACK_PRESETS {
        let mut rack = RackState::from_preset(preset);
        enable_all_fx(&mut rack);
        for m in &rack.modules {
            if !m.kind.has_audio_output() {
                continue;
            }
            // Voice modules MUST reach master.  FX may not (the chain
            // is intentionally orphaned in some presets).
            let is_voice = !matches!(
                m.kind,
                ModuleKind::FxReverb
                    | ModuleKind::FxDelay
                    | ModuleKind::FxChorus
                    | ModuleKind::FxPhaser
                    | ModuleKind::FxRingMod
                    | ModuleKind::FxWaveshaper
                    | ModuleKind::FxBitcrush
                    | ModuleKind::FxEq
                    | ModuleKind::FxCompressor
                    | ModuleKind::FxTapeSat
                    | ModuleKind::FxDrive
                    | ModuleKind::FxAutotune
                    | ModuleKind::FxPan
            );
            if is_voice {
                assert!(
                    rack.reaches_master(m.id),
                    "preset '{}' voice {:?} should reach master",
                    preset.name,
                    m.kind,
                );
            }
        }
    }
}

// ── XY-pad expansion (grid size, arrange, serde) ────────────────────────────

#[test]
fn supports_xy_pad_true_for_every_fx_kind_only() {
    // All FxXxx kinds have a pad now; non-FX kinds should never report true.
    let fx_kinds = [
        ModuleKind::FxReverb,
        ModuleKind::FxDelay,
        ModuleKind::FxChorus,
        ModuleKind::FxPhaser,
        ModuleKind::FxRingMod,
        ModuleKind::FxWaveshaper,
        ModuleKind::FxBitcrush,
        ModuleKind::FxEq,
        ModuleKind::FxCompressor,
        ModuleKind::FxTapeSat,
        ModuleKind::FxDrive,
        ModuleKind::FxAutotune,
        ModuleKind::FxPan,
    ];
    for k in fx_kinds {
        assert!(k.supports_xy_pad(), "{k:?} should support an XY pad");
    }
    for k in [
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::StepSequencer,
        ModuleKind::MasterOutput,
        ModuleKind::LfoModule,
        ModuleKind::LlmAgent,
        ModuleKind::SpectrumAnalyzer,
    ] {
        assert!(!k.supports_xy_pad(), "{k:?} should not support an XY pad");
    }
}

#[test]
fn three_pair_labels_dispatch_matches_cycle_order() {
    use crate::ui::rack_content_pad::three_pair_labels;
    assert_eq!(three_pair_labels(0, "A", "B", "C"), ("A", "B"));
    assert_eq!(three_pair_labels(1, "A", "B", "C"), ("A", "C"));
    assert_eq!(three_pair_labels(2, "A", "B", "C"), ("B", "C"));
    // Defensive: indices ≥ 3 fall through to the B/C pair (the widget
    // cycles within 0..num_pairs, so this branch guards against typos).
    assert_eq!(three_pair_labels(99, "A", "B", "C"), ("B", "C"));
}

#[test]
fn effective_grid_size_expands_when_pad_expanded() {
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::FxAutotune);
    rack.modules
        .iter_mut()
        .find(|m| m.id == id)
        .unwrap()
        .pad_expanded = true;
    let m = rack.module(id).unwrap();
    let (w, h) = rack.effective_grid_size(m);
    let (base_w, base_h) = ModuleKind::FxAutotune.grid_size(GRID_COLS);
    assert_eq!(w, base_w);
    assert_eq!(
        h,
        base_h + 1,
        "expanded autotune should be one row taller (pad fills extra cell)"
    );
}

#[test]
fn effective_grid_size_matches_static_when_collapsed() {
    // New modules default to pad_expanded = false; no extra row reserved.
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::FxAutotune);
    let m = rack.module(id).unwrap();
    let (w, h) = rack.effective_grid_size(m);
    assert_eq!((w, h), ModuleKind::FxAutotune.grid_size(GRID_COLS));
}

#[test]
fn effective_grid_size_ignores_flag_for_unsupported_kind() {
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::LfoModule);
    // LFO modules don't have XY pads — the flag is inert on them.
    rack.modules
        .iter_mut()
        .find(|m| m.id == id)
        .unwrap()
        .pad_expanded = true;
    let m = rack.module(id).unwrap();
    let (w, h) = rack.effective_grid_size(m);
    assert_eq!((w, h), ModuleKind::LfoModule.grid_size(GRID_COLS));
}

#[test]
fn arrange_grid_reserves_extra_row_for_expanded_autotune() {
    let mut rack = empty_rack();
    let at = rack.add_module(ModuleKind::FxAutotune);
    // Expand both pads — the reflow must not overlap them vertically.
    let at2 = rack.add_module(ModuleKind::FxAutotune);
    for id in [at, at2] {
        rack.modules
            .iter_mut()
            .find(|m| m.id == id)
            .unwrap()
            .pad_expanded = true;
    }
    rack.arrange_grid();
    let m1 = rack.module(at).unwrap();
    let m2 = rack.module(at2).unwrap();
    let (_, h1) = rack.effective_grid_size(m1);
    let (_, h2) = rack.effective_grid_size(m2);
    // Same zone, different placements. No vertical overlap.
    if m1.grid_col == m2.grid_col {
        let overlap = m1.grid_row < m2.grid_row + h2 && m2.grid_row < m1.grid_row + h1;
        assert!(!overlap, "expanded autotunes should not overlap");
    }
}

#[test]
fn pad_expanded_defaults_false_on_new_module() {
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::FxAutotune);
    assert!(
        !rack.module(id).unwrap().pad_expanded,
        "pad_expanded should default to false — users opt in per-module"
    );
    assert_eq!(
        rack.module(id).unwrap().pad_pair,
        0,
        "pad_pair should default to 0 (A/B)"
    );
}

#[test]
fn pad_expanded_serde_defaults_false_for_legacy_payload() {
    // Legacy session payloads that predate the pad fields must deserialize
    // with the new defaults (pad_expanded = false, pad_pair = 0).
    let legacy = r#"{
        "id": 42,
        "kind": "FxAutotune",
        "enabled": true,
        "zone": "FxMod",
        "slot": 0
    }"#;
    let m: RackModule = serde_json::from_str(legacy).unwrap();
    assert!(!m.pad_expanded);
    assert_eq!(m.pad_pair, 0);
}
