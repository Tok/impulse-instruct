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
use crate::state::{AppState, apply_llm_update, lock_params, propagate_style, unlock_params};

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
    #[serde(default)]
    pub one_shot: bool,
    /// Target agent by persona name (e.g. "BASS", "DRUMS"). Omit for global/first agent.
    #[serde(default)]
    pub agent: Option<String>,
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

#[derive(Deserialize)]
pub struct StyleRequest {
    /// Style id from styles.json (e.g. "drum_and_bass"), "__free__",
    /// "__custom__", or empty/null to clear the active style.
    #[serde(default)]
    pub id: Option<String>,
    /// Optional custom style brief, used when id == "__custom__".
    #[serde(default)]
    pub custom_text: Option<String>,
}

#[derive(Deserialize)]
pub struct FlipRequest {
    /// true = show back (cables), false = show front (knobs).
    pub show_back: bool,
}

#[derive(Deserialize)]
pub struct RackAddRequest {
    /// Module kind: "AcidBass", "DrumKit808", "DrumKit909", "FxReverb", "FxDelay", etc.
    pub kind: String,
}

#[derive(Deserialize)]
pub struct RackAgentRequest {
    /// Agent persona name (e.g. "BASS", "DRUMS", "FX")
    pub persona: String,
    /// Scope — what the agent controls (e.g. ["bass"], ["kit_a", "kit_b"])
    #[serde(default)]
    pub scope: Vec<String>,
    /// Model pattern to match (e.g. "gemma", "bonsai"). Omit to inherit default.
    #[serde(default)]
    pub model: Option<String>,
    /// Conversation mode: "off", "producer", "dj", "mc". Default: "producer".
    #[serde(default)]
    pub mode: Option<String>,
    /// If true, auto-add a TTS module and wire a control cable from this agent.
    #[serde(default)]
    pub tts: Option<bool>,
}

#[derive(Deserialize)]
pub struct RackCableRequest {
    /// Source module ID
    pub from: u32,
    /// Target module ID
    pub to: u32,
    /// Cable type: "control" or "audio" (default: "control")
    #[serde(default = "default_cable_kind")]
    pub kind: String,
}

fn default_cable_kind() -> String {
    "control".into()
}

#[derive(Deserialize)]
pub struct RackRemoveRequest {
    /// Module ID to remove
    pub id: u32,
}

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
    api_log(&api, format!("[API] prompt → {}: {}", target, req.prompt));
    api.llm_tx
        .try_send(LlmInput::Infer {
            prompt: req.prompt,
            one_shot: true,
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

/// Set (or clear) the global active style and propagate it to all agents whose
/// style is not locked. This mirrors what the UI style dropdown does, giving
/// demo scripts and external controllers a way to pin the style before
/// inference so prior-session bleed can't override the user's intent.
async fn post_style(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<StyleRequest>,
) -> Json<OkResponse> {
    let id = req.id.as_deref().map(str::trim).filter(|s| !s.is_empty());
    {
        let current = snapshot(&api.app_state);
        let next = match id {
            Some(id) => {
                let mut s = current;
                s.llm.active_style = Some(id.to_string());
                if let Some(text) = req.custom_text.as_deref() {
                    s.llm.custom_style_text = text.to_string();
                }
                propagate_style(s, id)
            }
            None => {
                let mut s = current;
                s.llm.active_style = None;
                for agent in &mut s.llm_agents {
                    if !agent.style_locked {
                        agent.active_style = None;
                    }
                }
                s
            }
        };
        *api.app_state.write() = next;
    }
    api_log(
        &api,
        match id {
            Some(id) => format!("[API] style: set to {}", id),
            None => "[API] style: cleared".into(),
        },
    );
    Json(OkResponse {
        ok: true,
        message: Some(match id {
            Some(id) => format!("style set to {}", id),
            None => "style cleared".into(),
        }),
    })
}

async fn post_preset(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<PresetRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    use crate::llm::vram::{PRESETS, find_model};

    let preset = PRESETS
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&req.name))
        .ok_or(StatusCode::NOT_FOUND)?;

    let available_models = crate::ui::scan_models();

    // Do everything in a single write lock — no snapshot+swap race.
    let mut agent_names = Vec::new();
    {
        let mut s = api.app_state.write();

        // Remove all existing agents
        let old_ids: Vec<u32> = s.llm_agents.iter().map(|a| a.id).collect();
        for id in &old_ids {
            s.rack.remove_module(*id);
        }
        s.llm_agents.clear();

        // Spawn agents from the preset (inline, no full-state swap)
        for pa in preset.agents {
            let model_path = find_model(pa.model_pattern, &available_models);
            let scope: Vec<String> = pa.scope.iter().map(|sc| sc.to_string()).collect();

            let id = s.rack.add_module(crate::state::ModuleKind::LlmAgent);
            let mut agent = crate::state::LlmAgentState::from_singleton(id, &s.llm);
            agent.persona_name = pa.persona.to_string();
            agent.scope = scope.clone();
            agent.role = pa.role;
            agent.model_path = model_path.clone();

            let targets: Vec<u32> = if scope.is_empty() {
                s.rack
                    .modules
                    .iter()
                    .filter(|m| {
                        !matches!(
                            m.kind,
                            crate::state::ModuleKind::MasterOutput
                                | crate::state::ModuleKind::LlmAgent
                                | crate::state::ModuleKind::LlmConsole
                        )
                    })
                    .map(|m| m.id)
                    .collect()
            } else {
                s.rack
                    .modules
                    .iter()
                    .filter(|m| {
                        scope
                            .iter()
                            .any(|sc| crate::state::rack_kind_name_matches(m.kind, sc))
                    })
                    .map(|m| m.id)
                    .collect()
            };
            for tid in &targets {
                s.rack.connect_control(id, *tid);
            }
            s.llm_agents.push(agent);
            agent_names.push(pa.persona);
        }

        // Set the main model path from the first agent
        if let Some(first) = preset.agents.first()
            && let Some(model) = find_model(first.model_pattern, &available_models)
        {
            s.llm.model_path = model;
        }
    }

    let msg = format!(
        "[API] preset '{}': {} agents ({})",
        preset.name,
        agent_names.len(),
        agent_names.join(", ")
    );
    api_log(&api, msg.clone());

    Ok(Json(OkResponse {
        ok: true,
        message: Some(msg),
    }))
}

async fn post_flip(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<FlipRequest>,
) -> Json<OkResponse> {
    let label = if req.show_back {
        "back (cables)"
    } else {
        "front (knobs)"
    };
    api.app_state.write().rack_flip_requested = Some(req.show_back);
    api_log(&api, format!("[API] rack flip → {}", label));
    Json(OkResponse {
        ok: true,
        message: Some(format!("rack: {}", label)),
    })
}

// ─── Rack manipulation endpoints ─────────────────────────────────────────────

/// Parse a module kind from a string name.
// parse_module_kind now lives in state::rack_scope so both the HTTP API
// and the LLM rack.add action path parse names identically.
use crate::state::parse_module_kind;

async fn post_rack_add(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackAddRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    use crate::state::{PortDir, PortKind, PortRef};
    let kind = parse_module_kind(&req.kind).ok_or(StatusCode::BAD_REQUEST)?;
    let mut s = api.app_state.write();
    let id = s.rack.add_module(kind);
    // Auto-wire voice and FX modules to MasterOutput with an audio cable.
    if !matches!(
        kind.default_zone(),
        crate::state::Zone::Global | crate::state::Zone::Ai
    ) && let Some(master_id) = s
        .rack
        .modules
        .iter()
        .find(|m| m.kind == crate::state::ModuleKind::MasterOutput)
        .map(|m| m.id)
    {
        s.rack.connect(
            PortRef {
                module_id: id,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: master_id,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
    }
    // Create per-module state for TTS modules.
    if kind == crate::state::ModuleKind::NeuTts {
        s.tts_modules.push(crate::state::TtsModuleState::new(id));
    }
    // Auto-scroll to the new module so it's visible.
    s.scroll_target = Some(req.kind.clone());
    drop(s);
    api_log(&api, format!("[API] rack: added {:?} (id={})", kind, id));
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

async fn post_rack_agent(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackAgentRequest>,
) -> Json<serde_json::Value> {
    let available_models = crate::ui::scan_models();
    let model_path = req
        .model
        .as_deref()
        .and_then(|pat| crate::llm::vram::find_model(pat, &available_models));

    // Add agent directly on the locked state — DO NOT snapshot+swap the entire
    // AppState, as that races with the audio thread writing current_step.
    let id = {
        let mut s = api.app_state.write();
        let id = s.rack.add_module(crate::state::ModuleKind::LlmAgent);
        let mut agent = crate::state::LlmAgentState::from_singleton(id, &s.llm);
        agent.persona_name = req.persona.clone();
        agent.scope = req.scope.clone();
        agent.role = crate::state::AgentRole::Specialist;
        agent.model_path = model_path;
        if let Some(ref mode_str) = req.mode {
            agent.conversation_mode = match mode_str.to_ascii_lowercase().as_str() {
                "off" => crate::state::ConversationMode::Off,
                "dj" => crate::state::ConversationMode::Dj,
                "mc" => crate::state::ConversationMode::Mc,
                _ => crate::state::ConversationMode::Producer,
            };
        }
        if req.tts == Some(true) {
            // Add a TTS module and wire a control cable from this agent.
            let tts_id = s.rack.add_module(crate::state::ModuleKind::NeuTts);
            s.tts_modules
                .push(crate::state::TtsModuleState::new(tts_id));
            s.rack.connect_control(id, tts_id);
        }

        // Wire control cables to modules matching the scope
        let targets: Vec<u32> = if req.scope.is_empty() {
            s.rack
                .modules
                .iter()
                .filter(|m| {
                    !matches!(
                        m.kind,
                        crate::state::ModuleKind::MasterOutput
                            | crate::state::ModuleKind::LlmAgent
                            | crate::state::ModuleKind::LlmConsole
                    )
                })
                .map(|m| m.id)
                .collect()
        } else {
            s.rack
                .modules
                .iter()
                .filter(|m| {
                    req.scope
                        .iter()
                        .any(|sc| crate::state::rack_kind_name_matches(m.kind, sc))
                })
                .map(|m| m.id)
                .collect()
        };
        for tid in &targets {
            s.rack.connect_control(id, *tid);
        }
        s.llm_agents.push(agent);
        // Auto-scroll to the AI zone (now its own tab containing the console
        // plus all agents) so the newly added agent is visible.
        s.scroll_target = Some("ai".to_string());
        id
    };

    api_log(
        &api,
        format!(
            "[API] rack: added agent {} (id={}, scope={:?})",
            req.persona, id, req.scope
        ),
    );
    Json(serde_json::json!({ "ok": true, "id": id }))
}

async fn post_rack_cable(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackCableRequest>,
) -> Json<OkResponse> {
    let mut s = api.app_state.write();
    if req.kind == "control" {
        s.rack.connect_control(req.from, req.to);
    } else {
        use crate::state::{PortDir, PortKind, PortRef};
        s.rack.connect(
            PortRef {
                module_id: req.from,
                dir: PortDir::Out,
                kind: PortKind::Audio,
                index: 0,
            },
            PortRef {
                module_id: req.to,
                dir: PortDir::In,
                kind: PortKind::Audio,
                index: 0,
            },
        );
    }
    drop(s);
    api_log(
        &api,
        format!("[API] rack: cable {} → {} ({})", req.from, req.to, req.kind),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!("cable {} → {}", req.from, req.to)),
    })
}

async fn post_rack_remove(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackRemoveRequest>,
) -> Json<OkResponse> {
    let mut s = api.app_state.write();
    s.rack.remove_module(req.id);
    s.llm_agents.retain(|a| a.id != req.id);
    s.tts_modules.retain(|t| t.id != req.id);
    drop(s);
    api_log(&api, format!("[API] rack: removed module {}", req.id));
    Json(OkResponse {
        ok: true,
        message: Some(format!("removed {}", req.id)),
    })
}

/// Full AppState reset — everything back to defaults, Empty rack preset.
/// Preserves the currently-loaded model path so the user doesn't lose it.
/// Intended for demo recording, CI, and automated sessions that need to
/// guarantee a blank slate even when attaching to an already-running app.
async fn post_state_reset(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
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
async fn post_rack_reset(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
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
        .route("/api/scroll", post(post_scroll))
        .route("/api/preset", post(post_preset))
        .route("/api/style", post(post_style))
        .route("/api/flip", post(post_flip))
        .route("/api/rack/add", post(post_rack_add))
        .route("/api/rack/agent", post(post_rack_agent))
        .route("/api/rack/cable", post(post_rack_cable))
        .route("/api/rack/remove", post(post_rack_remove))
        .route("/api/rack/reset", post(post_rack_reset))
        .route("/api/state/reset", post(post_state_reset))
        .route("/api/rack/collapse", post(post_collapse))
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
