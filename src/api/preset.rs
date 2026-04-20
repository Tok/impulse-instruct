// ─── api/preset.rs ───────────────────────────────────────────────────────────
// `POST /api/preset` — swap the LLM agent roster to one of the baked-in
// presets (Solo / Duo / Swarm / Crew / Voices / Lite) from `llm::vram`.
// Each preset defines a set of (persona, scope, role, model_pattern)
// agents; the handler clears the current roster, spawns fresh agents,
// wires them to master, and (when the preset's first-agent model
// pattern differs from the current global) pins the global model.
//
// Lifted out of api/mod.rs so that file stays under the 1000-line cap.

use axum::{Json, extract::State as AxumState, http::StatusCode};

use super::{ApiState, OkResponse, PresetRequest, api_log};

pub async fn post_preset(
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

        let global_model_lower = s.llm.model_path.to_ascii_lowercase();

        // Spawn agents from the preset (inline, no full-state swap).
        // If the preset's `model_pattern` already matches the user's
        // global, leave `model_path = None` so the agent inherits —
        // otherwise `find_model("gemma", …)` would pin every agent to
        // the first alphabetical Gemma in `available_models` (e.g. an
        // IQ2 quant) and load a redundant second llama-server.
        for pa in preset.agents {
            let model_path = if global_model_lower.contains(pa.model_pattern) {
                None
            } else {
                find_model(pa.model_pattern, &available_models)
            };
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

        // Only re-pin the global model if the preset's first-agent
        // pattern doesn't already match the user's chosen global —
        // otherwise we'd silently swap E4B for whatever sorts first.
        if let Some(first) = preset.agents.first()
            && !global_model_lower.contains(first.model_pattern)
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
