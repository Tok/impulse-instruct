// ─── state/crossfader.rs ─────────────────────────────────────────────────────
// Crossfader CV utility — single-knob A/B blend between two CV
// sources.  Mix knob 0 = pure A; 0.5 = 50/50; 1 = pure B.  Distinct
// from Math's Blend op (more general but with extra fields); the
// Crossfader is the dedicated A/B case with a single MIX knob,
// which is the common live-perform shape.

use serde::{Deserialize, Serialize};

pub const CROSSFADER_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrossfaderSlot {
    pub enabled: bool,
    /// Mix 0..1 — 0 = pure A, 1 = pure B.  Default 0.5 (centred).
    pub mix: f32,
}

impl Default for CrossfaderSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            mix: 0.5,
        }
    }
}
