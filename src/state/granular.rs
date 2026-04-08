// ─── state/granular.rs ───────────────────────────────────────────────────────
use serde::{Deserialize, Serialize};

/// Granular texture voice state — overlapping micro-grains from a loaded WAV.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GranularState {
    pub enabled: bool,
    pub volume: f32,          // 0–1 output level
    pub density: f32,         // 0–1 → 1–40 grains/sec
    pub grain_size: f32,      // 0–1 → 10–500 ms per grain
    pub position: f32,        // 0–1 playback position in the loaded WAV
    pub position_jitter: f32, // 0–1 random offset around position
    pub pitch_scatter: f32,   // 0–1 → ±12 semitones random per grain
    pub spray: f32,           // 0–1 stereo spread (0=mono centre, 1=full width)
    #[serde(default)]
    pub path: String, // WAV file path (empty = no sample loaded)
}

impl Default for GranularState {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.5,
            density: 0.4,    // ~10 grains/sec
            grain_size: 0.3, // ~150 ms
            position: 0.0,
            position_jitter: 0.2,
            pitch_scatter: 0.1,
            spray: 0.3,
            path: String::new(),
        }
    }
}
