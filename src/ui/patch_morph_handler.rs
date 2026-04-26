// ─── ui/patch_morph_handler.rs ────────────────────────────────────────────────
// AI patch morph scheduler — polled once per UI tick.  Watches
// `state.global_step_count` and fires the next LLM nudge when the
// configured bar interval has passed.  All scheduling lives here so
// the audio thread stays out of LLM business.
//
// Lifecycle:
//   1. UI / API enables a morph via `start_patch_morph` (sets
//      `state.patch_morph` fields).
//   2. Each UI tick `tick_patch_morph` runs.  When
//      `global_step_count` has advanced past `last_step_fired +
//      step_interval`, send an LLM Infer with a progress-augmented
//      prompt and bump the counters.
//   3. When `calls_done == total_calls`, deactivate.

use super::ImpulseApp;

impl ImpulseApp {
    /// Drive one cycle of the AI patch morph scheduler.  Cheap +
    /// idempotent — early-exits when no morph is in flight.
    pub(super) fn tick_patch_morph(&mut self) {
        // Snapshot the counters under a short read lock.  We don't
        // hold the lock across the LLM send to avoid contending
        // with the audio thread on the next bar boundary.
        let snap = {
            let s = self.state.read();
            if !s.patch_morph.in_progress() {
                return;
            }
            (
                s.global_step_count,
                s.patch_morph.last_step_fired,
                s.patch_morph.step_interval,
                s.patch_morph.next_nudge_prompt(),
            )
        };
        let (now, last_fired, interval, prompt) = snap;
        // Fire only when we've crossed the interval boundary.  The
        // first call fires immediately if `last_fired == 0` — that's
        // the convention `start_patch_morph` sets up so the user
        // hears the first nudge right after starting the morph.
        if now < last_fired.saturating_add(interval) {
            return;
        }
        // Update state under a write lock *before* sending so the
        // next tick can't double-fire the same step.
        {
            let mut s = self.state.write();
            s.patch_morph.last_step_fired = now;
            s.patch_morph.calls_done = s.patch_morph.calls_done.saturating_add(1);
            // Last call lands → deactivate so the next tick stops
            // polling without needing another lock.
            if s.patch_morph.calls_done >= s.patch_morph.total_calls {
                s.patch_morph.active = false;
            }
        }
        // Send via the shared helper — same path the LLM strip and
        // /api/prompt use, so the morph nudge benefits from
        // identical apply_llm_update + locked_params handling.
        self.send_llm_infer(prompt, /* one_shot = */ true, /* agent_id = */ None);
    }
}

/// Pre-compute the `step_interval` (in `global_step_count` ticks)
/// for a patch morph spanning `bars` bars at `step_division` steps
/// per beat, fired across `total_calls` LLM nudges.  Pure — extracted
/// so the math is testable without an `ImpulseApp`.
///
/// Defaults to 4 beats per bar (matches `LINK_QUANTUM` and the
/// sequencer's standard 4/4 interpretation).  Callers passing
/// `total_calls = 0` get back at least 1 step to keep the scheduler
/// from infinite-firing the same nudge.
pub fn compute_step_interval(bars: u32, step_division: u8, total_calls: u32) -> u64 {
    let div = step_division.max(1) as u64;
    let beats_per_bar: u64 = 4; // matches LINK_QUANTUM
    let total_steps = (bars as u64).max(1) * beats_per_bar * div;
    let calls = total_calls.max(1) as u64;
    (total_steps / calls).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_interval_default_grid_8_bars_8_calls_is_one_bar() {
        // 8 bars / 8 calls at 16ths = 16 steps per call (one bar).
        assert_eq!(compute_step_interval(8, 4, 8), 16);
    }

    #[test]
    fn step_interval_4_bars_2_calls_is_two_bars() {
        // 4 bars / 2 calls = 2 bars per call = 32 steps at 16ths.
        assert_eq!(compute_step_interval(4, 4, 2), 32);
    }

    #[test]
    fn step_interval_8th_grid() {
        // step_division = 2 (8th-note grid), 4 bars / 4 calls →
        // 4 * 4 * 2 = 32 total steps / 4 = 8 per call (one bar).
        assert_eq!(compute_step_interval(4, 2, 4), 8);
    }

    #[test]
    fn step_interval_clamps_pathological_inputs() {
        // 0 bars / 0 calls / 0 division shouldn't divide by zero
        // or return 0 (the scheduler would infinite-fire at 0).
        assert!(compute_step_interval(0, 0, 0) >= 1);
        assert!(compute_step_interval(8, 4, 0) >= 1);
        assert!(compute_step_interval(0, 4, 4) >= 1);
    }

    #[test]
    fn step_interval_more_calls_than_steps_returns_one() {
        // 100 calls across 1 bar at 16ths = 16 steps / 100 calls →
        // would round to 0; clamps to 1 so the scheduler still
        // fires sequentially rather than spinning.
        assert_eq!(compute_step_interval(1, 4, 100), 1);
    }
}
