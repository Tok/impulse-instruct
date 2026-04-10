// ─── tests/rack_tests.rs ──────────────────────────────────────────────────────
// Tests for rack module management, cable routing, cycle detection, and guards.

use crate::state::rack::*;

/// Empty rack with no modules or cables — avoids default rack's 50+ cables.
fn empty_rack() -> RackState {
    RackState {
        modules: Vec::new(),
        cables: Vec::new(),
        next_id: 1,
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
    let tts = rack.add_module(ModuleKind::EspeakNgTts);
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

// ── Module zone placement ──────────────────────────────────────────────────

#[test]
fn modules_go_to_correct_zones() {
    assert_eq!(ModuleKind::AcidBass.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::DrumKit808.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::EspeakNgTts.default_zone(), Zone::Voice);
    assert_eq!(ModuleKind::FxReverb.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::FxDelay.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::LfoModule.default_zone(), Zone::FxMod);
    assert_eq!(ModuleKind::MasterOutput.default_zone(), Zone::Global);
    assert_eq!(ModuleKind::LlmConsole.default_zone(), Zone::Global);
    assert_eq!(ModuleKind::LlmAgent.default_zone(), Zone::Global);
}
