// ─── state/function_gen.rs ───────────────────────────────────────────────────
// Function-generator CV utility — re-triggerable AD/AR envelope with
// curve shaping (linear / log / exp).  Maths-style: a gate-in cable
// fires the envelope on each rising edge, producing a one-shot 0..1
// envelope-out.  Distinct from `LfoModule` (free-running, no
// retriggering) and `CvSequencer` (step-table, no shaped envelope).
// Fills the "transient envelope" gap for plucks / drum sounds /
// synth attack tails when a gate signal needs an audio-rate ADSR-
// shaped CV without using a full bass voice.
//
// V1 design:
//   * Gate input on the Mod-In jack (rising-edge detected at 0.5).
//   * Knobs: attack 0..1 → 0..1 s, release 0..1 → 0..3 s,
//     curve -1..+1 (encoded as 0..1 with 0.5 = linear).
//   * Output: 0..1 envelope.  Idle stays at 0; attack rises to 1;
//     release falls back to 0.

use serde::{Deserialize, Serialize};

pub const FUNCTION_GEN_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunctionGenSlot {
    pub enabled: bool,
    /// Attack time 0..1 → 0..1 s exponential.  Snappy plucks live
    /// at the low end (0.05 ≈ 50 ms); slow swells at the high end.
    pub attack: f32,
    /// Release time 0..1 → 0..3 s.  Wider range than attack since
    /// release is what shapes the tail / decay character.
    pub release: f32,
    /// Curve shape 0..1 (knob centre 0.5 = linear; <0.5 = log /
    /// concave, >0.5 = exp / convex).  Same shape applied to both
    /// the attack and release segments.
    pub curve: f32,
}

impl Default for FunctionGenSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            attack: 0.1,  // ~100 ms — snappy pluck-style attack.
            release: 0.4, // ~1.2 s — audible decay tail.
            curve: 0.5,   // Linear.
        }
    }
}
