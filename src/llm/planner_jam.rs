// ─── llm/planner_jam.rs ───────────────────────────────────────────────────────
// Jam-cycle planner: retry-first + Phase 2 weighted single-lane picker.
// Split out of `planner.rs` to keep that file under the crate-wide 1000-line
// cap; the LLM-side planner (prompt/parse/heuristic/default) lives there,
// everything the jam path needs lives here.

use crate::llm::lane_scheduler::Xorshift32;
use crate::llm::lanes::LaneKind;
use crate::llm::planner::{LanePlan, lane_from_label, lane_is_live_pub};
use crate::state::AppState;

/// Single-lane jam-cycle picker.  Consults `LlmState.retry_queue` first
/// (Phase 3): any head that's still live on the rack pops off and wraps
/// as a single-lane plan, giving a bad-score lane a deterministic do-over
/// before the weighted sampler takes over.  Otherwise runs the Phase 2
/// weighted picker over `live_lane_candidates`.  Returns an empty plan
/// when nothing is eligible; caller should fall back to `default_plan`.
///
/// Scheduler biases: high lane_score → picked less often (so good bass
/// "lives" while kit/fx rotate); just-rewritten → picked less often
/// (recency decay); heat raises per-candidate jitter.
///
/// Dead retry entries (lane's module left the rack since the score fired)
/// are discarded, not returned — the rack is authoritative over the queue.
pub fn jam_plan(state: &AppState, rng: &mut Xorshift32) -> LanePlan {
    if let Some((lane, drained)) = pop_live_retry(state) {
        log::info!(
            "retry_queue: popped {} for retry (drained {} stale entries, {} remaining)",
            lane.label(),
            drained,
            state.llm.retry_queue.len().saturating_sub(drained + 1),
        );
        return LanePlan {
            lanes: vec![lane],
            rationale: format!(
                "phase-3 retry: {} (score was ≤ {:.2})",
                lane.label(),
                crate::llm::lane_eval::RETRY_THRESHOLD,
            ),
            from_retry: true,
        };
    }
    let candidates = live_lane_candidates(state);
    match crate::llm::lane_scheduler::pick_jam_lane(state, &candidates, rng) {
        Some(lane) => LanePlan {
            lanes: vec![lane],
            rationale: format!(
                "phase-2 weighted pick: {} (from {} candidates, cycle {})",
                lane.label(),
                candidates.len(),
                state.llm.jam_cycle_count,
            ),
            from_retry: false,
        },
        None => LanePlan::default(),
    }
}

/// Walk the retry queue from the front until a live lane turns up.
/// Returns `Some((lane, drained))` where `drained` is the number of dead
/// entries skipped (for logging), or `None` when the queue is exhausted
/// without a hit.  Pure lookup — callers consume the entry separately
/// via `consume_retry_prefix_mut` so the planner can stay an `&AppState`
/// read.
fn pop_live_retry(state: &AppState) -> Option<(LaneKind, usize)> {
    for (drained, label) in state.llm.retry_queue.iter().enumerate() {
        if let Some(lane) = lane_from_label(label)
            && lane_is_live_pub(state, lane)
        {
            return Some((lane, drained));
        }
    }
    None
}

/// After `jam_plan` returns a retry-originated plan, the caller must
/// remove the consumed entry (plus any dead preceding heads) from the
/// front of the queue.  Called from `pipeline_events::handle_pipeline_event`
/// on `PlanReady` with `from_retry` set, where only the `LlmState` slice
/// of `AppState` is convenient.
pub fn consume_retry_prefix_mut(llm: &mut crate::state::LlmState, lane: LaneKind) {
    // Drain every head that isn't `lane`, then drain `lane` itself.
    // Stop if the head doesn't match (defensive — the queue may have
    // been mutated between the plan build and this call).
    while let Some(front) = llm.retry_queue.front() {
        let matches = lane_from_label(front).map(|l| l == lane).unwrap_or(false);
        llm.retry_queue.pop_front();
        if matches {
            break;
        }
    }
}

/// Every lane that is currently live on the state (module wired + reaches
/// master + voice enabled).  `Settings` + `Fx` + `Modulation` are always
/// live; `Rack` is excluded because it's scheduler-ineligible (user-owned).
pub fn live_lane_candidates(state: &AppState) -> Vec<LaneKind> {
    let mut lanes: Vec<LaneKind> = vec![
        LaneKind::Settings,
        LaneKind::KitA,
        LaneKind::KitB,
        LaneKind::Amen,
        LaneKind::Bass(0),
        LaneKind::Bass(1),
        LaneKind::Bass(2),
        LaneKind::Bass(3),
        LaneKind::Hoover,
        LaneKind::An1x,
        LaneKind::Fx,
        LaneKind::Modulation,
    ];
    lanes.retain(|&l| lane_is_live_pub(state, l));
    lanes
}
