//! Occasional catalogue-backed MC calls alongside the musical lane pipeline.
use crate::state::{AppState, ConversationMode, ModuleKind, PortKind, TtsModuleState};

/// Called only when explicitly creating/selecting a Jungle setup.
/// Reuse the first producer and TTS module rather than loading another LLM.
pub fn configure(mut state: AppState) -> AppState {
    if state.llm.active_style.as_deref() != Some("jungle") {
        return state;
    }
    let Some(agent) = state.llm_agents.first_mut() else {
        return state;
    };
    agent.conversation_mode = ConversationMode::Mc;
    let agent_id = agent.id;
    let tts_id = state
        .rack
        .modules
        .iter()
        .find(|m| m.kind == ModuleKind::Tts)
        .map(|m| m.id)
        .unwrap_or_else(|| state.rack.add_module(ModuleKind::Tts));
    if !state.tts_modules.iter().any(|t| t.id == tts_id) {
        state.tts_modules.push(TtsModuleState::new(tts_id));
    }
    let tts = state
        .tts_modules
        .iter_mut()
        .find(|t| t.id == tts_id)
        .unwrap();
    if tts.voice_ref == "default" {
        tts.voice_ref = "mc".into();
    }
    tts.pitch_st = -2.0;
    tts.mic_drive = 1.0;
    tts.room_mix = 0.12;
    tts.music_duck = 1.0;
    tts.volume = 0.25;
    state.rack.connect_control(agent_id, tts_id);
    state.rack.wire_default_cables();
    state
}

/// Pure eligibility/selection step. Cooldown is shared by all agents so
/// a multi-agent rack cannot produce a pile-up of simultaneous shouts.
pub fn next_call(
    state: &AppState,
    agent_id: Option<u32>,
    seconds_since_last: Option<f32>,
    index: usize,
) -> Option<(String, TtsModuleState)> {
    if !state.sequencer.running
        || state.llm.active_style.as_deref() != Some("jungle")
        || seconds_since_last.is_some_and(|seconds| seconds < 90.0)
    {
        return None;
    }
    let aid = agent_id?;
    let agent = state.llm_agents.iter().find(|a| a.id == aid)?;
    if agent.sleeping
        || agent.conversation_mode != ConversationMode::Mc
        || agent
            .active_style
            .as_deref()
            .is_some_and(|style| style != "jungle")
        || !state.rack.modules.iter().any(|m| m.id == aid && m.enabled)
    {
        return None;
    }
    let tts = state
        .rack
        .cables
        .iter()
        .filter(|c| c.from.module_id == aid && c.from.kind == PortKind::Control)
        .find_map(|c| {
            state
                .rack
                .modules
                .iter()
                .find(|m| m.id == c.to.module_id && m.kind == ModuleKind::Tts && m.enabled)
                .and_then(|m| {
                    state
                        .tts_modules
                        .iter()
                        .find(|t| t.id == m.id && t.volume > 0.0)
                })
        })?;
    let lines: Vec<_> = crate::state::effective_mc_lines(state, "jungle")
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some((lines[index % lines.len()].clone(), tts.clone()))
}
