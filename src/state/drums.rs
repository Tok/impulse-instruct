use serde::{Deserialize, Serialize};

// ─── Drum Kit A (808-style) ───────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KickParams {
    pub pitch: f32,           // 0–1 → 40–80 Hz base
    pub decay: f32,           // 0–1 → 0.2–2.0 s
    pub punch: f32,           // 0–1 attack transient
    pub tone: f32,            // 0–1 sine/noise blend
    pub volume: f32,          // 0–1
    pub pitch_env_depth: f32, // 0–1 → 1×–10× pitch drop height
    pub pitch_env_time: f32,  // 0–1 → 10ms–200ms pitch drop decay
}

impl Default for KickParams {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            decay: 0.6,
            punch: 0.45,
            tone: 0.8,
            volume: 0.65,
            pitch_env_depth: 0.5, // 5.5× → close to hardcoded 6×
            pitch_env_time: 0.2,  // 48ms → close to hardcoded 40ms
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnareParams {
    pub tone: f32,   // 0–1 tone freq
    pub snappy: f32, // 0–1 noise amount
    pub decay: f32,  // 0–1
    pub volume: f32,
}

impl Default for SnareParams {
    fn default() -> Self {
        Self {
            tone: 0.5,
            snappy: 0.6,
            decay: 0.4,
            volume: 0.60,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HihatParams {
    pub decay: f32, // 0–1 (open hat = higher)
    pub tone: f32,  // 0–1 filter cutoff
    pub volume: f32,
}

impl Default for HihatParams {
    fn default() -> Self {
        Self {
            decay: 0.2,
            tone: 0.7,
            volume: 0.75,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TomParams {
    pub pitch: f32,
    pub decay: f32,
    pub volume: f32,
}

impl Default for TomParams {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            decay: 0.5,
            volume: 0.7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumKit808 {
    pub kick: KickParams,
    pub snare: SnareParams,
    pub hihat_closed: HihatParams,
    pub hihat_open: HihatParams,
    pub tom_hi: TomParams,
    pub tom_mid: TomParams,
    pub tom_lo: TomParams,
}

impl Default for DrumKit808 {
    fn default() -> Self {
        Self {
            kick: KickParams::default(),
            snare: SnareParams::default(),
            hihat_closed: HihatParams {
                decay: 0.08,
                tone: 0.8,
                volume: 0.55,
            },
            hihat_open: HihatParams {
                decay: 0.4,
                tone: 0.75,
                volume: 0.55,
            },
            tom_hi: TomParams {
                pitch: 0.7,
                decay: 0.4,
                volume: 0.65,
            },
            tom_mid: TomParams {
                pitch: 0.5,
                decay: 0.45,
                volume: 0.65,
            },
            tom_lo: TomParams {
                pitch: 0.3,
                decay: 0.5,
                volume: 0.65,
            },
        }
    }
}

// ─── Drum Kit B (909-style) ───────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClapParams {
    pub decay: f32,
    pub volume: f32,
}

impl Default for ClapParams {
    fn default() -> Self {
        Self {
            decay: 0.3,
            volume: 0.8,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumKit909 {
    pub kick: KickParams,
    pub snare: SnareParams,
    pub hihat_closed: HihatParams,
    pub hihat_open: HihatParams,
    pub clap: ClapParams,
    pub rim: SnareParams, // rim shot reuses snare params
}

impl Default for DrumKit909 {
    fn default() -> Self {
        Self {
            kick: KickParams {
                pitch: 0.55,
                decay: 0.5,
                punch: 0.5,
                tone: 0.9,
                volume: 0.65,
                pitch_env_depth: 0.5,
                pitch_env_time: 0.2,
            },
            snare: SnareParams {
                tone: 0.55,
                snappy: 0.7,
                decay: 0.35,
                volume: 0.60,
            },
            hihat_closed: HihatParams {
                decay: 0.06,
                tone: 0.85,
                volume: 0.55,
            },
            hihat_open: HihatParams {
                decay: 0.45,
                tone: 0.8,
                volume: 0.55,
            },
            clap: ClapParams {
                decay: 0.3,
                volume: 0.60,
            },
            rim: SnareParams {
                tone: 0.7,
                snappy: 0.3,
                decay: 0.15,
                volume: 0.55,
            },
        }
    }
}
