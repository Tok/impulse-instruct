// ─── state/cv_seq.rs ─────────────────────────────────────────────────────────
// CV sequencer module — 16-step CV pattern that drives a chosen
// `LfoTarget` parameter, advancing in lock-step with the audio
// step clock.  Distinct from `LfoSlot` (which is a continuous
// waveform: sine, saw, etc.) — the CV sequencer is a hand-drawn
// per-step value table, useful for stepped modulation patterns
// (filter sweeps that change every step, gate-like duck patterns,
// pitch transposition tables).
//
// V1 deliberately scoped:
//   * 16 step values 0..1 (compatible with LfoTarget's bipolar
//     value space — `apply_mod_target` already centres around the
//     target's natural range).
//   * Single `target: LfoTarget` per slot, exactly like an LFO
//     slot; the same opcode dispatch table picks the destination.
//   * Depth knob multiplies the step value before it reaches the
//     target.
//   * Per-block evaluation in process_block — reads the live
//     `current_step` from the sequencer and looks up
//     `step_values[step % 16]`.
//
// Cable-routed CV in / out is deferred to V2 once the modulation
// graph supports CV-sequencer sources alongside LFO sources.

use serde::{Deserialize, Serialize};

use super::LfoTarget;

/// Number of steps each CV sequencer slot holds.  16 mirrors the
/// canonical sequencer-bar grid so the CV sequence walks through
/// in time with the audio pattern.
pub const CV_SEQ_STEPS: usize = 16;

/// Number of CV sequencer slots in the engine.  Mirrors the LFO
/// slot count so the audio thread carries an identical-shape
/// fixed array on the parallel modulation path.
pub const CV_SEQ_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CvSeqSlot {
    /// Slot active.  Disabled slots cost zero per block —
    /// process_block early-outs before evaluating the step.
    pub enabled: bool,
    /// 16 step values 0..1.  Centred around 0.5 so depth knob
    /// can swing the target either side of its base value.
    pub step_values: [f32; CV_SEQ_STEPS],
    /// Mod target — same enum as the LFO target so users get a
    /// consistent dropdown across both modulation source types.
    pub target: LfoTarget,
    /// Depth multiplier 0..1.  Final mod value applied to target
    /// is `(step_value - 0.5) * 2.0 * depth` (bipolar) so a
    /// flat 0.5 step row leaves the target untouched.
    pub depth: f32,
}

impl Default for CvSeqSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            // Default: every step at 0.5 (target untouched).
            step_values: [0.5; CV_SEQ_STEPS],
            target: LfoTarget::None,
            depth: 0.3,
        }
    }
}
