// ─── llm/pipeline_pool.rs ────────────────────────────────────────────────────
// Production wiring between `LlamaServerPool` and the lane pipeline —
// extracted from pipeline.rs to keep that file under the 1000-line
// cap.  The two units here are:
//
//   • `PoolBackend` — adapter that implements `PipelineBackend` by
//     routing each `infer_lane_json` call to a live llama-server via
//     the pool / port pair.
//
//   • `run_pipeline_via_pool` — convenience entrypoint that builds
//     the adapter and runs the pipeline in one call.  This is what
//     the LLM worker loop uses.
//
// Tests use the in-file `MockBackend` instead, so this module isn't
// involved in any test path.

use super::pipeline::{PipelineBackend, PipelineEvent, run_pipeline};
use crate::llm::SamplingParams;
use crate::state::AppState;
use serde_json::Value;

/// Thin adapter that plugs a live `LlamaServerPool` + port into the
/// `PipelineBackend` trait.  This is what production calls should use —
/// the pool owns the real llama-server subprocess, the adapter routes
/// each `infer_lane_json` call to it.  Cheap to construct (holds
/// mutable borrows), so each pipeline run builds a fresh one.
pub struct PoolBackend<'a> {
    pool: &'a mut crate::llm::LlamaServerPool,
    port: u16,
}

impl<'a> PoolBackend<'a> {
    pub fn new(pool: &'a mut crate::llm::LlamaServerPool, port: u16) -> Self {
        Self { pool, port }
    }
}

impl PipelineBackend for PoolBackend<'_> {
    fn infer_lane_json(
        &mut self,
        system: &str,
        user: &str,
        schema: &Value,
        sampling: &SamplingParams,
    ) -> anyhow::Result<Value> {
        self.pool
            .infer_lane(self.port, system, user, schema, sampling)
    }
}

/// Convenience entrypoint for callers who already have a pool + port —
/// builds the adapter and runs the pipeline in one call.  Used by the
/// LLM worker loop to swap monolithic inference for the lane path.
#[allow(clippy::too_many_arguments)]
pub fn run_pipeline_via_pool(
    state: AppState,
    user_prompt: &str,
    pool: &mut crate::llm::LlamaServerPool,
    port: u16,
    sampling: &SamplingParams,
    is_jam: bool,
    live_state: Option<&std::sync::Arc<parking_lot::RwLock<AppState>>>,
    progress: impl FnMut(PipelineEvent),
    on_lane_applied: impl FnMut(&Value, &[String]),
) -> AppState {
    let mut backend = PoolBackend::new(pool, port);
    run_pipeline(
        state,
        user_prompt,
        &mut backend,
        sampling,
        is_jam,
        live_state,
        progress,
        on_lane_applied,
    )
}
