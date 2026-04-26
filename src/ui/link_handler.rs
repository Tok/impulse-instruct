// ─── ui/link_handler.rs ──────────────────────────────────────────────────────
// Per-frame Ableton Link sync.  Called once per UI tick from
// `app_update::update`.  Four flows:
//
//   1. **Pull tempo from network**: if Link reports a different BPM
//      than ours, update `state.sequencer.bpm`.  We remember the
//      value we wrote (`last_link_bpm`) so the next iteration knows
//      the change came from us-pulling, not the user.
//
//   2. **Push tempo from user**: if the local BPM differs from
//      `last_link_bpm` AND from the network's current value, the
//      user (or LLM, or MIDI clock) changed it — push our value
//      to the network so peers follow.
//
//   3. **Bar-phase snap on enable** (V2): on the off→on transition
//      of the Link toggle, ask the network for its current beat-
//      phase within `LINK_QUANTUM` and push an `AudioCommand::SnapClock`
//      so our sequencer's `current_step` lands on the corresponding
//      bar-relative step.  One-shot.
//
//   4. **Continuous bar-phase drift correction** (V2.1): even after
//      the initial snap, the local clock can drift from the network
//      over a long session — audio scheduling jitter, paused
//      transports, fine-grained phase adjustments Link makes that
//      pure BPM tracking misses.  This loop watches the network
//      phase against our local `current_step` and re-snaps when the
//      drift exceeds `DRIFT_TOLERANCE_STEPS` (≈ 1 step at the
//      current grid).  Rate-limited to one re-snap per
//      `DRIFT_CHECK_INTERVAL` so a noisy source can't thrash the
//      sequencer; large drifts (> half a bar) bypass the rate limit
//      because they're catastrophic — stop+start, network reset, a
//      late-joining peer with a wildly different clock — and
//      should be corrected immediately.
//
// Stays cheap when Link is disabled — both paths early-return on
// the `is_enabled()` flag.  When the `link` cargo feature is off
// the underlying calls are no-ops anyway, so this code compiles
// and runs fine without the C++ toolchain.

use super::ImpulseApp;
use crate::audio::AudioCommand;
use crate::sync::LINK_QUANTUM;

/// Min drift (in steps at the current grid) before the continuous
/// loop fires a re-snap.  At default 16th-note grid + 120 BPM,
/// 1 step ≈ 125 ms — small enough to be inaudible-ish, big enough
/// to be above the sample-clock + audio-buffer jitter floor that
/// would otherwise cause perpetual chasing.
const DRIFT_TOLERANCE_STEPS: i64 = 1;

/// Min wall-clock interval between consecutive drift re-snaps.
/// Bounds the worst-case "thrash on a noisy source" scenario at
/// ~one snap per second, while still letting moderate drift catch
/// up within a few seconds of accumulating.
const DRIFT_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000);

impl ImpulseApp {
    /// Drive one cycle of the Link bidirectional tempo sync.  Idempotent
    /// — safe to call every frame.
    pub(super) fn tick_link_sync(&mut self) {
        // First ensure the Link participation matches the user's pref.
        // Toggling enable() is cheap; the inner stub is a no-op when
        // the feature is off.
        let was_enabled = self.link_sync.is_enabled();
        let pref_enabled = self.state.read().ui_prefs.link_enabled;
        if pref_enabled != was_enabled {
            self.link_sync.enable(pref_enabled);
            // Reset the last-pulled BPM so the next pull / push edge
            // doesn't see stale state from a previous session.
            self.last_link_bpm = 0.0;
        }
        if !self.link_sync.is_enabled() {
            return;
        }
        // Off→on transition: snap our sequencer clock to the network's
        // bar phase.  `pull_phase` returns None on the stub build or
        // when Link is disabled, so this is a no-op without the
        // network — preserving the pre-V2 behaviour for non-Link users.
        let just_enabled = !was_enabled && pref_enabled;
        if just_enabled {
            self.snap_clock_to_link_phase();
            // Treat the off→on snap as the most recent re-snap so
            // the drift loop below doesn't double-snap on the very
            // next tick (rate limit + just-snapped both want this).
            self.last_link_drift_resnap = Some(std::time::Instant::now());
        } else {
            // Continuous bar-phase drift correction.  Only checks
            // when we *didn't* just enable (the off→on path already
            // resnapped above); the rate limiter inside the helper
            // bounds the per-second re-snap count.
            self.tick_link_drift_correction();
        }

        // Snapshot local BPM once — both branches read it.
        let local_bpm = self.state.read().sequencer.bpm;

        // Pull: if the network has a different tempo, write to AppState.
        // The `pull_tempo` helper returns None when the values match
        // within 0.01 BPM, so this only fires when there's a real
        // change.
        if let Some(net_bpm) = self.link_sync.pull_tempo(local_bpm) {
            self.state.write().sequencer.bpm = net_bpm;
            self.last_link_bpm = net_bpm;
            // BPM is part of AudioParams — push so the audio thread
            // sees the new tempo on the next block.
            self.push_audio_params();
            return;
        }

        // Push: the user / LLM / MIDI changed our BPM.  Detect by
        // comparing against `last_link_bpm` (the value we wrote on
        // the last pull).  Tolerance avoids float-jitter loops.
        if (local_bpm - self.last_link_bpm).abs() > 0.05 {
            self.link_sync.push_tempo(local_bpm);
            self.last_link_bpm = local_bpm;
        }
    }

    /// Send an `AudioCommand::SnapClock` that aligns our sequencer's
    /// `current_step` to the network's current bar phase.  Mapping is
    /// `target_step = phase_in_beats * step_division` — with the
    /// default 16th-note grid (step_division = 4) and a 4-beat
    /// quantum, this lands the snap on one of 16 bar-relative steps.
    /// No-op (returns early) when the network has no phase
    /// information to offer (stub build or disabled).
    fn snap_clock_to_link_phase(&mut self) {
        let Some(phase) = self.link_sync.pull_phase(LINK_QUANTUM) else {
            return;
        };
        let step_division = self.state.read().sequencer.step_division.max(1);
        let target_step = link_phase_to_step(phase, step_division);
        let _ = self
            .audio_tx
            .push(AudioCommand::SnapClock { step: target_step });
    }

    /// One pass of the continuous drift correction loop.  Compares
    /// our local sequencer step against the network's expected
    /// bar-relative step; pushes a re-snap when drift exceeds
    /// `DRIFT_TOLERANCE_STEPS`, subject to the rate limiter
    /// (`DRIFT_CHECK_INTERVAL`).  Catastrophic drift (more than
    /// half a bar) bypasses the rate limiter so a paused-then-
    /// resumed sequencer doesn't have to wait a full second to
    /// catch up.
    fn tick_link_drift_correction(&mut self) {
        let Some(phase) = self.link_sync.pull_phase(LINK_QUANTUM) else {
            return;
        };
        let (step_division, local_step, running) = {
            let s = self.state.read();
            (
                s.sequencer.step_division.max(1),
                s.sequencer.current_step,
                s.sequencer.running,
            )
        };
        // Only correct drift while the sequencer is running —
        // when it's stopped the local step is frozen and would
        // always look "drifted."
        if !running {
            return;
        }
        let now = std::time::Instant::now();
        let bar_steps = (step_division as f64 * LINK_QUANTUM) as usize;
        let expected = link_phase_to_step(phase, step_division);
        let local_bar_step = if bar_steps > 0 {
            local_step % bar_steps
        } else {
            0
        };
        let Some(target) = drift_resnap_target(
            local_bar_step,
            expected,
            bar_steps,
            DRIFT_TOLERANCE_STEPS,
            self.last_link_drift_resnap.map(|t| now.duration_since(t)),
            DRIFT_CHECK_INTERVAL,
        ) else {
            return;
        };
        let _ = self.audio_tx.push(AudioCommand::SnapClock { step: target });
        self.last_link_drift_resnap = Some(now);
    }
}

/// Pure drift-correction policy.  Returns `Some(target_step)` when
/// the caller should push an `AudioCommand::SnapClock`, `None`
/// otherwise.  Pulled out as a free function so the policy
/// (tolerance, rate limit, catastrophic-drift bypass) is
/// unit-testable without a Link session or AppState.
///
/// `local_bar_step` and `expected_bar_step` are both `0..bar_steps`.
/// `since_last_resnap` is `None` on the first ever check (no rate
/// limit applies); `Some(d)` afterwards.  `min_interval` is the
/// rate-limit window for non-catastrophic drifts.  Catastrophic
/// drift = more than half a bar — the shortest-path distance
/// already encodes "wrap to the closer side", so any drift
/// magnitude past `bar_steps / 2` bypasses the rate limiter.
pub fn drift_resnap_target(
    local_bar_step: usize,
    expected_bar_step: usize,
    bar_steps: usize,
    tolerance_steps: i64,
    since_last_resnap: Option<std::time::Duration>,
    min_interval: std::time::Duration,
) -> Option<usize> {
    if bar_steps == 0 {
        return None;
    }
    // Shortest-path drift on the circular bar: if local is ahead
    // of expected by more than half a bar, pulling backward is
    // shorter than pushing forward, so we represent drift as
    // signed in `[-bar_steps/2, +bar_steps/2]`.
    let bar_i = bar_steps as i64;
    let raw = local_bar_step as i64 - expected_bar_step as i64;
    let drift = ((raw + bar_i / 2).rem_euclid(bar_i)) - bar_i / 2;
    let abs_drift = drift.abs();
    if abs_drift <= tolerance_steps {
        return None;
    }
    // Catastrophic drift bypasses the rate limit.  "Catastrophic"
    // = more than a quarter of a bar.  Anything past a half bar
    // can't actually happen on this metric (shortest-path), so
    // the threshold sits comfortably below that ceiling.
    let catastrophic = abs_drift > (bar_i / 4).max(2);
    if !catastrophic
        && let Some(elapsed) = since_last_resnap
        && elapsed < min_interval
    {
        return None;
    }
    Some(expected_bar_step)
}

/// Pure mapping `phase ∈ [0, LINK_QUANTUM) → step index` using the
/// sequencer's `step_division` (steps per beat).  Pulled out as a
/// free function so the math is testable without spinning up an
/// audio engine or Link session.  The result is wrapped to a single
/// bar (`step_division * LINK_QUANTUM` steps) — the sequencer
/// itself is polymeter-aware and wraps each voice's pattern modulo
/// its own length, so we don't need to know per-voice pattern lengths
/// here.
pub fn link_phase_to_step(phase: f64, step_division: u8) -> usize {
    let div = step_division.max(1) as f64;
    let bar_steps = (div * LINK_QUANTUM) as usize;
    if bar_steps == 0 {
        return 0;
    }
    let raw = (phase * div).floor() as i64;
    // Defensive modulo: phase is supposed to land in [0, quantum),
    // but a malformed source could send anything — wrap into the
    // bar regardless.
    raw.rem_euclid(bar_steps as i64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_phase_to_step_default_grid_lands_on_bar_relative_step() {
        // step_division = 4 (16th-note grid) and quantum = 4 beats
        // → 16 steps per bar.  Phase 0 → step 0; phase 1 (one beat
        // in) → step 4; phase 3.5 → step 14.
        assert_eq!(link_phase_to_step(0.0, 4), 0);
        assert_eq!(link_phase_to_step(1.0, 4), 4);
        assert_eq!(link_phase_to_step(2.0, 4), 8);
        assert_eq!(link_phase_to_step(3.5, 4), 14);
    }

    #[test]
    fn link_phase_to_step_eighth_grid() {
        // step_division = 2 (8th-note grid) → 8 steps per bar.
        assert_eq!(link_phase_to_step(0.0, 2), 0);
        assert_eq!(link_phase_to_step(1.0, 2), 2);
        assert_eq!(link_phase_to_step(3.5, 2), 7);
    }

    #[test]
    fn link_phase_to_step_thirtysecond_grid() {
        // step_division = 8 (32nd-note grid) → 32 steps per bar.
        assert_eq!(link_phase_to_step(0.0, 8), 0);
        assert_eq!(link_phase_to_step(2.0, 8), 16);
        assert_eq!(link_phase_to_step(3.999, 8), 31);
    }

    #[test]
    fn link_phase_to_step_clamps_pathological_phase() {
        // Phases outside [0, quantum) shouldn't blow up — we want
        // to wrap them into the bar.  Negative phase via rem_euclid.
        assert_eq!(link_phase_to_step(-0.5, 4), link_phase_to_step(3.5, 4));
        assert_eq!(link_phase_to_step(4.0, 4), 0); // exactly quantum → bar 0
        assert_eq!(link_phase_to_step(5.0, 4), 4); // beyond quantum, wraps
    }

    #[test]
    fn link_phase_to_step_zero_division_doesnt_panic() {
        // step_division clamps to 1 internally — defensive against
        // a SequencerState that hasn't been initialised yet.
        assert_eq!(link_phase_to_step(0.5, 0), 0);
        assert_eq!(link_phase_to_step(2.0, 0), 2);
    }

    use std::time::Duration;

    fn one_sec() -> Duration {
        Duration::from_millis(1000)
    }

    #[test]
    fn drift_within_tolerance_returns_none() {
        // Local at step 4, expected at step 4 — perfectly aligned,
        // no drift.  Then off by exactly the tolerance — still no
        // re-snap (the boundary is "<=").
        let bar = 16;
        assert_eq!(
            drift_resnap_target(4, 4, bar, 1, Some(Duration::ZERO), one_sec()),
            None,
            "zero drift → no re-snap"
        );
        assert_eq!(
            drift_resnap_target(5, 4, bar, 1, Some(Duration::ZERO), one_sec()),
            None,
            "drift exactly at tolerance → no re-snap"
        );
    }

    #[test]
    fn drift_past_tolerance_re_snaps_when_rate_limit_clears() {
        // 3 steps of drift, well past tolerance.  Last snap was
        // 2 seconds ago → past the 1-second rate limit, so we snap.
        let bar = 16;
        let result = drift_resnap_target(10, 7, bar, 1, Some(Duration::from_secs(2)), one_sec());
        assert_eq!(result, Some(7), "re-snap to expected step");
    }

    #[test]
    fn moderate_drift_blocked_by_rate_limit() {
        // 3 steps of drift.  Last snap was 100 ms ago — inside
        // the 1-second rate-limit window — so suppress.
        let bar = 16;
        let result =
            drift_resnap_target(10, 7, bar, 1, Some(Duration::from_millis(100)), one_sec());
        assert_eq!(result, None, "rate limit suppresses moderate drift");
    }

    #[test]
    fn catastrophic_drift_bypasses_rate_limit() {
        // 6 steps of drift on a 16-step bar — past `bar/4` so it
        // counts as catastrophic.  Last snap 100 ms ago shouldn't
        // matter; we snap anyway because something dramatic
        // happened (paused transport, late peer, etc.).
        let bar = 16;
        let result =
            drift_resnap_target(12, 6, bar, 1, Some(Duration::from_millis(100)), one_sec());
        assert_eq!(
            result,
            Some(6),
            "catastrophic drift snaps despite rate limit"
        );
    }

    #[test]
    fn drift_takes_shortest_path_around_bar_wrap() {
        // Local at step 1, expected at step 15 on a 16-step bar.
        // Going forward = 14 steps; going backward = 2 steps.
        // Shortest path is "back 2", which exceeds tolerance, so
        // we re-snap.
        let bar = 16;
        let result = drift_resnap_target(1, 15, bar, 1, Some(Duration::from_secs(2)), one_sec());
        assert_eq!(
            result,
            Some(15),
            "wrap-around drift uses shortest-path metric"
        );
    }

    #[test]
    fn drift_first_check_has_no_rate_limit() {
        // `since_last_resnap = None` — never snapped before, so
        // the rate limiter doesn't apply and any non-tolerance
        // drift fires immediately.
        let bar = 16;
        let result = drift_resnap_target(10, 7, bar, 1, None, one_sec());
        assert_eq!(result, Some(7), "first ever check has no rate limit");
    }

    #[test]
    fn drift_zero_bar_steps_doesnt_panic() {
        // Defensive — if the sequencer hasn't initialised
        // properly, bar_steps could be 0.  Should return None
        // rather than divide by zero.
        let result = drift_resnap_target(0, 0, 0, 1, None, one_sec());
        assert_eq!(result, None);
    }
}
