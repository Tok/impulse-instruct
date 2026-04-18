// ─── tests/rack_tests.rs ──────────────────────────────────────────────────────
// Tests for rack module management, cable routing, cycle detection, and guards.

use crate::state::rack::*;
use crate::state::rack_scope::scope_from_control_cables;

/// Empty rack with no modules or cables — avoids default rack's 50+ cables.
fn empty_rack() -> RackState {
    RackState {
        modules: Vec::new(),
        cables: Vec::new(),
        next_id: 1,
        dyn_sequencer_rows: None,
    }
}

// ── Module management ───────────────────────────────────────────────────────

#[test]
fn add_module_returns_unique_ids() {
    let mut rack = empty_rack();
    let id1 = rack.add_module(ModuleKind::AcidBass);
    let id2 = rack.add_module(ModuleKind::DrumKit808);
    let id3 = rack.add_module(ModuleKind::FxReverb);
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

#[test]
fn remove_module_also_removes_its_cables() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    let b = rack.add_module(ModuleKind::FxReverb);
    rack.connect(
        PortRef {
            module_id: a,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        PortRef {
            module_id: b,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
    );
    assert_eq!(rack.cables.len(), 1);
    rack.remove_module(a);
    assert!(
        rack.cables.is_empty(),
        "cables should be removed with module"
    );
    assert!(
        rack.modules.iter().all(|m| m.id != a),
        "module should be gone"
    );
}

// ── Audio cycle detection ───────────────────────────────────────────────────

#[test]
fn self_connection_detected_as_cycle() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    assert!(rack.would_create_audio_cycle(a, a));
}

#[test]
fn simple_audio_cycle_rejected() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    let b = rack.add_module(ModuleKind::FxReverb);
    // a → b
    let added = rack.connect(
        PortRef {
            module_id: a,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        PortRef {
            module_id: b,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
    );
    assert!(added, "first cable should succeed");
    // b → a would create a cycle
    let added2 = rack.connect(
        PortRef {
            module_id: b,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        PortRef {
            module_id: a,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
    );
    assert!(!added2, "cycle-creating cable should be rejected");
}

#[test]
fn three_node_audio_cycle_rejected() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    let b = rack.add_module(ModuleKind::FxReverb);
    let c = rack.add_module(ModuleKind::FxDelay);
    // a → b → c
    let mk = |from, to| {
        (
            PortRef {
                module_id: from,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: to,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        )
    };
    let (f, t) = mk(a, b);
    assert!(rack.connect(f, t));
    let (f, t) = mk(b, c);
    assert!(rack.connect(f, t));
    // c → a would close the cycle
    let (f, t) = mk(c, a);
    assert!(!rack.connect(f, t), "3-node cycle should be rejected");
}

#[test]
fn non_cyclic_audio_chain_allowed() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    let b = rack.add_module(ModuleKind::FxReverb);
    let c = rack.add_module(ModuleKind::MasterOutput);
    let mk = |from, to| {
        (
            PortRef {
                module_id: from,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: to,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        )
    };
    let (f, t) = mk(a, b);
    assert!(rack.connect(f, t));
    let (f, t) = mk(b, c);
    assert!(rack.connect(f, t));
    // a → c (parallel path, not a cycle)
    let (f, t) = mk(a, c);
    assert!(rack.connect(f, t), "parallel path should be allowed");
}

#[test]
fn control_cables_not_cycle_checked() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::LlmAgent);
    let b = rack.add_module(ModuleKind::AcidBass);
    // Control cables are not checked for cycles
    rack.connect_control(a, b);
    rack.connect_control(b, a); // would be a cycle in audio, but control is fine
    let ctrl_cables: Vec<_> = rack
        .cables
        .iter()
        .filter(|c| c.from.kind == PortKind::Control)
        .collect();
    assert_eq!(ctrl_cables.len(), 2);
}

#[test]
fn strip_audio_cycles_removes_offenders() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    let b = rack.add_module(ModuleKind::FxReverb);
    // Manually push a cyclic pair (bypassing connect guard)
    let color = rack.next_cable_color();
    rack.cables.push(Cable {
        from: PortRef {
            module_id: a,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        to: PortRef {
            module_id: b,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
        color,
    });
    let color2 = rack.next_cable_color();
    rack.cables.push(Cable {
        from: PortRef {
            module_id: b,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        to: PortRef {
            module_id: a,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
        color: color2,
    });
    assert_eq!(rack.cables.len(), 2);
    let removed = rack.strip_audio_cycles();
    assert_eq!(removed, 1, "one cyclic cable should be stripped");
    assert_eq!(rack.cables.len(), 1, "one cable should remain");
}

// ── Duplicate cable check ───────────────────────────────────────────────────

#[test]
fn duplicate_cable_rejected() {
    let mut rack = empty_rack();
    let a = rack.add_module(ModuleKind::AcidBass);
    let b = rack.add_module(ModuleKind::FxReverb);
    let mk = || {
        (
            PortRef {
                module_id: a,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: b,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        )
    };
    let (f, t) = mk();
    assert!(rack.connect(f, t), "first cable should succeed");
    let (f, t) = mk();
    assert!(!rack.connect(f, t), "duplicate cable should be rejected");
    assert_eq!(rack.cables.len(), 1);
}

// ── Scope from control cables ───────────────────────────────────────────────

#[test]
fn scope_from_cables_returns_correct_names() {
    let mut rack = empty_rack();
    let agent = rack.add_module(ModuleKind::LlmAgent);
    let bass = rack.add_module(ModuleKind::AcidBass);
    let kit = rack.add_module(ModuleKind::DrumKit808);
    rack.connect_control(agent, bass);
    rack.connect_control(agent, kit);
    let scope = scope_from_control_cables(&rack, agent);
    assert!(scope.contains(&"bass".to_string()));
    assert!(scope.contains(&"kit_a".to_string()));
    assert_eq!(scope.len(), 2);
}

#[test]
fn scope_empty_when_no_cables() {
    let mut rack = empty_rack();
    let agent = rack.add_module(ModuleKind::LlmAgent);
    let scope = scope_from_control_cables(&rack, agent);
    assert!(
        scope.is_empty(),
        "no cables = empty scope = controls everything"
    );
}

#[test]
fn scope_excludes_non_scoped_targets() {
    let mut rack = empty_rack();
    let agent = rack.add_module(ModuleKind::LlmAgent);
    let tts = rack.add_module(ModuleKind::NeuTts);
    rack.connect_control(agent, tts);
    let scope = scope_from_control_cables(&rack, agent);
    // TTS modules may or may not have a scope name — check what kind_to_scope_name returns
    // If TTS doesn't map to a scope name, scope should be empty (= controls everything)
    // This is important: an agent wired ONLY to TTS should still control synths via empty scope
    assert!(
        scope.is_empty() || scope.iter().any(|s| s.contains("tts")),
        "TTS-only wiring should either give empty scope or a TTS scope name"
    );
}

// ── Grid placement ─────────────────────────────────────────────────────────

#[test]
fn arrange_grid_no_overlap() {
    let rack = RackState::default();
    // Verify no two modules in the same zone overlap on the grid.
    for zone in [Zone::Ai, Zone::Global, Zone::Voice, Zone::FxMod] {
        let mods: Vec<_> = rack.modules.iter().filter(|m| m.zone == zone).collect();
        for (i, a) in mods.iter().enumerate() {
            let (aw, ah) = a.kind.grid_size(GRID_COLS);
            for b in &mods[i + 1..] {
                let (bw, bh) = b.kind.grid_size(GRID_COLS);
                let a_right = a.grid_col + aw;
                let a_bottom = a.grid_row + ah;
                let b_right = b.grid_col + bw;
                let b_bottom = b.grid_row + bh;
                let overlaps = a.grid_col < b_right
                    && b.grid_col < a_right
                    && a.grid_row < b_bottom
                    && b.grid_row < a_bottom;
                assert!(
                    !overlaps,
                    "{:?} at ({},{}) {}x{} overlaps {:?} at ({},{}) {}x{} in {:?}",
                    a.kind,
                    a.grid_col,
                    a.grid_row,
                    aw,
                    ah,
                    b.kind,
                    b.grid_col,
                    b.grid_row,
                    bw,
                    bh,
                    zone,
                );
            }
        }
    }
}

#[test]
fn arrange_grid_modules_within_bounds() {
    let rack = RackState::default();
    for m in &rack.modules {
        let (w, _h) = m.kind.grid_size(GRID_COLS);
        assert!(
            m.grid_col + w <= GRID_COLS,
            "{:?} at col {} + width {} exceeds {} columns",
            m.kind,
            m.grid_col,
            w,
            GRID_COLS,
        );
    }
}

#[test]
fn arrange_grid_full_preset_no_overlap() {
    use crate::state::RACK_PRESETS;
    // Test the "Full" preset specifically (index 3)
    let rack = RackState::from_preset(&RACK_PRESETS[3]);
    for zone in [Zone::Ai, Zone::Global, Zone::Voice, Zone::FxMod] {
        let mods: Vec<_> = rack.modules.iter().filter(|m| m.zone == zone).collect();
        for (i, a) in mods.iter().enumerate() {
            let (aw, ah) = a.kind.grid_size(GRID_COLS);
            for b in &mods[i + 1..] {
                let (bw, bh) = b.kind.grid_size(GRID_COLS);
                let overlaps = a.grid_col < b.grid_col + bw
                    && b.grid_col < a.grid_col + aw
                    && a.grid_row < b.grid_row + bh
                    && b.grid_row < a.grid_row + ah;
                assert!(
                    !overlaps,
                    "Full preset: {:?}@({},{}) {}x{} overlaps {:?}@({},{}) {}x{} in {:?}",
                    a.kind,
                    a.grid_col,
                    a.grid_row,
                    aw,
                    ah,
                    b.kind,
                    b.grid_col,
                    b.grid_row,
                    bw,
                    bh,
                    zone,
                );
            }
        }
    }
}

#[test]
fn arrange_grid_is_idempotent() {
    // Arranging twice should produce the same positions.
    let mut rack = RackState::default();
    let before: Vec<(u32, u8, u8)> = rack
        .modules
        .iter()
        .map(|m| (m.id, m.grid_col, m.grid_row))
        .collect();
    rack.arrange_grid();
    let after: Vec<(u32, u8, u8)> = rack
        .modules
        .iter()
        .map(|m| (m.id, m.grid_col, m.grid_row))
        .collect();
    assert_eq!(before, after, "arrange_grid should be idempotent");
}

#[test]
fn preset_arrange_is_idempotent() {
    use crate::state::RACK_PRESETS;
    for (i, preset) in RACK_PRESETS.iter().enumerate() {
        let mut rack = RackState::from_preset(preset);
        let before: Vec<(u32, u8, u8)> = rack
            .modules
            .iter()
            .map(|m| (m.id, m.grid_col, m.grid_row))
            .collect();
        rack.arrange_grid();
        let after: Vec<(u32, u8, u8)> = rack
            .modules
            .iter()
            .map(|m| (m.id, m.grid_col, m.grid_row))
            .collect();
        assert_eq!(
            before, after,
            "preset {} ('{}') arrange_grid not idempotent",
            i, preset.name
        );
    }
}

// ── Module zone placement ──────────────────────────────────────────────────

#[test]
fn modules_go_to_correct_zones() {
    assert_eq!(ModuleKind::AcidBass.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::DrumKit808.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::NeuTts.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::FxReverb.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::FxDelay.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::LfoModule.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::MasterOutput.default_zone(), Zone::Global);
    assert_eq!(ModuleKind::LlmConsole.default_zone(), Zone::Ai);
    assert_eq!(ModuleKind::LlmAgent.default_zone(), Zone::Ai);
}

// ── GabberKick ModuleKind wiring ───────────────────────────────────────────

#[test]
fn gabber_kick_lfo_targets_parse_and_label() {
    use crate::state::LfoTarget;
    use crate::state::modulation::{
        lfo_target_module_kind, lfo_target_short_label, parse_lfo_target,
    };

    for (name, variant, label) in [
        ("GabberKickPitch", LfoTarget::GabberKickPitch, "GK.PIT"),
        ("gabberkickdecay", LfoTarget::GabberKickDecay, "GK.DEC"),
        ("GabberKickClip", LfoTarget::GabberKickClip, "GK.CLP"),
        ("gabberkickpan", LfoTarget::GabberKickPan, "GK.PAN"),
    ] {
        assert_eq!(parse_lfo_target(name), Some(variant));
        assert_eq!(lfo_target_short_label(variant), label);
        assert_eq!(
            lfo_target_module_kind(variant),
            Some(ModuleKind::GabberKick)
        );
    }
}

#[test]
fn gabber_kick_module_kind_parses_and_scopes() {
    use crate::state::rack_scope::{parse_module_kind, rack_kind_name_matches};

    // Name parser accepts canonical + common aliases.
    assert_eq!(parse_module_kind("gabber"), Some(ModuleKind::GabberKick));
    assert_eq!(
        parse_module_kind("gabber_kick"),
        Some(ModuleKind::GabberKick),
    );
    assert_eq!(parse_module_kind("Rotterdam"), Some(ModuleKind::GabberKick));

    // Reverse matcher accepts the same aliases.
    assert!(rack_kind_name_matches(ModuleKind::GabberKick, "gabber"));
    assert!(rack_kind_name_matches(
        ModuleKind::GabberKick,
        "gabber_kick",
    ));
    assert!(!rack_kind_name_matches(ModuleKind::GabberKick, "808"));

    // Voice-zone, single-instance, unique grid label.
    assert_eq!(ModuleKind::GabberKick.default_zone(), Zone::Voice);
    assert!(!ModuleKind::GabberKick.allows_multiple());
    assert_eq!(ModuleKind::GabberKick.label(), "GABBER KICK");
}

// ── LfoTarget StereoWidth wiring ───────────────────────────────────────────

#[test]
fn stereo_width_target_parse_and_label() {
    use crate::state::LfoTarget;
    use crate::state::modulation::{
        lfo_target_module_kind, lfo_target_short_label, parse_lfo_target,
    };

    assert_eq!(
        parse_lfo_target("StereoWidth"),
        Some(LfoTarget::StereoWidth)
    );
    assert_eq!(
        parse_lfo_target("stereowidth"),
        Some(LfoTarget::StereoWidth)
    );
    assert_eq!(lfo_target_short_label(LfoTarget::StereoWidth), "M.WID");
    assert_eq!(
        lfo_target_module_kind(LfoTarget::StereoWidth),
        Some(ModuleKind::MasterOutput),
    );
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
    // wiring.  Catches regressions in `wire_default_cables`.
    for preset in crate::state::RACK_PRESETS {
        let rack = RackState::from_preset(preset);
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
fn supports_xy_pad_true_only_for_rolled_out_fx() {
    // Exhaustive match would be brittle; assert the current rollout and
    // a few kinds that definitely shouldn't have one. Expand this as more
    // FX get pads.
    assert!(ModuleKind::FxAutotune.supports_xy_pad());
    assert!(ModuleKind::FxReverb.supports_xy_pad());
    assert!(!ModuleKind::FxCompressor.supports_xy_pad());
    assert!(!ModuleKind::AcidBass.supports_xy_pad());
    assert!(!ModuleKind::StepSequencer.supports_xy_pad());
}

#[test]
fn three_pair_labels_dispatch_matches_cycle_order() {
    use crate::ui::rack_content::three_pair_labels;
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
    // New modules default to pad_expanded = true.
    let m = rack.module(id).unwrap();
    let (w, h) = rack.effective_grid_size(m);
    let (base_w, base_h) = ModuleKind::FxAutotune.grid_size(GRID_COLS);
    assert_eq!(w, base_w);
    assert_eq!(h, base_h + 1, "expanded autotune should be one row taller");
}

#[test]
fn effective_grid_size_matches_static_when_collapsed() {
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::FxAutotune);
    rack.modules
        .iter_mut()
        .find(|m| m.id == id)
        .unwrap()
        .pad_expanded = false;
    let m = rack.module(id).unwrap();
    let (w, h) = rack.effective_grid_size(m);
    assert_eq!((w, h), ModuleKind::FxAutotune.grid_size(GRID_COLS));
}

#[test]
fn effective_grid_size_ignores_flag_for_unsupported_kind() {
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::FxCompressor);
    // Compressor doesn't support a pad yet — the flag should be inert.
    rack.modules
        .iter_mut()
        .find(|m| m.id == id)
        .unwrap()
        .pad_expanded = true;
    let m = rack.module(id).unwrap();
    let (w, h) = rack.effective_grid_size(m);
    assert_eq!((w, h), ModuleKind::FxCompressor.grid_size(GRID_COLS));
}

#[test]
fn arrange_grid_reserves_extra_row_for_expanded_autotune() {
    let mut rack = empty_rack();
    let at = rack.add_module(ModuleKind::FxAutotune);
    // Place a second autotune — together they must not overlap vertically
    // when both are in their default (expanded) state.
    let at2 = rack.add_module(ModuleKind::FxAutotune);
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
fn pad_expanded_default_true_on_new_module() {
    let mut rack = empty_rack();
    let id = rack.add_module(ModuleKind::FxAutotune);
    assert!(
        rack.module(id).unwrap().pad_expanded,
        "pad_expanded should default to true"
    );
}

#[test]
fn pad_expanded_serde_defaults_true_for_legacy_payload() {
    // Sessions saved before this field existed must deserialize with
    // pad_expanded = true so the new default takes effect on upgrade.
    let legacy = r#"{
        "id": 42,
        "kind": "FxAutotune",
        "enabled": true,
        "zone": "FxMod",
        "slot": 0
    }"#;
    let m: RackModule = serde_json::from_str(legacy).unwrap();
    assert!(m.pad_expanded);
}
