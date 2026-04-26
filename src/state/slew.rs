// ─── state/slew.rs ───────────────────────────────────────────────────────────
// Slew / glide CV utility module — smooths an incoming CV signal
// with separate attack (rise) and release (fall) time constants.
// Distinct from `LfoSlot` (generates a CV) and from the bass
// voice's portamento (audio-rate pitch glide on a single voice):
// this is a *post-source* CV transform that any LFO / CV-seq /
// other utility output can feed through, then on to a synth /
// FX param via cable.
//
// V1 design:
//   * CV input: one Mod-In port resolved by cable graph compile
//   * Knobs: attack, release (each 0..1 → 0..2 s exponential time
//     constant)
//   * Audio thread: per-block one-pole smoothing with the
//     direction-appropriate coefficient.

use serde::{Deserialize, Serialize};

/// Number of slew slots in the engine.  Mirrors the LFO / CV-seq
/// slot count — each rack instance of `ModuleKind::Slew` maps to
/// one slot in rack order.
pub const SLEW_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlewSlot {
    pub enabled: bool,
    /// Attack (rise) time 0..1 → 0..2 s exponential.  The smoother
    /// chases its target with this coefficient when the input is
    /// rising; falling uses `release` instead.
    pub attack: f32,
    /// Release (fall) time 0..1 → 0..2 s.
    pub release: f32,
}

impl Default for SlewSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            attack: 0.2,
            release: 0.2,
        }
    }
}
