// ─── llm/pipeline_events.rs ──────────────────────────────────────────────────
// Bridge between `pipeline::PipelineEvent` (per-lane streaming events from
// `run_pipeline_via_pool`) and the rest of the app:
//   1. mutate `state.llm.pipeline_progress` so the LLM console can render
//      a progress bar that ticks as each lane finishes,
//   2. log + send LlmOutput summary lines for the on-screen log.
//
// Extracted from `mod.rs` to keep that file under the 1000-line cap.

use std::sync::Arc;

use crossbeam_channel::Sender;
use parking_lot::RwLock;

use crate::llm::LlmOutput;
use crate::llm::pipeline::PipelineEvent;
use crate::state::{AppState, PipelineProgress};

/// Handle one pipeline event.  Mutates progress state, sends UI log lines,
/// and bumps `lanes_ran` for the caller's bookkeeping.
pub fn handle_pipeline_event(
    event: PipelineEvent,
    state: &Arc<RwLock<AppState>>,
    output_tx: &Sender<LlmOutput>,
    agent_id: Option<u32>,
    agent_label: &str,
    lanes_ran: &mut usize,
) {
    match event {
        PipelineEvent::PlanReady { plan } => {
            let labels: Vec<&str> = plan.lanes.iter().map(|l| l.label()).collect();
            log::info!(
                "pipeline: plan = [{}] ({})",
                labels.join(", "),
                plan.rationale
            );
            state.write().llm.pipeline_progress = Some(PipelineProgress {
                total_lanes: plan.lanes.len(),
                lanes_done: 0,
                failed_count: 0,
                current_lane: None,
            });
            let _ = output_tx.try_send(LlmOutput {
                text: format!("[plan: {} lanes — {}]", plan.lanes.len(), labels.join(", ")),
                agent_id,
                ..LlmOutput::default()
            });
        }
        PipelineEvent::LaneStarted { lane } => {
            if let Some(p) = state.write().llm.pipeline_progress.as_mut() {
                p.current_lane = Some(lane.label().to_string());
            }
        }
        PipelineEvent::LaneApplied { lane, ms, .. } => {
            *lanes_ran += 1;
            log::info!("pipeline: {} applied in {}ms", lane.label(), ms);
            if let Some(p) = state.write().llm.pipeline_progress.as_mut() {
                p.lanes_done = p.lanes_done.saturating_add(1);
                p.current_lane = None;
            }
        }
        PipelineEvent::LaneFailed { lane, error } => {
            log::warn!("pipeline: {} failed — {}", lane.label(), error);
            if let Some(p) = state.write().llm.pipeline_progress.as_mut() {
                p.lanes_done = p.lanes_done.saturating_add(1);
                p.failed_count = p.failed_count.saturating_add(1);
                p.current_lane = None;
            }
            let _ = output_tx.try_send(LlmOutput {
                text: format!("[{} failed: {}]", lane.label(), error),
                agent_id,
                ..LlmOutput::default()
            });
        }
        PipelineEvent::PipelineDone {
            total_ms,
            lanes_succeeded,
            lanes_failed,
        } => {
            log::info!(
                "pipeline: done ({} ok, {} failed, {}ms total, agent={})",
                lanes_succeeded,
                lanes_failed,
                total_ms,
                agent_label,
            );
            state.write().llm.pipeline_progress = None;
        }
    }
}
