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

/// Apply `f` to whichever progress slot belongs to this turn — the per-agent
/// slot when `agent_id` is `Some`, otherwise the global console slot.  Both
/// are updated so the LLM console always shows the in-flight progress and
/// the agent's own card mirrors it during jam cycles.
fn with_progress(
    state: &Arc<RwLock<AppState>>,
    agent_id: Option<u32>,
    f: impl Fn(&mut PipelineProgress),
) {
    let mut s = state.write();
    if let Some(p) = s.llm.pipeline_progress.as_mut() {
        f(p);
    }
    if let Some(aid) = agent_id
        && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
        && let Some(p) = a.pipeline_progress.as_mut()
    {
        f(p);
    }
}

/// Handle one pipeline event.  Mutates progress state (global + per-agent),
/// sends UI log lines, and bumps `lanes_ran` for the caller's bookkeeping.
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
            // Initialise both progress slots — global console + per-agent
            // (when running on an agent).  Idle agents stay at None.
            let progress = PipelineProgress {
                total_lanes: plan.lanes.len(),
                lanes_done: 0,
                failed_count: 0,
                current_lane: None,
            };
            let mut s = state.write();
            s.llm.pipeline_progress = Some(progress.clone());
            if let Some(aid) = agent_id
                && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
            {
                a.pipeline_progress = Some(progress);
            }
            // Phase 3: if this plan was drawn from the retry queue, drop
            // the consumed entry (plus any stale preceding heads) from the
            // shared queue so the next cycle doesn't re-pick the same lane.
            if plan.from_retry
                && let Some(&lane) = plan.lanes.first()
            {
                crate::llm::planner_jam::consume_retry_prefix_mut(&mut s.llm, lane);
            }
            drop(s);
            let _ = output_tx.try_send(LlmOutput {
                text: format!("[plan: {} lanes — {}]", plan.lanes.len(), labels.join(", ")),
                agent_id,
                ..LlmOutput::default()
            });
        }
        PipelineEvent::LaneStarted { lane } => {
            with_progress(state, agent_id, |p| {
                p.current_lane = Some(lane.label().to_string());
            });
        }
        PipelineEvent::LaneApplied { lane, update, ms } => {
            *lanes_ran += 1;
            log::info!("pipeline: {} applied in {}ms", lane.label(), ms);
            with_progress(state, agent_id, |p| {
                p.lanes_done = p.lanes_done.saturating_add(1);
                p.current_lane = None;
            });
            // Mirror the monolithic path's per-inference output: send an
            // LlmOutput carrying the lane's JSON as `param_update` so
            // `drain_llm_outputs` renders a `"<persona>: …"` line in the
            // LLM console.  Without this, the pipeline is silent in the
            // UI log (only `[plan: …]` / `[pipeline: …]` brackets,
            // which the drain filter deliberately hides).  `is_jam` stays
            // false so the drain filter lets the line through on both
            // one-shot and jam cycles — showing per-lane activity is the
            // whole point of the agent console.
            let lane_text = format!("[{}]", lane.label());
            let _ = output_tx.try_send(LlmOutput {
                text: lane_text,
                param_update: Some(update),
                agent_id,
                ..LlmOutput::default()
            });
            // Phase 1 lane lifecycle: score the apply against the rules
            // we encode in the system prompt and stash the score on
            // state.llm.lane_scores.  Phase 2/3 read these for the
            // weighted picker + retry queue.
            let score = {
                let s = state.read();
                crate::llm::lane_eval::evaluate_lane(&s, lane)
            };
            crate::llm::lane_eval::record_lane_score(&mut state.write().llm, lane, score);
            // Lane fade-in: dip the voice's volume and ramp back up over
            // ~1 bar so the new pattern swells in instead of snapping on.
            // No-op for kits/fx/settings/mod (no single-channel volume)
            // and for voices that are locked or already near-silent.
            {
                let s_clone = state.read().clone();
                let next = crate::state::jam_tools::schedule_lane_fade_in(s_clone, lane);
                // Only llm.active_ramps changed — avoid trampling voice
                // fields that the pipeline just wrote.
                state.write().llm.active_ramps = next.llm.active_ramps;
            }
        }
        PipelineEvent::LaneFailed { lane, error } => {
            log::warn!("pipeline: {} failed — {}", lane.label(), error);
            with_progress(state, agent_id, |p| {
                p.lanes_done = p.lanes_done.saturating_add(1);
                p.failed_count = p.failed_count.saturating_add(1);
                p.current_lane = None;
            });
            let _ = output_tx.try_send(LlmOutput {
                text: format!("[{} failed: {}]", lane.label(), error),
                agent_id,
                ..LlmOutput::default()
            });
        }
        PipelineEvent::LaneSkipped { lane, reason } => {
            // Count as done (so progress still ticks) but not as failed.
            // Mid-cycle removals aren't model errors; they're user / style
            // actions the pipeline has to step around gracefully.
            log::info!("pipeline: {} skipped — {}", lane.label(), reason);
            with_progress(state, agent_id, |p| {
                p.lanes_done = p.lanes_done.saturating_add(1);
                p.current_lane = None;
            });
            let _ = output_tx.try_send(LlmOutput {
                text: format!("[{} skipped: {}]", lane.label(), reason),
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
            let mut s = state.write();
            s.llm.pipeline_progress = None;
            if let Some(aid) = agent_id
                && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == aid)
            {
                a.pipeline_progress = None;
            }
        }
    }
}
