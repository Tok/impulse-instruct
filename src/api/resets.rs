// ─── api/resets.rs ───────────────────────────────────────────────────────────
// `/api/state/reset` (full AppState reset to defaults) and `/api/rack/reset`
// (strip the rack to sequencer + master + console).  Split from api/mod.rs
// to stay under the 1000-line file cap.

use axum::{Json, extract::State as AxumState};

use super::{ApiState, OkResponse, api_log};

/// Full AppState reset — everything back to defaults, Empty rack preset.
/// Preserves the currently-loaded model path so the user doesn't lose it.
/// Intended for demo recording, CI, and automated sessions that need to
/// guarantee a blank slate even when attaching to an already-running app.
pub async fn post_state_reset(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    use crate::state::{AppState, RACK_PRESETS, RackState};
    let mut s = api.app_state.write();
    // Preserve LLM-thread-owned runtime flags.  These are set once by the
    // LLM thread at startup (see src/llm/mod.rs) and never re-asserted.
    // Clobbering them to AppState::default() would flip llm_initializing
    // back to true while the server is actually already live, making
    // wait_for_llm poll forever.
    let model_path = s.llm.model_path.clone();
    let is_mock = s.llm.is_mock;
    let model_missing = s.llm.model_missing;
    let llm_initializing = s.llm.llm_initializing;
    let ui_scale = s.ui_prefs.ui_scale;
    *s = AppState::default();
    s.llm.model_path = model_path;
    s.llm.is_mock = is_mock;
    s.llm.model_missing = model_missing;
    s.llm.llm_initializing = llm_initializing;
    s.ui_prefs.ui_scale = ui_scale;
    s.rack = RackState::from_preset(&RACK_PRESETS[0]);
    drop(s);
    api_log(&api, "[API] state: full reset to defaults");
    Json(OkResponse {
        ok: true,
        message: Some("state reset".into()),
    })
}

/// Clear the rack to a minimal setup: just sequencer + master + LLM console.
pub async fn post_rack_reset(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    use crate::state::ModuleKind;
    let mut s = api.app_state.write();
    // Keep only Sequencer, MasterOutput, LlmConsole
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
    s.llm_agents.clear();
    s.tts_modules.clear();
    drop(s);
    api_log(
        &api,
        "[API] rack: reset to minimal (seq + master + console)",
    );
    Json(OkResponse {
        ok: true,
        message: Some("rack reset".into()),
    })
}
