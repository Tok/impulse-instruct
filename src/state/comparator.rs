// ─── state/comparator.rs ─────────────────────────────────────────────────────
// Comparator CV utility — output 1.0 when the incoming CV is
// above `threshold`, 0.0 otherwise.  Useful for turning an LFO
// or envelope into a gate signal that drives some other
// modulation target.

use serde::{Deserialize, Serialize};

pub const COMPARATOR_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComparatorSlot {
    pub enabled: bool,
    /// Threshold 0..1.  Inputs above this value emit 1.0;
    /// inputs at or below emit 0.0.  Bipolar inputs (-1..+1)
    /// are compared against the same scalar — set the
    /// threshold to 0 for a "positive half" gate.
    pub threshold: f32,
}

impl Default for ComparatorSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 0.0,
        }
    }
}
