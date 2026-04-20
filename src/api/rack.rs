// ─── api/rack.rs ─────────────────────────────────────────────────────────────
// HTTP handlers for modular-rack manipulation: add modules, spawn agents,
// connect / adjust / remove cables.  Split out of `api/mod.rs` so the
// rack surface area owns its own file.

use axum::{Json, extract::State as AxumState, http::StatusCode};
use serde::Deserialize;

use super::{ApiState, OkResponse, api_log};
use crate::state::parse_module_kind;

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
    /// Optional per-audio-cable gain (0..=1.5).  Forward audio cables
    /// use this as a send amount on the voice→FX entry; FX→FX back-edges
    /// use it (clamped to `FEEDBACK_GAIN_MAX`) as the feedback mix level.
    /// `None` = leave the cable at the default 1.0.
    #[serde(default)]
    pub audio_gain: Option<f32>,
}

fn default_cable_kind() -> String {
    "control".into()
}

#[derive(Deserialize)]
pub struct RackRemoveRequest {
    /// Module ID to remove
    pub id: u32,
}

#[derive(Deserialize)]
pub struct RackCableGainRequest {
    pub from: u32,
    pub to: u32,
    /// 0..=1.5 (unity-ish range plus headroom).  Feedback-edge clamping
    /// to `FEEDBACK_GAIN_MAX` happens at `compile_fx_plan` time.
    pub gain: f32,
}

pub async fn post_rack_add(
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

pub async fn post_rack_agent(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackAgentRequest>,
) -> Json<serde_json::Value> {
    // Model resolution mirrors the wizard (src/ui/wizard.rs:566-588):
    // if the requested pattern already matches the globally-loaded model,
    // leave `model_path = None` so the agent inherits instead of spinning
    // up a second llama-server.  Without this guard, `find_model("gemma")`
    // pins the agent to the first Gemma GGUF in `models/` in lexical order
    // — which is the 26B-A4B thinking variant when it's on disk — even
    // though the user already has the E4B loaded.  The result was the demo
    // spawning a second server with the wrong (reasoning) model and every
    // lane failing with `content:""` + `finish_reason:length`.
    let model_path = match req.model.as_deref() {
        None => None,
        Some(pat) => {
            let pat_lower = pat.to_ascii_lowercase();
            let global_lower = api.app_state.read().llm.model_path.to_ascii_lowercase();
            if global_lower.contains(&pat_lower) {
                None
            } else {
                let available_models = crate::ui::scan_models();
                crate::llm::vram::find_model(pat, &available_models)
            }
        }
    };

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

pub async fn post_rack_cable(
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
        // Apply an explicit audio_gain if the caller supplied one —
        // mutating the most-recently-pushed cable that matches the
        // (from, to) pair.  Clamped to [0, 1.5]; feedback-edge clamps
        // apply later at `compile_fx_plan` time.
        if let Some(g) = req.audio_gain
            && let Some(cable) = s.rack.cables.iter_mut().rev().find(|c| {
                c.from.module_id == req.from
                    && c.to.module_id == req.to
                    && c.from.kind == PortKind::Audio
            })
        {
            cable.audio_gain = g.clamp(0.0, 1.5);
        }
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

pub async fn post_rack_cable_gain(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<RackCableGainRequest>,
) -> Json<OkResponse> {
    let mut s = api.app_state.write();
    let clamped = req.gain.clamp(0.0, 1.5);
    let mut hit = false;
    for cable in &mut s.rack.cables {
        if cable.from.module_id == req.from
            && cable.to.module_id == req.to
            && cable.from.kind == crate::state::PortKind::Audio
        {
            cable.audio_gain = clamped;
            hit = true;
            break;
        }
    }
    drop(s);
    api.params_dirty
        .store(true, std::sync::atomic::Ordering::Relaxed);
    if hit {
        api_log(
            &api,
            format!(
                "[API] rack: cable gain {} → {} = {:.3}",
                req.from, req.to, clamped
            ),
        );
        Json(OkResponse {
            ok: true,
            message: Some(format!("gain {:.3}", clamped)),
        })
    } else {
        Json(OkResponse {
            ok: false,
            message: Some("no matching audio cable".into()),
        })
    }
}

pub async fn post_rack_remove(
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
