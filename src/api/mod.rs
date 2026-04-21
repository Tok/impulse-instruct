// ─── api/mod.rs ──────────────────────────────────────────────────────────────
// HTTP API — MCP-compatible REST interface.
// Runs on its own tokio runtime in a separate OS thread.

use axum::{
    Router,
    extract::State as AxumState,
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use crossbeam_channel::Sender;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::llm::LlmInput;
use crate::state::{AppState, apply_llm_update, lock_params, unlock_params};

// ─── Shared API state ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiState {
    pub app_state: Arc<RwLock<AppState>>,
    pub llm_tx: Sender<LlmInput>,
    /// Lock-free channel for API→UI log messages.
    /// Avoids taking a write lock on AppState just to push log strings.
    pub api_log_tx: Sender<String>,
    /// Set by API when params change — UI polls this and pushes to audio thread.
    pub params_dirty: Arc<std::sync::atomic::AtomicBool>,
}

// ─── Request / response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
    /// When `true` (default), the prompt fires once and stops.  When
    /// `false`, the LLM worker sends `[jam_cycle_done]` after the
    /// pipeline so the UI's jam loop picks it up and re-fires the next
    /// agent (requires `llm.heat > 0.0` for re-fire to schedule).  The
    /// pipeline writeback is surgical — only voice/FX fields land in
    /// shared state, so jam-via-API doesn't clobber user-owned rack /
    /// preference state.
    #[serde(default = "default_one_shot")]
    pub one_shot: bool,
    /// Target agent by persona name (e.g. "BASS", "DRUMS"). Omit for global/first agent.
    #[serde(default)]
    pub agent: Option<String>,
}

fn default_one_shot() -> bool {
    true
}

#[derive(Deserialize)]
pub struct ParamsRequest {
    pub params: serde_json::Value,
}

#[derive(Deserialize)]
pub struct LockRequest {
    pub paths: Vec<String>,
}

#[derive(Deserialize)]
pub struct ScrollRequest {
    /// Target to scroll to: zone name ("global", "voice", "fxmod") or
    /// module kind ("AcidBass", "DrumKit808", "FxReverb", etc.)
    pub target: String,
    /// Optional: collapse other zones when scrolling to focus on the target.
    #[serde(default)]
    pub collapse_others: bool,
}

#[derive(Deserialize)]
pub struct CollapseRequest {
    /// "all", "none", "global", "voice", "fxmod"
    pub action: String,
}

#[derive(Deserialize)]
pub struct PresetRequest {
    /// Preset name: "Solo", "Duo", "Swarm", "Crew", "Voices", "Lite"
    pub name: String,
}

mod rack;
pub use rack::{
    RackAddRequest, RackAgentRequest, RackCableGainRequest, RackCableRequest, RackRemoveRequest,
    post_rack_add, post_rack_agent, post_rack_cable, post_rack_cable_gain, post_rack_remove,
};

mod instrument;
pub use instrument::{
    AmenRequest, FlipRequest, GranularRequest, StyleRequest, post_amen, post_flip, post_granular,
    post_randomize, post_style,
};

mod rack_mod;
pub use rack_mod::{
    RackModCableRequest, RackModDepthRequest, RackModTargetRequest, post_rack_mod_cable,
    post_rack_mod_depth, post_rack_mod_target,
};

mod rack_pad;
pub use rack_pad::{RackPadRequest, post_rack_pad};

mod preset;
pub use preset::post_preset;

mod song;
pub use song::{SongRequest, SongResponse, get_song, post_song};

mod resets;
pub use resets::{post_rack_reset, post_state_reset};

mod midi_export;
pub use midi_export::{MidiExportRequest, post_midi_export};

mod midi_import;
pub use midi_import::{MidiImportRequest, MidiImportResponse, post_midi_import};

mod ui_prefs_api;
pub use ui_prefs_api::post_ui_prefs;

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Clone the app state without holding the read lock longer than necessary.
fn snapshot(state: &Arc<RwLock<AppState>>) -> AppState {
    state.read().clone()
}

/// Push a log message to the UI console + stderr log.
fn api_log(api: &ApiState, msg: impl Into<String>) {
    let s = msg.into();
    log::debug!("{}", s);
    let _ = api.api_log_tx.try_send(s);
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

async fn get_state(AxumState(api): AxumState<ApiState>) -> Json<serde_json::Value> {
    let state = snapshot(&api.app_state);
    Json(serde_json::to_value(&state).unwrap_or(serde_json::Value::Null))
}

async fn get_schema() -> Json<serde_json::Value> {
    Json(crate::llm::param_json_schema())
}

async fn post_prompt(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<PromptRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    // Resolve agent by persona name (case-insensitive).
    let agent_id = req.agent.as_ref().and_then(|name| {
        let s = api.app_state.read();
        s.llm_agents
            .iter()
            .find(|a| a.persona_name.eq_ignore_ascii_case(name))
            .map(|a| a.id)
    });
    let target = agent_id
        .and_then(|id| {
            api.app_state
                .read()
                .llm_agents
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.persona_name.clone())
        })
        .unwrap_or_else(|| "global".into());
    let mode = if req.one_shot { "one-shot" } else { "jam" };
    api_log(
        &api,
        format!("[API] prompt ({}) → {}: {}", mode, target, req.prompt),
    );
    api.llm_tx
        .try_send(LlmInput::Infer {
            prompt: req.prompt,
            one_shot: req.one_shot,
            agent_id,
        })
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

async fn post_params(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<ParamsRequest>,
) -> Json<OkResponse> {
    // Snapshot first, then release read lock before acquiring write lock.
    let current = snapshot(&api.app_state);
    let next = apply_llm_update(current, &req.params, &[]);
    let keys: Vec<&str> = req
        .params
        .as_object()
        .map(|o| o.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    api_log(&api, format!("[API] params: {}", keys.join(", ")));
    *api.app_state.write() = next;
    api.params_dirty
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Json(OkResponse {
        ok: true,
        message: Some("params updated".into()),
    })
}

async fn post_lock(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<LockRequest>,
) -> Json<OkResponse> {
    let refs: Vec<&str> = req.paths.iter().map(String::as_str).collect();
    let current = snapshot(&api.app_state);
    let next = lock_params(current, &refs);
    api_log(&api, format!("[API] locked: {}", req.paths.join(", ")));
    *api.app_state.write() = next;
    Json(OkResponse {
        ok: true,
        message: Some(format!("locked {} params", req.paths.len())),
    })
}

async fn post_unlock(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<LockRequest>,
) -> Json<OkResponse> {
    let refs: Vec<&str> = req.paths.iter().map(String::as_str).collect();
    let current = snapshot(&api.app_state);
    let next = unlock_params(current, &refs);
    api_log(&api, format!("[API] unlocked: {}", req.paths.join(", ")));
    *api.app_state.write() = next;
    Json(OkResponse {
        ok: true,
        message: Some(format!("unlocked {} params", req.paths.len())),
    })
}

async fn post_play(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    api.app_state.write().sequencer.running = true;
    api.params_dirty
        .store(true, std::sync::atomic::Ordering::Relaxed);
    api_log(&api, "[API] sequencer: play");
    Json(OkResponse {
        ok: true,
        message: None,
    })
}

async fn post_stop(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    api.app_state.write().sequencer.running = false;
    api.params_dirty
        .store(true, std::sync::atomic::Ordering::Relaxed);
    api_log(&api, "[API] sequencer: stop");
    Json(OkResponse {
        ok: true,
        message: None,
    })
}

async fn post_scroll(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<ScrollRequest>,
) -> Json<OkResponse> {
    if req.collapse_others {
        let is_ai = req.target == "ai" || req.target == "console" || req.target == "agent";
        let is_global = req.target == "global"
            || req.target == "main"
            || req.target == "mainaudio"
            || req.target == "sequencer";
        let is_voice = req.target == "voice"
            || req.target == "bass"
            || req.target == "808"
            || req.target == "909";
        let is_fx = req.target == "fxmod" || req.target == "fx";
        api.app_state.write().collapse_requested = Some((!is_ai, !is_global, !is_voice, !is_fx));
    }
    api.app_state.write().scroll_target = Some(req.target.clone());
    api_log(&api, format!("[API] scroll → {}", req.target));
    Json(OkResponse {
        ok: true,
        message: Some(format!("scrolling to {}", req.target)),
    })
}

async fn post_collapse(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<CollapseRequest>,
) -> Json<OkResponse> {
    // Tuple order: (ai, global, voice, fxmod) → each bool is "collapsed?".
    // Action "X" collapses only zone X (leaving the others expanded), mirroring
    // the pre-split behaviour of /api/rack/collapse { "action": "global" }.
    let collapse = match req.action.as_str() {
        "all" => Some((true, true, true, true)),
        "none" => Some((false, false, false, false)),
        "ai" => Some((true, false, false, false)),
        "global" | "main" | "mainaudio" => Some((false, true, false, false)),
        "voice" => Some((false, false, true, false)),
        "fxmod" => Some((false, false, false, true)),
        _ => None,
    };
    if let Some(c) = collapse {
        api.app_state.write().collapse_requested = Some(c);
    }
    api_log(&api, format!("[API] collapse → {}", req.action));
    Json(OkResponse {
        ok: true,
        message: Some(format!("collapse {}", req.action)),
    })
}

// ─── Server entry point ───────────────────────────────────────────────────────

pub fn build_router(api_state: ApiState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/state", get(get_state))
        .route("/api/schema", get(get_schema))
        .route("/api/prompt", post(post_prompt))
        .route("/api/params", post(post_params))
        .route("/api/lock", post(post_lock))
        .route("/api/unlock", post(post_unlock))
        .route("/api/sequencer/play", post(post_play))
        .route("/api/sequencer/stop", post(post_stop))
        .route("/api/song", get(get_song).post(post_song))
        .route("/api/scroll", post(post_scroll))
        .route("/api/preset", post(post_preset))
        .route("/api/style", post(post_style))
        .route("/api/randomize", post(post_randomize))
        .route("/api/amen", post(post_amen))
        .route("/api/granular", post(post_granular))
        .route("/api/flip", post(post_flip))
        .route("/api/rack/add", post(post_rack_add))
        .route("/api/rack/agent", post(post_rack_agent))
        .route("/api/rack/cable", post(post_rack_cable))
        .route("/api/rack/cable_gain", post(post_rack_cable_gain))
        .route("/api/rack/mod_cable", post(post_rack_mod_cable))
        .route("/api/rack/mod_target", post(post_rack_mod_target))
        .route("/api/rack/mod_depth", post(post_rack_mod_depth))
        .route("/api/rack/remove", post(post_rack_remove))
        .route("/api/rack/pad", post(post_rack_pad))
        .route("/api/rack/reset", post(post_rack_reset))
        .route("/api/state/reset", post(post_state_reset))
        .route("/api/rack/collapse", post(post_collapse))
        .route("/api/midi/export", post(post_midi_export))
        .route("/api/midi/import", post(post_midi_import))
        .route("/api/ui_prefs", post(post_ui_prefs))
        .layer(cors)
        .with_state(api_state)
}

pub async fn run_server(api_state: ApiState, port: u16) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log::info!("HTTP API listening on http://{}", addr);

    axum::serve(listener, build_router(api_state)).await?;
    Ok(())
}
