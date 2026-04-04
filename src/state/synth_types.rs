// ─── state/synth_types.rs ─────────────────────────────────────────────────────
// Shared oscillator and filter enums used across multiple voice types.

use serde::{Deserialize, Serialize};

/// Oscillator waveform.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Waveform {
    Saw,
    Square,
    Supersaw, // detuned unison saws
}

/// Filter mode selector — shared by bass, AN1X, and any future voiced filters.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Copy)]
pub enum FilterMode {
    Lowpass,
    Highpass,
    Bandpass,
}
