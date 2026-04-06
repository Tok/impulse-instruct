// ─── state/persistence.rs ────────────────────────────────────────────────────
// Project save / load — pure I/O, no state mutation.

use super::AppState;
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
    // UI prefs subset
    pub knob_style: Option<crate::state::KnobStyle>,
    pub knob_size: Option<crate::state::KnobSize>,
    pub pad_size: Option<crate::state::PadSize>,
    pub use_sliders: Option<bool>,
    pub ui_scale: Option<f32>,
    pub log_level_idx: Option<usize>,
    #[serde(default)]
    pub autosave_interval: Option<crate::state::AutosaveInterval>,
}

/// Save session data extracted from current AppState.
pub fn save_session(state: &AppState, show_cables: bool) {
    let data = SessionData {
        rack: Some(state.rack.clone()),
        active_style: state.llm.active_style.clone(),
        custom_style_text: Some(state.llm.custom_style_text.clone()),
        user_instructions: Some(state.llm.user_instructions.clone()),
        persona_name: Some(state.llm.persona_name.clone()),
        model_path: Some(state.llm.model_path.clone()),
        show_cables: Some(show_cables),
        knob_style: Some(state.ui_prefs.knob_style),
        knob_size: Some(state.ui_prefs.knob_size),
        pad_size: Some(state.ui_prefs.pad_size),
        use_sliders: Some(state.ui_prefs.use_sliders),
        ui_scale: Some(state.ui_prefs.ui_scale),
        log_level_idx: Some(state.ui_prefs.log_level_idx),
        autosave_interval: Some(state.ui_prefs.autosave_interval),
    };
    match serde_json::to_string_pretty(&data) {
        Ok(json) => {
            if let Err(e) = std::fs::write(SESSION_PATH, json) {
                log::warn!("Could not write {SESSION_PATH}: {e}");
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
    if let Some(rack) = data.rack {
        state.rack = rack;
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
    if let Some(v) = data.knob_style {
        state.ui_prefs.knob_style = v;
    }
    if let Some(v) = data.knob_size {
        state.ui_prefs.knob_size = v;
    }
    if let Some(v) = data.pad_size {
        state.ui_prefs.pad_size = v;
    }
    if let Some(v) = data.use_sliders {
        state.ui_prefs.use_sliders = v;
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
