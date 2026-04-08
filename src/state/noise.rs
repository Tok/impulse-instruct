// ─── state/noise.rs ────────────────────────────────────────────────────────────
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoiseVoiceState {
    pub enabled: bool,
    pub volume: f32,           // 0-1
    pub color: f32,            // 0=white, 0.5=pink, 1=brown
    pub cutoff: f32,           // 0-1 → LP filter 200-20000 Hz
    pub attack: f32,           // 0-1 → 1ms–5s amplitude attack
    pub release: f32,          // 0-1 → 1ms–10s amplitude release
    pub filter_lfo_rate: f32,  // 0-1 → 0.05–10 Hz filter modulation
    pub filter_lfo_depth: f32, // 0-1 filter mod depth
    pub sh_rate: f32,          // 0-1 → 0.5–20 Hz sample-and-hold rate
    pub sh_depth: f32,         // 0-1 S&H modulation depth on filter
}

impl Default for NoiseVoiceState {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.5,
            color: 0.0,
            cutoff: 1.0,
            attack: 0.0,
            release: 0.1,
            filter_lfo_rate: 0.0,
            filter_lfo_depth: 0.0,
            sh_rate: 0.0,
            sh_depth: 0.0,
        }
    }
}
