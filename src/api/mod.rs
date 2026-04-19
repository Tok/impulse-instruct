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

#[derive(Deserialize)]
pub struct AmenRequest {
    /// Explicit path to a WAV file (relative or absolute).  Use this when
    /// you know which sample you want, e.g. for scripted demos.
    #[serde(default)]
    pub path: Option<String>,
    /// When true, pick a random WAV from samples/amen/.  Ignored when
    /// `path` is set.
    #[serde(default)]
    pub random: bool,
}

/// Same shape as AmenRequest, separate type so we can extend either
/// module independently without breaking the other.
#[derive(Deserialize)]
pub struct GranularRequest {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub random: bool,
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
    /// Model pattern to match (e.g. "gemma", "qwen3"). Omit to inherit default.
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

/// Set (or clear) the global active style and propagate it to all agents whose
/// style is not locked. This mirrors what the UI style dropdown does, giving
/// demo scripts and external controllers a way to pin the style before
/// inference so prior-session bleed can't override the user's intent.
/// Load a sample into the AmenSampler — either a specific path or a random
/// file from samples/amen/.  The API writes the path into AppState; the UI
/// panel auto-detects the change on its next frame and handles the actual
/// WAV decode + audio-thread push + waveform cache rebuild (the same code
/// path the user-facing picker uses).
async fn post_amen(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<AmenRequest>,
) -> Json<OkResponse> {
    let resolved: Option<String> = if let Some(p) = req.path.as_deref().filter(|s| !s.is_empty()) {
        Some(p.to_string())
    } else if req.random {
        crate::ui::panels::amen::pick_random_sample()
    } else {
        None
    };
    match resolved {
        Some(p) => {
            api.app_state.write().amen.path = p.clone();
            // Clear custom slice positions — they belonged to the previous
            // sample and probably won't land on the new file's transients.
            api.app_state.write().amen.slice_positions.clear();
            api_log(&api, format!("[API] amen: loaded {}", p));
            Json(OkResponse {
                ok: true,
                message: Some(format!("amen: {}", p)),
            })
        }
        None => {
            api_log(&api, "[API] amen: no sample resolved".to_string());
            Json(OkResponse {
                ok: false,
                message: Some("no path and no samples found in samples/amen/".into()),
            })
        }
    }
}

/// Load a texture sample into the granular voice — mirror of /api/amen.
/// Writes granular.path in AppState; the UI panel picks up the change
/// and handles the full load (decode + audio-thread push via
/// AudioCommand::LoadGranular) on its next frame.
async fn post_granular(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<GranularRequest>,
) -> Json<OkResponse> {
    let resolved: Option<String> = if let Some(p) = req.path.as_deref().filter(|s| !s.is_empty()) {
        Some(p.to_string())
    } else if req.random {
        crate::ui::panels::granular::pick_random_texture()
    } else {
        None
    };
    match resolved {
        Some(p) => {
            api.app_state.write().granular.path = p.clone();
            api_log(&api, format!("[API] granular: loaded {}", p));
            Json(OkResponse {
                ok: true,
                message: Some(format!("granular: {}", p)),
            })
        }
        None => {
            api_log(&api, "[API] granular: no sample resolved".to_string());
            Json(OkResponse {
                ok: false,
                message: Some("no path and no samples found in samples/textures/".into()),
            })
        }
    }
}

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

async fn post_randomize(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
    use crate::llm::LlmInput;
    use crate::llm::styles::StyleCatalog;
    let styles = StyleCatalog::get().styles();
    if styles.is_empty() {
        return Json(OkResponse {
            ok: false,
            message: Some("no styles available".into()),
        });
    }
    // Cheap nanosecond-based pick — good enough for a UX dice roll without a
    // rand-crate dependency.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    let style = &styles[nanos % styles.len()];
    let style_id = style.id.clone();
    let style_name = style.name.clone();
    let baseline = style.baseline_params.clone();
    let rack_modules = style.rack_modules.clone();
    // Apply baseline params + rack modules + style id under one write.
    {
        use crate::state::{apply_llm_update, parse_module_kind};
        let mut s_owned = api.app_state.read().clone();
        if let Some(bp) = baseline {
            s_owned = apply_llm_update(s_owned, &bp, &[]);
        }
        let mut added: Vec<&str> = Vec::new();
        for name in &rack_modules {
            let Some(kind) = parse_module_kind(name) else {
                continue;
            };
            if !s_owned.rack.modules.iter().any(|m| m.kind == kind) {
                s_owned.rack.add_module(kind);
                added.push(name.as_str());
            }
        }
        if !added.is_empty() {
            s_owned.rack.wire_default_cables();
        }
        s_owned.llm.active_style = Some(style_id.clone());
        for agent in &mut s_owned.llm_agents {
            if !agent.style_locked {
                agent.active_style = Some(style_id.clone());
            }
        }
        *api.app_state.write() = s_owned;
    }
    // Kick the LLM into generating a fresh pattern for the picked style.
    let _ = api.llm_tx.try_send(LlmInput::Infer {
        prompt: format!(
            "FULL RESET to {} — randomize: generate all parameters from scratch.",
            style_name
        ),
        one_shot: true,
        agent_id: None,
    });
    api_log(
        &api,
        format!(
            "[API] randomize → style '{}' ({} rack modules)",
            style_id,
            rack_modules.len()
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!("randomized → {}", style_name)),
    })
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
            // Scroll to the new TTS module so the viewer sees where the
            // MC/DJ voice actually comes from in the rack.  Overwrites the
            // "ai" scroll_target set below; TTS is the more interesting
            // destination when a voice just appeared.
            s.scroll_target = Some("tts".to_string());
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
        // Auto-scroll to the AI zone by default so the new agent is
        // visible.  If a TTS module was also added, the earlier tts block
        // already set scroll_target to "tts" (a more interesting
        // destination for MC/DJ spawns) — don't overwrite it.
        if req.tts != Some(true) {
            s.scroll_target = Some("ai".to_string());
        }
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
        .route("/api/rack/mod_cable", post(post_rack_mod_cable))
        .route("/api/rack/mod_target", post(post_rack_mod_target))
        .route("/api/rack/mod_depth", post(post_rack_mod_depth))
        .route("/api/rack/remove", post(post_rack_remove))
        .route("/api/rack/pad", post(post_rack_pad))
        .route("/api/rack/reset", post(post_rack_reset))
        .route("/api/state/reset", post(post_state_reset))
        .route("/api/rack/collapse", post(post_collapse))
        .route("/api/midi/export", post(post_midi_export))
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
