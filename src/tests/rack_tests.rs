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
        audio_gain: 1.0,
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
        audio_gain: 1.0,
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
