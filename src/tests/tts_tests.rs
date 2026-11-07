// ─── tests/tts_tests.rs ───────────────────────────────────────────────────────
// Tests for TTS module lifecycle, cable routing, and per-module state.

use crate::state::*;

#[test]
fn default_rack_creates_tts_module_state() {
    let s = AppState::default();
    let tts_modules: Vec<_> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::Tts)
        .collect();
    // Default rack has a Tts module
    assert!(
        !tts_modules.is_empty(),
        "default rack should have a TTS module"
    );
    // Each TTS rack module should have a corresponding TtsModuleState
    for m in &tts_modules {
        assert!(
            s.tts_modules.iter().any(|t| t.id == m.id),
            "TTS module {} should have a TtsModuleState",
            m.id
        );
    }
}

#[test]
fn tts_module_state_defaults() {
    let tts = TtsModuleState::new(42);
    assert_eq!(tts.id, 42);
    assert_eq!(tts.voice_ref, "default");
    assert!((tts.temperature - 0.8).abs() < 0.01);
    assert_eq!(tts.top_k, 30);
    assert!((tts.top_p - 0.9).abs() < 0.01);
    assert!(!tts.pitch_snap);
}

#[test]
fn tts_module_removed_with_rack_module() {
    let mut s = AppState::default();
    let id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(id));
    assert!(s.tts_modules.iter().any(|t| t.id == id));
    // Simulate removal (as done in API)
    s.rack.remove_module(id);
    s.tts_modules.retain(|t| t.id != id);
    assert!(!s.tts_modules.iter().any(|t| t.id == id));
}

#[test]
fn tts_modules_cleared_on_rack_reset() {
    let mut s = AppState::default();
    let id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(id));
    assert!(!s.tts_modules.is_empty());
    // Simulate rack reset (keep only core modules)
    let keep: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                ModuleKind::StepSequencer | ModuleKind::MasterOutput | ModuleKind::LlmConsole
            )
        })
        .map(|m| m.id)
        .collect();
    s.rack.modules.retain(|m| keep.contains(&m.id));
    s.rack
        .cables
        .retain(|c| keep.contains(&c.from.module_id) && keep.contains(&c.to.module_id));
    s.tts_modules.clear();
    assert!(s.tts_modules.is_empty());
}

#[test]
fn agent_finds_tts_via_control_cable() {
    let mut s = AppState::default();
    // Add an agent
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    // Add a TTS module
    let tts_id = s.rack.add_module(ModuleKind::Tts);
    let mut tts_state = TtsModuleState::new(tts_id);
    tts_state.temperature = 1.2;
    s.tts_modules.push(tts_state);
    // Wire agent -> TTS via control cable
    s.rack.connect_control(agent_id, tts_id);
    // Find TTS module via cable routing (same logic as llm/mod.rs)
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts && m.enabled)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_some(), "should find TTS module via cable");
    assert!((found.unwrap().temperature - 1.2).abs() < 0.01);
}

#[test]
fn agent_without_tts_cable_finds_nothing() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts_id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(tts_id));
    // NO cable wired — agent should not find TTS
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_none(), "no cable = no TTS");
}

#[test]
fn disabled_tts_module_not_found() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts_id = s.rack.add_module(ModuleKind::Tts);
    s.tts_modules.push(TtsModuleState::new(tts_id));
    s.rack.connect_control(agent_id, tts_id);
    // Disable the TTS module
    if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == tts_id) {
        m.enabled = false;
    }
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts && m.enabled)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_none(), "disabled TTS module should not be found");
}

#[test]
fn multiple_tts_modules_agent_finds_wired_one() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts1 = s.rack.add_module(ModuleKind::Tts);
    let tts2 = s.rack.add_module(ModuleKind::Tts);
    let mut state1 = TtsModuleState::new(tts1);
    state1.temperature = 0.5;
    let mut state2 = TtsModuleState::new(tts2);
    state2.temperature = 1.5;
    s.tts_modules.push(state1);
    s.tts_modules.push(state2);
    // Wire agent to tts2 only
    s.rack.connect_control(agent_id, tts2);
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack
                .modules
                .iter()
                .any(|m| m.id == target && m.kind == ModuleKind::Tts && m.enabled)
            {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_some());
    let tts = found.unwrap();
    assert_eq!(tts.id, tts2);
    assert!((tts.temperature - 1.5).abs() < 0.01);
}

#[test]
fn tts_module_state_serializes() {
    let tts = TtsModuleState::new(7);
    let json = serde_json::to_string(&tts).unwrap();
    let back: TtsModuleState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, 7);
    assert_eq!(back.voice_ref, "default");
}

// ── Engine switch NeuTTS → Qwen3-TTS (2026-11) ─────────────────────────────
// Sessions and projects saved before the switch serialise the module kind as
// "NeuTts" and the LFO target as "NeuTtsVolume"; both must still load.

#[test]
fn legacy_neutts_module_kind_deserialises_to_tts() {
    let k: ModuleKind = serde_json::from_str("\"NeuTts\"").unwrap();
    assert_eq!(k, ModuleKind::Tts);
    // Older engines' aliases still hold too.
    let k: ModuleKind = serde_json::from_str("\"CoquiTts\"").unwrap();
    assert_eq!(k, ModuleKind::Tts);
    // New saves use the engine-neutral name.
    assert_eq!(serde_json::to_string(&ModuleKind::Tts).unwrap(), "\"Tts\"");
}

#[test]
fn legacy_neutts_volume_lfo_target_deserialises() {
    use crate::state::LfoTarget;
    let t: LfoTarget = serde_json::from_str("\"NeuTtsVolume\"").unwrap();
    assert_eq!(t, LfoTarget::TtsVolume);
}

#[test]
fn sanitize_tts_text_keeps_unicode_drops_control_and_caps_length() {
    use crate::llm::tts::sanitize_tts_text;
    // Non-ASCII survives (the old filter stripped umlauts to "Der Bass ist zurck").
    assert_eq!(
        sanitize_tts_text("Der Bass ist zurück!"),
        "Der Bass ist zurück!"
    );
    // Newlines/tabs become spaces; other control chars vanish; edges trimmed.
    assert_eq!(sanitize_tts_text("\n drop\tit\u{7}  "), "drop it");
    // 200-character cap counts chars, not bytes.
    let long: String = "ü".repeat(500);
    assert_eq!(sanitize_tts_text(&long).chars().count(), 200);
    assert_eq!(sanitize_tts_text(" \n\t "), "");
}

/// Live round-trip through `speak_tts` → scripts/tts-server.py → WAV decode →
/// ring buffer.  Needs the server running on TTS_PORT:
///   .tts-venv/bin/python scripts/tts-server.py &
///   cargo test tts_server_roundtrip -- --ignored
#[test]
#[ignore = "needs a running scripts/tts-server.py on TTS_PORT"]
fn tts_server_roundtrip_pushes_samples() {
    use crate::audio::TtsSink;
    use parking_lot::Mutex;
    use std::sync::Arc;
    let (tx, mut rx) = rtrb::RingBuffer::<f32>::new(48_000 * 30);
    let sink = TtsSink {
        tx: Arc::new(Mutex::new(tx)),
        target_sr: 48_000,
    };
    let mut tts = TtsModuleState::new(1);
    tts.voice_ref = "mc".to_string();
    // Non-ASCII on purpose: the pre-Qwen3 filter stripped it.
    crate::llm::tts::speak_tts("Der Bass ist zurück, Selecta!", &tts, &sink);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut got = 0usize;
    while std::time::Instant::now() < deadline {
        while rx.pop().is_ok() {
            got += 1;
        }
        if got > 48_000 {
            break; // > 1 s of audio at the device rate
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        got > 48_000,
        "only {got} samples arrived from the TTS server"
    );
}
