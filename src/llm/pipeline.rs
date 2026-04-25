// ─── llm/pipeline.rs ─────────────────────────────────────────────────────────
// Sequential lane-inference executor.  Replaces the monolithic
// "emit every field at once" inference with a short pipeline:
//
//   1. Planner call — reads user prompt, returns ordered LanePlan
//   2. For each lane in the plan:
//        a. Build focused system prompt + required-fields schema
//        b. Fire inference with grammar-constrained output
//        c. Filter response JSON to the lane's scope
//        d. Apply to AppState
//        e. Emit progress event so the UI streams per-lane updates
//
// Per-lane outputs are small (100-400 tokens) which dodges the decode
// truncation + rule-skipping we saw from the monolithic path.  State
// updates between lanes let later calls see earlier output (bass #2
// can counterpoint bass #1 because bass #1 is already in the snapshot
// by the time bass #2's prompt is built).
//
// The `PipelineBackend` trait is narrow on purpose — the real
// LlamaServerBackend gets a wrapper, and tests get a mock, without
// reshaping the app-wide `LlmBackend` trait.

use crate::llm::lanes::{LaneKind, build_lane_prompt, lane_schema};
use crate::llm::planner::{
    LanePlan, build_planner_prompt, default_plan, parse_planner_output, planner_schema,
};
use crate::llm::planner_heuristic::heuristic_plan;
use crate::llm::{SamplingParams, json_repair};
use crate::state::AppState;
use serde_json::Value;

/// A structured-output LLM call.  Separated from `LlmBackend::infer` so
/// the pipeline can plug in any backend (real llama-server, mock in
/// tests, future alternatives) without touching the monolithic trait.
pub trait PipelineBackend {
    fn infer_lane_json(
        &mut self,
        system: &str,
        user: &str,
        schema: &Value,
        sampling: &SamplingParams,
    ) -> anyhow::Result<Value>;
}

/// Everything the pipeline emits about a single lane step.  The UI
/// subscribes to this via the progress callback to stream updates.
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// Planner finished; the pipeline is about to start running lanes.
    PlanReady { plan: LanePlan },
    /// Entering a lane's inference.
    LaneStarted { lane: LaneKind },
    /// Lane inference succeeded; `update` is the applied JSON, `ms` is wall time.
    LaneApplied {
        lane: LaneKind,
        update: Value,
        ms: u128,
    },
    /// Lane inference or apply failed; pipeline continues with the next lane.
    LaneFailed { lane: LaneKind, error: String },
    /// Lane was skipped because a mid-pipeline live-state check found the
    /// lane's target module missing / disabled (e.g. user removed the
    /// rack module while the pipeline was mid-cycle).  Distinct from
    /// `LaneFailed` so the progress UI can show "skipped" without
    /// framing it as a model error.
    LaneSkipped { lane: LaneKind, reason: String },
    /// Whole pipeline done.
    PipelineDone {
        total_ms: u128,
        lanes_succeeded: usize,
        lanes_failed: usize,
    },
}

/// Run the sequential lane pipeline.  Returns the new `AppState` with
/// every successful lane applied; failed lanes are logged via the
/// progress callback but don't abort the pipeline.
///
/// `progress` fires for plan/lane events (logging / UI).
///
/// `on_lane_applied` fires once per SUCCESSFULLY applied lane with the
/// lane's filtered JSON output + its apply scope.  The caller is
/// expected to re-play that update against the shared `AppState` via
/// `apply_llm_update` so the audio thread hears the change immediately
/// instead of waiting for every lane to finish.  This replaces an
/// earlier design that copied full sub-structs from the pipeline's
/// start-of-pipeline snapshot back to live state — which silently
/// clobbered any `api_params` / UI changes the user made *during* the
/// pipeline (voice-2 `enabled`, kit volumes, LFO targets, etc.).
/// Tests that don't care about writeback can pass `|_, _| {}`.
pub fn run_pipeline<B: PipelineBackend>(
    mut state: AppState,
    user_prompt: &str,
    backend: &mut B,
    sampling: &SamplingParams,
    is_jam: bool,
    live_state: Option<&std::sync::Arc<parking_lot::RwLock<AppState>>>,
    mut progress: impl FnMut(PipelineEvent),
    mut on_lane_applied: impl FnMut(&Value, &[String]),
) -> AppState {
    let t_start = std::time::Instant::now();

    // ── 1. Planner ───────────────────────────────────────────────────────────
    // Jam cycles (is_jam=true) use the Phase 2 weighted single-lane picker
    // so each cycle rewrites exactly one voice/kit rather than rerunning
    // the full default plan; score + recency decay in `lane_scheduler`
    // rotate focus across voices between cycles.  User turns fall through
    // to the regular heuristic → LLM planner → default chain.
    let plan = if is_jam {
        let mut rng = crate::llm::lane_scheduler::Xorshift32::from_entropy();
        let jp = crate::llm::planner_jam::jam_plan(&state, &mut rng);
        if jp.lanes.is_empty() {
            log::info!("pipeline: jam_plan empty, falling back to default_plan");
            default_plan(&state)
        } else {
            log::info!("pipeline: {}", jp.rationale);
            jp
        }
    } else if let Some(heur) = {
        // Strip the trailing /think or /no_think directive the LLM worker
        // appends before reaching the pipeline.  The heuristic's 120-char
        // sanity cap (see `heuristic_plan`) was firing on otherwise-short
        // prompts because the 7–10 extra tag characters pushed them over,
        // forcing a slow LLM planner + fallback plan that drops the
        // requested lane.  The think tag is for the inference server,
        // not for natural-language matching.
        let raw = user_prompt
            .strip_suffix(" /no_think")
            .or_else(|| user_prompt.strip_suffix(" /think"))
            .unwrap_or(user_prompt);
        heuristic_plan(&state, raw)
    } {
        log::info!("pipeline: heuristic plan hit — {}", heur.rationale);
        heur
    } else {
        run_planner(&state, user_prompt, backend, sampling).unwrap_or_else(|e| {
            log::info!(
                "pipeline: planner fell back to default plan — {}",
                truncate(&e.to_string(), 200)
            );
            default_plan(&state)
        })
    };
    // Defensive filter: drop any lane whose voice/module isn't actually
    // present in the rack right now.  `parse_planner_output` already
    // applies `lane_is_live`, but the planner sometimes produces stale
    // labels (e.g. after a style switch removes a voice mid-cycle), and
    // `default_plan` only reads the rack at construction.  Filtering
    // here means a no-op-prone lane never burns an inference call.
    let plan = LanePlan {
        lanes: plan
            .lanes
            .into_iter()
            .filter(|&l| crate::llm::planner::lane_is_live_pub(&state, l))
            .collect(),
        rationale: plan.rationale,
        from_retry: plan.from_retry,
    };
    if plan.lanes.is_empty() {
        progress(PipelineEvent::PipelineDone {
            total_ms: t_start.elapsed().as_millis(),
            lanes_succeeded: 0,
            lanes_failed: 0,
        });
        return state;
    }
    progress(PipelineEvent::PlanReady { plan: plan.clone() });

    // ── 2. Lanes ─────────────────────────────────────────────────────────────
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    for lane in plan.lanes.iter().copied() {
        // Mid-pipeline live-state re-check: `plan.lanes` was filtered
        // against the snapshot at plan time, but the user (or another
        // agent / the API) can mutate the rack while the pipeline is
        // mid-cycle.  Re-checking against the shared `live_state` here
        // means we skip lanes whose target module has since been
        // removed or disabled, instead of burning an inference call
        // producing output that won't apply.  `None` live_state keeps
        // the old snapshot-only behaviour (tests, one-shot turns).
        if let Some(live) = live_state {
            let live_snap = live.read();
            if !crate::llm::planner::lane_is_live_pub(&live_snap, lane) {
                let reason = format!("{}: module removed / disabled mid-cycle", lane.label());
                log::warn!("pipeline: skipping {} — {}", lane.label(), reason);
                progress(PipelineEvent::LaneSkipped { lane, reason });
                continue;
            }
        }
        progress(PipelineEvent::LaneStarted { lane });
        let t_lane = std::time::Instant::now();
        let system = build_lane_prompt(&state, lane);
        let schema = lane_schema(lane);
        match infer_lane_with_retry(backend, &system, user_prompt, &schema, sampling, lane) {
            Ok(raw) => {
                // Log the raw lane output before filtering so we can see
                // what the model actually emitted — essential when a lane
                // "applies" but produces silent output.
                log::info!(
                    "pipeline: {} raw = {}",
                    lane.label(),
                    truncate(&raw.to_string(), 400)
                );
                let filtered = filter_lane_output(lane, raw);
                let scope = lane_apply_scope(lane);
                state = crate::state::apply_llm_update(state, &filtered, &scope);
                // Commit this lane's updates to the shared app state right
                // away so the audio thread picks them up — without this,
                // every lane silently accumulates and the full turn's
                // worth of patterns would suddenly switch on at the end
                // (which was reported as "blocking and silent until the
                // last lane is in").  The callback gets the filtered JSON
                // + scope rather than the local state, so it can apply
                // ONLY what the lane actually wrote against the live
                // AppState — preserving any user / api_params edits that
                // happened while the pipeline was in flight.
                on_lane_applied(&filtered, &scope);
                progress(PipelineEvent::LaneApplied {
                    lane,
                    update: filtered,
                    ms: t_lane.elapsed().as_millis(),
                });
                succeeded += 1;
            }
            Err(e) => {
                progress(PipelineEvent::LaneFailed {
                    lane,
                    error: truncate(&e.to_string(), 240),
                });
                failed += 1;
            }
        }
    }

    progress(PipelineEvent::PipelineDone {
        total_ms: t_start.elapsed().as_millis(),
        lanes_succeeded: succeeded,
        lanes_failed: failed,
    });
    state
}

/// Fire the planner and parse its output into a `LanePlan`.  Empty /
/// malformed output returns an Err so the caller can fall back to
/// `default_plan`.
fn run_planner<B: PipelineBackend>(
    state: &AppState,
    user_prompt: &str,
    backend: &mut B,
    sampling: &SamplingParams,
) -> anyhow::Result<LanePlan> {
    let system = build_planner_prompt(state);
    let schema = planner_schema();
    // Planner should be deterministic-ish — keep heat low regardless
    // of jam heat so the planner picks the obvious lane set.
    let planner_sampling = SamplingParams {
        heat: 0.1,
        temperature: 0.4,
        ..sampling.clone()
    };
    let raw = backend.infer_lane_json(&system, user_prompt, &schema, &planner_sampling)?;
    parse_planner_output(state, &raw).ok_or_else(|| anyhow::anyhow!("planner returned empty plan"))
}

/// Lane inference with a one-shot temperature-bump retry on failure.
/// First attempt uses `sampling` as-is; if the model returns Err
/// (parse / repair / server error), retry once with `temperature +
/// LANE_RETRY_TEMP_BUMP` before propagating the failure.  The bump
/// breaks the model out of stuck-output modes that triggered the
/// initial parse failure (most commonly: the grammar-constrained
/// decode hit a dead-end token sequence and the JSON came back
/// truncated or malformed).  One retry only — chained retries would
/// stall the whole pipeline if a lane is fundamentally broken.
pub(crate) const LANE_RETRY_TEMP_BUMP: f32 = 0.1;

fn infer_lane_with_retry<B: PipelineBackend>(
    backend: &mut B,
    system: &str,
    user: &str,
    schema: &Value,
    sampling: &SamplingParams,
    lane: LaneKind,
) -> anyhow::Result<Value> {
    match backend.infer_lane_json(system, user, schema, sampling) {
        Ok(v) => Ok(v),
        Err(first_err) => {
            let bumped = SamplingParams {
                temperature: (sampling.temperature + LANE_RETRY_TEMP_BUMP).min(2.0),
                ..sampling.clone()
            };
            log::info!(
                "pipeline: {} retry @ temp={:.2} — first attempt: {}",
                lane.label(),
                bumped.temperature,
                truncate(&first_err.to_string(), 160)
            );
            backend.infer_lane_json(system, user, schema, &bumped)
        }
    }
}

// `lane_apply_scope` and `filter_lane_output` live in
// `pipeline_filter.rs` (extracted to keep this file under the 1000-
// line cap).  Re-exported here so existing imports keep working.
pub use crate::llm::pipeline_filter::{filter_lane_output, lane_apply_scope};

/// Tight string truncation for log messages — avoids dumping a whole
/// llama-server stack trace into the UI log when a lane fails.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Break on char boundary so we don't split a multi-byte glyph.
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}…", &s[..end])
    }
}

// Keep `json_repair` referenced so the module-level use line doesn't
// warn if future refactors drop its direct use — the real backend
// wrapper (Phase 5) will call `json_repair::repair_truncated_json` on
// the raw server response before handing it to the pipeline.
#[allow(unused_imports)]
use json_repair as _json_repair_keepalive;

// `PoolBackend` + `run_pipeline_via_pool` live in `pipeline_pool.rs`
// (extracted to keep this file under the 1000-line cap).  Re-exported
// here so existing imports (`crate::llm::pipeline::run_pipeline_via_pool`)
// keep working.
pub use crate::llm::pipeline_pool::{PoolBackend, run_pipeline_via_pool};

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
