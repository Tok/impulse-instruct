// ─── state/sample_hold.rs ────────────────────────────────────────────────────
// Sample-and-hold CV utility — latches the incoming CV value on
// each new sequencer step (the "clock edge"), then holds that
// value until the next step.  Distinct from the LFO's S&H
// waveform option: that one re-latches on its own LFO phase
// wrap; this one re-latches on the audio sequencer's step
// transitions, so the held value is always musically aligned
// with the bar grid.

use serde::{Deserialize, Serialize};

pub const SAMPLE_HOLD_SLOTS: usize = 4;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SampleHoldSlot {
    pub enabled: bool,
}
