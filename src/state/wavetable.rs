// ─── state/wavetable.rs ──────────────────────────────────────────────────────
// Wavetable synthesis voice — scans through a user-supplied WAV split
// into fixed 2048-sample frames, reading the selected frame at pitch
// rate.  Complements AN1X (analog model) and Hoover (fixed supersaw)
// with user-extensible waveform character.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WavetableState {
    /// Gate — enable the voice in the mix.
    pub enabled: bool,
    /// Frame position (0..1) — scans through the loaded wavetable's
    /// sequence of single-cycle frames.  Linearly interpolates between
    /// adjacent frames so sweeps morph smoothly.
    pub position: f32,
    /// Phase offset (0..1 → 0..2π) — shifts the read pointer inside
    /// the selected frame so the user can detune the cycle's start
    /// point without retriggering.
    pub phase_offset: f32,
    /// Output volume (0..1).
    pub volume: f32,
    /// Stereo pan (-1.0 = L, 0.0 = centre, 1.0 = R).
    #[serde(default)]
    pub pan: f32,
    /// Global pitch offset in semitones (-24..+24).
    #[serde(default)]
    pub pitch_offset_semi: f32,
    /// Filesystem path of the currently loaded wavetable WAV.  Empty =
    /// no wavetable loaded (voice plays silence).  The UI polls this
    /// so API-driven `/api/wavetable` writes surface in the audio
    /// path on the next frame.
    #[serde(default)]
    pub wave_path: String,
}

impl Default for WavetableState {
    fn default() -> Self {
        Self {
            enabled: false,
            position: 0.0,
            phase_offset: 0.0,
            volume: 0.7,
            pan: 0.0,
            pitch_offset_semi: 0.0,
            wave_path: String::new(),
        }
    }
}
