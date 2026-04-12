// ─── state/persistence.rs ────────────────────────────────────────────────────
// Project save / load — pure I/O, no state mutation.

use super::{AppState, ModuleKind};
use serde::{Deserialize, Serialize};

const SETTINGS_PATH: &str = "settings.json";
const SESSION_PATH: &str = "session.json";

/// Write the model path to `settings.json` so it persists across restarts.
pub fn save_model_setting(model_path: &str) {
    let json = format!(
        "{{\"model_path\":{}}}\n",
        serde_json::to_string(model_path).unwrap_or_default()
    );
    if let Err(e) = std::fs::write(SETTINGS_PATH, json) {
        log::warn!("Could not write {SETTINGS_PATH}: {e}");
    }
}

/// Read the saved model path from `settings.json`, if it exists and is valid.
pub fn load_model_setting() -> Option<String> {
    let json = std::fs::read_to_string(SETTINGS_PATH).ok()?;
    let v: serde_json::Value = serde_json::from_str(&json).ok()?;
    let path = v.get("model_path")?.as_str()?.to_string();
    if std::path::Path::new(&path).exists() {
        Some(path)
    } else {
        None
    }
}

// ─── Session persistence (auto-save on change) ───────────────────────────────

/// Subset of AppState that is auto-saved to `session.json`.
/// Survives restarts without cluttering project files.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SessionData {
    pub rack: Option<crate::state::RackState>,
    pub active_style: Option<String>,
    pub custom_style_text: Option<String>,
    pub user_instructions: Option<String>,
    pub persona_name: Option<String>,
    pub model_path: Option<String>,
    pub show_cables: Option<bool>,
    #[serde(default)]
    pub rack_flipped: Option<bool>,
    // UI prefs subset
    pub ui_scale: Option<f32>,
    pub log_level_idx: Option<usize>,
    #[serde(default)]
    pub autosave_interval: Option<crate::state::AutosaveInterval>,
    #[serde(default)]
    pub llm_agents: Option<Vec<super::LlmAgentState>>,
    #[serde(default)]
    pub wasd_as_arrows: Option<bool>,
    #[serde(default)]
    pub enable_thinking: Option<bool>,
    #[serde(default)]
    pub show_thinking_in_log: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub wizard_done: Option<bool>,
    #[serde(default)]
    pub module_scales: Option<std::collections::HashMap<crate::state::ModuleKind, f32>>,
}

/// Save session data extracted from current AppState.
pub fn save_session(state: &AppState, show_cables: bool, rack_flipped: bool) {
    save_session_ext(state, show_cables, rack_flipped, None)
}

/// Extended save with optional wizard_done flag.
pub fn save_session_ext(
    state: &AppState,
    show_cables: bool,
    rack_flipped: bool,
    wizard_done: Option<bool>,
) {
    let data = SessionData {
        rack: Some(state.rack.clone()),
        active_style: state.llm.active_style.clone(),
        custom_style_text: Some(state.llm.custom_style_text.clone()),
        user_instructions: Some(state.llm.user_instructions.clone()),
        persona_name: Some(state.llm.persona_name.clone()),
        model_path: Some(state.llm.model_path.clone()),
        show_cables: Some(show_cables),
        rack_flipped: Some(rack_flipped),
        llm_agents: Some(state.llm_agents.clone()),
        ui_scale: Some(state.ui_prefs.ui_scale),
        log_level_idx: Some(state.ui_prefs.log_level_idx),
        autosave_interval: Some(state.ui_prefs.autosave_interval),
        wasd_as_arrows: Some(state.ui_prefs.wasd_as_arrows),
        enable_thinking: Some(state.llm.enable_thinking),
        show_thinking_in_log: Some(state.llm.show_thinking_in_log),
        temperature: Some(state.llm.temperature),
        wizard_done,
        module_scales: None, // populated by caller when available
    };
    match serde_json::to_string_pretty(&data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(SESSION_PATH, &json) {
                log::warn!("Could not write {SESSION_PATH}: {e}");
            } else {
                log::debug!("Session saved ({} bytes)", json.len());
            }
        }
        Err(e) => log::warn!("Could not serialise session: {e}"),
    }
}

/// Load session data from `session.json` if it exists.
pub fn load_session() -> Option<SessionData> {
    let json = std::fs::read_to_string(SESSION_PATH).ok()?;
    serde_json::from_str(&json).ok()
}

/// Apply loaded session data onto a mutable AppState.
pub fn apply_session(state: &mut AppState, data: SessionData) {
    if let Some(mut rack) = data.rack {
        let removed = rack.strip_audio_cycles();
        if removed > 0 {
            log::warn!("Session: removed {removed} cyclic audio cable(s) from rack");
        }
        // Migrate old sessions: auto-wire if modules exist but no cables.
        if rack.cables.is_empty() && rack.modules.len() > 1 {
            log::info!("Session: no cables found — auto-wiring default routing");
            rack.wire_default_cables();
        }
        // Migrate pre-v0.7.2 sessions: re-apply default zones (LLM console/agent
        // moved from Zone::Global to Zone::Ai). Force re-layout so modules land
        // in their new zones.
        let mut zones_changed = false;
        for m in rack.modules.iter_mut() {
            let z = m.kind.default_zone();
            if m.zone != z {
                m.zone = z;
                zones_changed = true;
            }
        }
        if zones_changed {
            log::info!("Session: migrated module zones (AI split out from Global)");
            rack.arrange_grid();
        }
        // Migrate old sessions: if all modules are at (0,0), run grid placement.
        let needs_grid = rack
            .modules
            .iter()
            .all(|m| m.grid_col == 0 && m.grid_row == 0)
            && rack.modules.len() > 1;
        state.rack = rack;
        if needs_grid {
            state.rack.arrange_grid();
        }
    }
    if let Some(v) = data.active_style {
        state.llm.active_style = Some(v);
    }
    if let Some(v) = data.custom_style_text {
        state.llm.custom_style_text = v;
    }
    if let Some(v) = data.user_instructions {
        state.llm.user_instructions = v;
    }
    if let Some(v) = data.persona_name {
        state.llm.persona_name = v;
    }
    if let Some(v) = data.model_path
        && !v.is_empty()
    {
        state.llm.model_path = v;
    }
    if let Some(v) = data.ui_scale {
        state.ui_prefs.ui_scale = v.clamp(0.5, 3.0);
    }
    if let Some(v) = data.log_level_idx {
        state.ui_prefs.log_level_idx = v;
    }
    if let Some(v) = data.autosave_interval {
        state.ui_prefs.autosave_interval = v;
    }
    if let Some(agents) = data.llm_agents
        && !agents.is_empty()
    {
        state.llm_agents = agents;
    }
    if let Some(v) = data.wasd_as_arrows {
        state.ui_prefs.wasd_as_arrows = v;
    }
    if let Some(v) = data.enable_thinking {
        state.llm.enable_thinking = v;
    }
    if let Some(v) = data.show_thinking_in_log {
        state.llm.show_thinking_in_log = v;
    }
    if let Some(v) = data.temperature {
        state.llm.temperature = v;
    }

    // ── Migration: ensure LlmConsole + at least one LlmAgent exist ───────
    // Migration: add modules that were introduced in later versions
    for kind in [
        ModuleKind::LlmConsole,
        ModuleKind::NoiseVoice,
        ModuleKind::GranularTexture,
    ] {
        if !state.rack.modules.iter().any(|m| m.kind == kind) {
            let id = state.rack.add_module(kind);
            log::info!("Migration: added {:?} module to rack", kind);
            // Wire voice modules to MasterOutput (audio) like the default rack
            if matches!(kind, ModuleKind::NoiseVoice | ModuleKind::GranularTexture)
                && let Some(master_id) = state
                    .rack
                    .modules
                    .iter()
                    .find(|m| m.kind == ModuleKind::MasterOutput)
                    .map(|m| m.id)
            {
                state.rack.connect(
                    super::PortRef {
                        module_id: id,
                        dir: super::PortDir::Out,
                        kind: super::PortKind::Audio,
                        index: 0,
                    },
                    super::PortRef {
                        module_id: master_id,
                        dir: super::PortDir::In,
                        kind: super::PortKind::Audio,
                        index: 0,
                    },
                );
                log::info!("Migration: wired {:?} → MasterOutput", kind);
            }
        }
    }
    if !state
        .rack
        .modules
        .iter()
        .any(|m| m.kind == ModuleKind::LlmAgent)
    {
        let id = state.rack.add_module(ModuleKind::LlmAgent);
        let agent = super::LlmAgentState::from_singleton(id, &state.llm);
        state.llm_agents.push(agent);
        // Wire control cables to all controllable modules
        let targets: Vec<u32> = state
            .rack
            .modules
            .iter()
            .filter(|m| {
                !matches!(
                    m.kind,
                    ModuleKind::MasterOutput | ModuleKind::LlmAgent | ModuleKind::LlmConsole
                )
            })
            .map(|m| m.id)
            .collect();
        for tid in &targets {
            state.rack.connect_control(id, *tid);
        }
        log::info!("Migration: added default LlmAgent + control cables");
    }

    // Ensure existing agents have control cables (migration for sessions that
    // were saved before control cables existed).
    let agent_ids: Vec<u32> = state
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::LlmAgent)
        .map(|m| m.id)
        .collect();
    for aid in &agent_ids {
        let has_any_control = state
            .rack
            .cables
            .iter()
            .any(|c| c.from.module_id == *aid && c.from.kind == super::PortKind::Control);
        if !has_any_control {
            let targets: Vec<u32> = state
                .rack
                .modules
                .iter()
                .filter(|m| {
                    !matches!(
                        m.kind,
                        ModuleKind::MasterOutput | ModuleKind::LlmAgent | ModuleKind::LlmConsole
                    )
                })
                .map(|m| m.id)
                .collect();
            for tid in &targets {
                state.rack.connect_control(*aid, *tid);
            }
            log::info!("Migration: wired control cables for agent {}", aid);
        }
    }
}

/// Serialise `state` to `project-<unix_seconds>.json` in the current directory.
/// Returns the path written on success.
pub fn save_project(state: &AppState) -> Result<std::path::PathBuf, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = std::path::PathBuf::from(format!("project-{}.json", ts));
    let json = serde_json::to_string_pretty(state).map_err(|e| format!("serialise error: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write error: {e}"))?;
    Ok(path)
}

/// Load an `AppState` from a JSON project file.
pub fn load_project(path: &std::path::Path) -> Result<AppState, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("parse error: {e}"))
}
