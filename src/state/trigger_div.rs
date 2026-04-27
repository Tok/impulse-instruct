// ─── state/trigger_div.rs ────────────────────────────────────────────────────
// Trigger divider CV utility — fires the output gate every N input
// gates.  Distinct from `Comparator` (threshold gate, no division)
// and from the sequencer's per-step clock: this divides any incoming
// gate stream (LFO with Square waveform, CvSequencer, comparator
// output, even another TriggerDiv) by a configurable ratio.
//
// V1 ratios: /2, /3, /4, /5, /7 — covers the most common
// polyrhythmic patches (3-against-4, 5-against-4, 7-against-4) plus
// the basic halve / quarter divisions.
//
// Audio thread state (in `DspState`):
//   * `trigger_div_count: [u32; SLOTS]` — current count modulo ratio.
//   * `trigger_div_prev_input: [f32; SLOTS]` — previous input sample
//     for rising-edge detection (Schmitt-style hysteresis around 0.5).

use serde::{Deserialize, Serialize};

/// Number of trigger-divider slots in the engine.  Mirrors the LFO /
/// CvSequencer / utility slot counts — each rack instance maps to
/// one slot in rack order; the 5th instance stacks on the last slot.
pub const TRIGGER_DIV_SLOTS: usize = 4;

/// Available division ratios — fixed enum keeps the LLM schema small
/// and the audio-thread match cheap.
pub const TRIGGER_DIV_RATIOS: [u8; 5] = [2, 3, 4, 5, 7];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TriggerDivSlot {
    pub enabled: bool,
    /// Division ratio — every Nth input gate fires the output.  Stored
    /// as the raw integer (2 / 3 / 4 / 5 / 7).  Values outside this
    /// set are clamped to the nearest valid ratio at compile time.
    pub ratio: u8,
}

impl Default for TriggerDivSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            ratio: 2, // /2 — half-time gate, the most common starting point.
        }
    }
}

/// Snap an arbitrary u8 to the nearest valid ratio in `TRIGGER_DIV_RATIOS`.
/// Used by the LLM apply path + the params snapshot to keep the audio
/// thread's match arms exhaustive.
pub fn nearest_trigger_div_ratio(r: u8) -> u8 {
    *TRIGGER_DIV_RATIOS
        .iter()
        .min_by_key(|&&v| (v as i16 - r as i16).abs())
        .unwrap_or(&2)
}
