// ─── state/noise.rs ────────────────────────────────────────────────────────────
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoiseVoiceState {
    pub enabled: bool,
    pub volume: f32, // 0-1
    pub color: f32,  // 0=white, 0.5=pink, 1=brown
    pub cutoff: f32, // 0-1 → LP filter 200-20000 Hz
}

impl Default for NoiseVoiceState {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.5,
            color: 0.0,
            cutoff: 1.0,
        }
    }
}
