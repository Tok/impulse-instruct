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

/// AI patch morph — schedule a sequence of LLM "nudge" prompts
/// that evolve the FX chain along a textual prompt across N bars.
/// Body: `{ "prompt": "...", "bars": 8, "calls": 8 }` — `calls`
/// defaults to `bars` (one nudge per bar).  The scheduler lives in
/// `ui::patch_morph_handler::tick_patch_morph` and fires on bar
/// boundaries via the existing LLM input channel.
pub async fn post_patch_morph(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<serde_json::Value>,
) -> Json<OkResponse> {
    let prompt = req
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if prompt.trim().is_empty() {
        return Json(OkResponse {
            ok: false,
            message: Some("morph requires a non-empty prompt".into()),
        });
    }
    let bars = req
        .get("bars")
        .and_then(|v| v.as_u64())
        .unwrap_or(8)
        .clamp(1, 64) as u32;
    let total_calls = req
        .get("calls")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(bars)
        .clamp(1, bars * 4); // soft cap at 4 calls per bar to avoid LLM flood

    let mut s = api.app_state.write();
    let step_division = s.sequencer.step_division;
    let now = s.global_step_count;
    s.patch_morph =
        crate::state::PatchMorphState::start(prompt.clone(), bars, total_calls, step_division, now);
    drop(s);
    api_log(
        &api,
        format!("[API] morph: \"{prompt}\" over {bars} bars × {total_calls} calls"),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!("morph started: {bars} bars, {total_calls} calls")),
    })
}

/// Eurorack-style "patch generator" — wipes the rack to its minimal
/// shape, then rebuilds it from a randomised layout (random voices /
/// FX / LFOs picked from curated pools, deterministic per seed).
/// Wires default cables on top so every voice reaches master out of
/// the box.  Optional `seed` parameter lets the caller replay an
/// interesting roll; without it, a nanosecond-derived seed is used.
/// Inner logic lives in `state::rack_random::apply_random_layout`
/// so the UI menu entry can call exactly the same code path.
pub async fn post_rack_random(
    AxumState(api): AxumState<ApiState>,
    Json(req): Json<serde_json::Value>,
) -> Json<OkResponse> {
    use crate::state::rack_random::apply_random_layout;

    let seed = req.get("seed").and_then(|v| v.as_u64()).unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });

    let layout = {
        let mut s = api.app_state.write();
        apply_random_layout(&mut s, seed)
    };

    api_log(
        &api,
        format!(
            "[API] rack/random: seed={seed} → {} voices, {} fx, {} lfos",
            layout.voices.len(),
            layout.fx.len(),
            layout.lfo_count
        ),
    );
    Json(OkResponse {
        ok: true,
        message: Some(format!(
            "random rack: {} voices, {} fx, {} lfos (seed {seed})",
            layout.voices.len(),
            layout.fx.len(),
            layout.lfo_count
        )),
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
