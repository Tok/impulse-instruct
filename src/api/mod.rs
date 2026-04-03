// ─── api/mod.rs ──────────────────────────────────────────────────────────────
// HTTP API — MCP-compatible REST interface.
// Runs on its own tokio runtime in a separate OS thread.

use axum::{
    extract::State as AxumState,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
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
}

// ─── Request / response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
    #[serde(default)]
    pub one_shot: bool,
}

#[derive(Deserialize)]
pub struct ParamsRequest {
    pub params: serde_json::Value,
}

#[derive(Deserialize)]
pub struct LockRequest {
    pub paths: Vec<String>,
}

#[derive(Serialize)]
pub struct OkResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

async fn get_state(AxumState(api): AxumState<ApiState>) -> Json<serde_json::Value> {
    let state = api.app_state.read().clone();
    Json(serde_json::to_value(&state).unwrap_or(serde_json::Value::Null))
}

async fn get_schema() -> Json<serde_json::Value> {
    Json(crate::llm::param_json_schema())
}

async fn post_prompt(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<PromptRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    api.llm_tx
        .try_send(LlmInput::Infer { prompt: req.prompt, one_shot: req.one_shot })
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(Json(OkResponse { ok: true, message: None }))
}

async fn post_params(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<ParamsRequest>,
) -> Json<OkResponse> {
    let next = apply_llm_update(api.app_state.read().clone(), &req.params);
    *api.app_state.write() = next;
    Json(OkResponse { ok: true, message: Some("params updated".into()) })
}

async fn post_lock(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<LockRequest>,
) -> Json<OkResponse> {
    let refs: Vec<&str> = req.paths.iter().map(String::as_str).collect();
    let next = lock_params(api.app_state.read().clone(), &refs);
    *api.app_state.write() = next;
    Json(OkResponse { ok: true, message: Some(format!("locked {} params", req.paths.len())) })
}

async fn post_unlock(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<LockRequest>,
) -> Json<OkResponse> {
    let refs: Vec<&str> = req.paths.iter().map(String::as_str).collect();
    let next = unlock_params(api.app_state.read().clone(), &refs);
    *api.app_state.write() = next;
    Json(OkResponse { ok: true, message: Some(format!("unlocked {} params", req.paths.len())) })
}

async fn post_play(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    let next = crate::state::toggle_sequencer_running(api.app_state.read().clone());
    // Ensure it ends up running regardless of current state
    let mut s = next;
    s.sequencer.running = true;
    *api.app_state.write() = s;
    Json(OkResponse { ok: true, message: None })
}

async fn post_stop(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    let mut next = api.app_state.read().clone();
    next.sequencer.running = false;
    *api.app_state.write() = next;
    Json(OkResponse { ok: true, message: None })
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

