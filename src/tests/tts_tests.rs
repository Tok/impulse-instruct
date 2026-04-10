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
        .filter(|m| matches!(m.kind, ModuleKind::EspeakNgTts | ModuleKind::CoquiTts))
        .collect();
    // Default rack has an EspeakNgTts module
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
    assert_eq!(tts.engine, TtsEngine::EspeakNg);
    assert_eq!(tts.pitch, 0);
    assert_eq!(tts.speed, 0);
    assert_eq!(tts.amplitude, 0);
    assert_eq!(tts.voice_char, McVoiceChar::Auto);
    assert!(!tts.randomise);
    assert!(!tts.pitch_snap);
}

#[test]
fn coqui_tts_module_sets_engine() {
    let mut s = AppState::default();
    let id = s.rack.add_module(ModuleKind::CoquiTts);
    let mut tts = TtsModuleState::new(id);
    tts.engine = TtsEngine::CoquiTts;
    s.tts_modules.push(tts);
    let found = s.tts_modules.iter().find(|t| t.id == id).unwrap();
    assert_eq!(found.engine, TtsEngine::CoquiTts);
}

#[test]
fn tts_module_removed_with_rack_module() {
    let mut s = AppState::default();
    let id = s.rack.add_module(ModuleKind::EspeakNgTts);
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
    let id = s.rack.add_module(ModuleKind::EspeakNgTts);
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
    let tts_id = s.rack.add_module(ModuleKind::EspeakNgTts);
    let mut tts_state = TtsModuleState::new(tts_id);
    tts_state.pitch = 50;
    s.tts_modules.push(tts_state);
    // Wire agent → TTS via control cable
    s.rack.connect_control(agent_id, tts_id);
    // Find TTS module via cable routing (same logic as llm/mod.rs)
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack.modules.iter().any(|m| {
                m.id == target
                    && matches!(m.kind, ModuleKind::EspeakNgTts | ModuleKind::CoquiTts)
                    && m.enabled
            }) {
                s.tts_modules.iter().find(|t| t.id == target)
            } else {
                None
            }
        } else {
            None
        }
    });
    assert!(found.is_some(), "should find TTS module via cable");
    assert_eq!(found.unwrap().pitch, 50);
}

#[test]
fn agent_without_tts_cable_finds_nothing() {
    let mut s = AppState::default();
    let agent_id = s.rack.add_module(ModuleKind::LlmAgent);
    let tts_id = s.rack.add_module(ModuleKind::EspeakNgTts);
    s.tts_modules.push(TtsModuleState::new(tts_id));
    // NO cable wired — agent should not find TTS
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack.modules.iter().any(|m| {
                m.id == target && matches!(m.kind, ModuleKind::EspeakNgTts | ModuleKind::CoquiTts)
            }) {
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
    let tts_id = s.rack.add_module(ModuleKind::EspeakNgTts);
    s.tts_modules.push(TtsModuleState::new(tts_id));
    s.rack.connect_control(agent_id, tts_id);
    // Disable the TTS module
    if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == tts_id) {
        m.enabled = false;
    }
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack.modules.iter().any(|m| {
                m.id == target
                    && matches!(m.kind, ModuleKind::EspeakNgTts | ModuleKind::CoquiTts)
                    && m.enabled
            }) {
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
    let tts1 = s.rack.add_module(ModuleKind::EspeakNgTts);
    let tts2 = s.rack.add_module(ModuleKind::CoquiTts);
    let mut state1 = TtsModuleState::new(tts1);
    state1.pitch = 10;
    let mut state2 = TtsModuleState::new(tts2);
    state2.pitch = 90;
    state2.engine = TtsEngine::CoquiTts;
    s.tts_modules.push(state1);
    s.tts_modules.push(state2);
    // Wire agent to tts2 only
    s.rack.connect_control(agent_id, tts2);
    let found = s.rack.cables.iter().find_map(|c| {
        if c.from.module_id == agent_id && c.from.kind == PortKind::Control {
            let target = c.to.module_id;
            if s.rack.modules.iter().any(|m| {
                m.id == target
                    && matches!(m.kind, ModuleKind::EspeakNgTts | ModuleKind::CoquiTts)
                    && m.enabled
            }) {
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
    assert_eq!(tts.pitch, 90);
    assert_eq!(tts.engine, TtsEngine::CoquiTts);
}

#[test]
fn tts_module_state_serializes() {
    let tts = TtsModuleState::new(7);
    let json = serde_json::to_string(&tts).unwrap();
    let back: TtsModuleState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.id, 7);
    assert_eq!(back.engine, TtsEngine::EspeakNg);
}
