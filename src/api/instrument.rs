// ─── api/instrument.rs ───────────────────────────────────────────────────────
// HTTP handlers for instrument / style selection: sample loaders (amen,
// granular), global style set + propagate, randomize, rack flip.  Split out
// of `api/mod.rs` so per-domain handlers live in their own file.

use axum::{Json, extract::State as AxumState};
use serde::Deserialize;

use super::{ApiState, OkResponse, api_log, snapshot};
use crate::state::propagate_style;

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

/// Load a sample into the AmenSampler — either a specific path or a random
/// file from samples/amen/.  The API writes the path into AppState; the UI
/// panel auto-detects the change on its next frame and handles the actual
/// WAV decode + audio-thread push + waveform cache rebuild (the same code
/// path the user-facing picker uses).
pub async fn post_amen(
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
pub async fn post_granular(
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

/// Set (or clear) the global active style and propagate it to all agents whose
/// style is not locked. This mirrors what the UI style dropdown does, giving
/// demo scripts and external controllers a way to pin the style before
/// inference so prior-session bleed can't override the user's intent.
pub async fn post_style(
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

pub async fn post_randomize(AxumState(api): AxumState<ApiState>) -> Json<OkResponse> {
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

pub async fn post_flip(
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
