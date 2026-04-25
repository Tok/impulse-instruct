// ─── state/sample_instrument.rs ──────────────────────────────────────────────
// Sample-Instrument voice — load a single pitched .wav and play it back
// across the keyboard via ratio resampling.  Distinct from `WavetableVoice`
// (which scans single-cycle frames) and `AmenSampler` (which plays slices
// at the original pitch): this module re-pitches the entire recording
// based on the played note vs. the root note.
//
// V1 is intentionally minimal — single sample, monophonic, simple AR
// envelope (no full ADSR yet), no loop points (always-loops the whole
// buffer), no on-load pitch detection.  The plan in PLAN.md tracks the
// V2 enhancements.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleInstrumentState {
    /// Gate — enable the voice in the mix.
    pub enabled: bool,
    /// Source-recording root note (MIDI).  Played notes are pitch-shifted
    /// relative to this: rate = 2^((played_note − root_note) / 12).
    /// Defaults to C4 (60); the user re-tunes via the UI.
    pub root_note: u8,
    /// Output volume (0..1.5; >1 boosts).
    pub volume: f32,
    /// Stereo pan (-1..+1).
    #[serde(default)]
    pub pan: f32,
    /// Fine pitch trim in cents (-100..+100), applied on top of the
    /// per-note pitch.
    #[serde(default)]
    pub pitch_offset_cents: f32,
    /// Filesystem path of the currently loaded sample.  Empty = none
    /// loaded; voice plays silence.  UI polls this so API-driven loads
    /// surface to the panel.
    #[serde(default)]
    pub sample_path: String,
}

impl Default for SampleInstrumentState {
    fn default() -> Self {
        Self {
            enabled: false,
            root_note: 60, // C4
            volume: 0.7,
            pan: 0.0,
            pitch_offset_cents: 0.0,
            sample_path: String::new(),
        }
    }
}
