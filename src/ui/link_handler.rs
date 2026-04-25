// ─── ui/link_handler.rs ──────────────────────────────────────────────────────
// Per-frame Ableton Link sync.  Called once per UI tick from
// `app_update::update`.  Three flows:
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
//      bar-relative step.  One-shot — continuous drift correction
//      is a separate later step.
//
// Stays cheap when Link is disabled — both paths early-return on
// the `is_enabled()` flag.  When the `link` cargo feature is off
// the underlying calls are no-ops anyway, so this code compiles
// and runs fine without the C++ toolchain.

use super::ImpulseApp;
use crate::audio::AudioCommand;
use crate::sync::LINK_QUANTUM;

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
}
