// ─── state/quantizer.rs ──────────────────────────────────────────────────────
// CV quantizer utility — snap an incoming CV signal to the
// nearest scale note before passing it on.  Input is treated as
// a bipolar pitch swing (-1..+1 → -12..+12 semitones); output
// is the same bipolar value rounded to the nearest note in the
// configured scale relative to the configured root.
//
// Distinct from the audio-thread sequencer's `scale_snap` (which
// snaps LLM-provided sequencer notes); this is a CV-pipeline
// transform that any modulator can feed through.

use serde::{Deserialize, Serialize};

use super::Scale;

/// Number of quantizer slots in the engine.  Mirrors the LFO /
/// CV-seq / Slew pool.
pub const QUANTIZER_SLOTS: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantizerSlot {
    pub enabled: bool,
    /// Tonic / root note (0=C, 1=C#, ..., 11=B).  Reuses the
    /// codebase's existing `root_note` convention so a quantizer
    /// patched on top of the sequencer's running scale stays in
    /// key without manual re-tuning.
    pub root: u8,
    pub scale: Scale,
}

impl Default for QuantizerSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            root: 0,
            scale: Scale::Major,
        }
    }
}
