use serde::{Deserialize, Serialize};

#[allow(clippy::wildcard_imports)]
use super::*;

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
    /// Hard-clip drive for gabber character.
    /// 0 = clean, 1 = maximum hard clip (10× boost then clamp to ±1).
    #[serde(default)]
    pub clip: f32,
    /// Stereo pan position: -1.0 = hard left, 0.0 = center, 1.0 = hard right.
    #[serde(default)]
    pub pan: f32,
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
            clip: 0.0,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnareParams {
    pub tone: f32,   // 0–1 tone freq
    pub snappy: f32, // 0–1 noise amount
    pub decay: f32,  // 0–1
    pub volume: f32,
    /// Stereo pan position: -1.0 = hard left, 0.0 = center, 1.0 = hard right.
    #[serde(default)]
    pub pan: f32,
}

impl Default for SnareParams {
    fn default() -> Self {
        Self {
            tone: 0.5,
            snappy: 0.6,
            decay: 0.4,
            volume: 0.60,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HihatParams {
    pub decay: f32, // 0–1 (open hat = higher)
    pub tone: f32,  // 0–1 filter cutoff
    pub volume: f32,
    /// Stereo pan position: -1.0 = hard left, 0.0 = center, 1.0 = hard right.
    #[serde(default)]
    pub pan: f32,
}

impl Default for HihatParams {
    fn default() -> Self {
        Self {
            decay: 0.2,
            tone: 0.7,
            volume: 0.75,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TomParams {
    pub pitch: f32,
    pub decay: f32,
    pub volume: f32,
    /// Stereo pan position: -1.0 = hard left, 0.0 = center, 1.0 = hard right.
    #[serde(default)]
    pub pan: f32,
}

impl Default for TomParams {
    fn default() -> Self {
        Self {
            pitch: 0.5,
            decay: 0.5,
            volume: 0.7,
            pan: 0.0,
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
                pan: 0.0,
            },
            hihat_open: HihatParams {
                decay: 0.4,
                tone: 0.75,
                volume: 0.55,
                pan: 0.0,
            },
            tom_hi: TomParams {
                pitch: 0.7,
                decay: 0.4,
                volume: 0.65,
                pan: 0.0,
            },
            tom_mid: TomParams {
                pitch: 0.5,
                decay: 0.45,
                volume: 0.65,
                pan: 0.0,
            },
            tom_lo: TomParams {
                pitch: 0.3,
                decay: 0.5,
                volume: 0.65,
                pan: 0.0,
            },
        }
    }
}

// ─── Drum Kit B (909-style) ───────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClapParams {
    pub decay: f32,
    pub volume: f32,
    /// Stereo pan position: -1.0 = hard left, 0.0 = center, 1.0 = hard right.
    #[serde(default)]
    pub pan: f32,
}

impl Default for ClapParams {
    fn default() -> Self {
        Self {
            decay: 0.3,
            volume: 0.8,
            pan: 0.0,
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
                clip: 0.0,
                pan: 0.0,
            },
            snare: SnareParams {
                tone: 0.55,
                snappy: 0.7,
                decay: 0.35,
                volume: 0.60,
                pan: 0.0,
            },
            hihat_closed: HihatParams {
                decay: 0.06,
                tone: 0.85,
                volume: 0.55,
                pan: 0.0,
            },
            hihat_open: HihatParams {
                decay: 0.45,
                tone: 0.8,
                volume: 0.55,
                pan: 0.0,
            },
            clap: ClapParams {
                decay: 0.3,
                volume: 0.60,
                pan: 0.0,
            },
            rim: SnareParams {
                tone: 0.7,
                snappy: 0.3,
                decay: 0.15,
                volume: 0.55,
                pan: 0.0,
            },
        }
    }
}

// ─── DrumVoice identifier ─────────────────────────────────────────────────────

/// Identifies which drum machine voice a sequencer pattern lane controls.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DrumVoice {
    Kick808,
    Snare808,
    HihatClosed808,
    HihatOpen808,
    TomHi808,
    TomMid808,
    TomLo808,
    Kick909,
    Snare909,
    HihatClosed909,
    HihatOpen909,
    Clap909,
    Rim909,
    /// WAV sampler voice — plays the loaded amen (or any user WAV).
    Amen,
    /// Dedicated gabber-kick voice — hardcore/Rotterdam-style distorted kick
    /// with a transient click layer, tuned higher and harder than 808/909.
    GabberKick,
}

impl DrumVoice {
    pub const ALL: &'static [DrumVoice] = &[
        DrumVoice::Kick808,
        DrumVoice::Snare808,
        DrumVoice::HihatClosed808,
        DrumVoice::HihatOpen808,
        DrumVoice::TomHi808,
        DrumVoice::TomMid808,
        DrumVoice::TomLo808,
        DrumVoice::Kick909,
        DrumVoice::Snare909,
        DrumVoice::HihatClosed909,
        DrumVoice::HihatOpen909,
        DrumVoice::Clap909,
        DrumVoice::Rim909,
        DrumVoice::Amen,
        DrumVoice::GabberKick,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            DrumVoice::Kick808 => "808 KICK",
            DrumVoice::Snare808 => "808 SNARE",
            DrumVoice::HihatClosed808 => "808 CLOSED HH",
            DrumVoice::HihatOpen808 => "808 OPEN HH",
            DrumVoice::TomHi808 => "808 HI TOM",
            DrumVoice::TomMid808 => "808 MID TOM",
            DrumVoice::TomLo808 => "808 LO TOM",
            DrumVoice::Kick909 => "909 KICK",
            DrumVoice::Snare909 => "909 SNARE",
            DrumVoice::HihatClosed909 => "909 CLOSED HH",
            DrumVoice::HihatOpen909 => "909 OPEN HH",
            DrumVoice::Clap909 => "909 CLAP",
            DrumVoice::Rim909 => "909 RIM",
            DrumVoice::Amen => "AMEN",
            DrumVoice::GabberKick => "GABBER KICK",
        }
    }

    /// Canonical JSON key used in the LLM schema (`kick_a`, `snare_b`, …).
    /// Returns `None` for voices not exposed to the LLM schema.
    pub fn schema_key(self) -> Option<&'static str> {
        Some(match self {
            DrumVoice::Kick808 => "kick_a",
            DrumVoice::Snare808 => "snare_a",
            DrumVoice::HihatClosed808 => "hihat_a",
            DrumVoice::HihatOpen808 => "hihat_a_open",
            DrumVoice::Kick909 => "kick_b",
            DrumVoice::Snare909 => "snare_b",
            DrumVoice::HihatClosed909 => "hihat_b",
            DrumVoice::HihatOpen909 => "hihat_b_open",
            DrumVoice::Clap909 => "clap_b",
            DrumVoice::GabberKick => "gabber_kick",
            _ => return None,
        })
    }

    /// Which rack `ModuleKind` this drum voice belongs to.
    pub fn module_kind(self) -> crate::state::ModuleKind {
        match self {
            DrumVoice::Kick808
            | DrumVoice::Snare808
            | DrumVoice::HihatClosed808
            | DrumVoice::HihatOpen808
            | DrumVoice::TomHi808
            | DrumVoice::TomMid808
            | DrumVoice::TomLo808 => crate::state::ModuleKind::DrumKit808,
            DrumVoice::Kick909
            | DrumVoice::Snare909
            | DrumVoice::HihatClosed909
            | DrumVoice::HihatOpen909
            | DrumVoice::Clap909
            | DrumVoice::Rim909 => crate::state::ModuleKind::DrumKit909,
            DrumVoice::Amen => crate::state::ModuleKind::AmenSampler,
            DrumVoice::GabberKick => crate::state::ModuleKind::DrumKit808,
        }
    }

    pub fn get_volume(&self, s: &AppState) -> f32 {
        match self {
            DrumVoice::Kick808 => s.kit_a.kick.volume,
            DrumVoice::Snare808 => s.kit_a.snare.volume,
            DrumVoice::HihatClosed808 => s.kit_a.hihat_closed.volume,
            DrumVoice::HihatOpen808 => s.kit_a.hihat_open.volume,
            DrumVoice::TomHi808 => s.kit_a.tom_hi.volume,
            DrumVoice::TomMid808 => s.kit_a.tom_mid.volume,
            DrumVoice::TomLo808 => s.kit_a.tom_lo.volume,
            DrumVoice::Kick909 => s.kit_b.kick.volume,
            DrumVoice::Snare909 => s.kit_b.snare.volume,
            DrumVoice::HihatClosed909 => s.kit_b.hihat_closed.volume,
            DrumVoice::HihatOpen909 => s.kit_b.hihat_open.volume,
            DrumVoice::Clap909 => s.kit_b.clap.volume,
            DrumVoice::Rim909 => s.kit_b.rim.volume,
            DrumVoice::Amen => s.amen.volume,
            DrumVoice::GabberKick => s.gabber_kick.volume,
        }
    }

    pub fn set_volume(self, mut s: AppState, v: f32) -> AppState {
        match self {
            DrumVoice::Kick808 => s.kit_a.kick.volume = v,
            DrumVoice::Snare808 => s.kit_a.snare.volume = v,
            DrumVoice::HihatClosed808 => s.kit_a.hihat_closed.volume = v,
            DrumVoice::HihatOpen808 => s.kit_a.hihat_open.volume = v,
            DrumVoice::TomHi808 => s.kit_a.tom_hi.volume = v,
            DrumVoice::TomMid808 => s.kit_a.tom_mid.volume = v,
            DrumVoice::TomLo808 => s.kit_a.tom_lo.volume = v,
            DrumVoice::Kick909 => s.kit_b.kick.volume = v,
            DrumVoice::Snare909 => s.kit_b.snare.volume = v,
            DrumVoice::HihatClosed909 => s.kit_b.hihat_closed.volume = v,
            DrumVoice::HihatOpen909 => s.kit_b.hihat_open.volume = v,
            DrumVoice::Clap909 => s.kit_b.clap.volume = v,
            DrumVoice::Rim909 => s.kit_b.rim.volume = v,
            DrumVoice::Amen => s.amen.volume = v,
            DrumVoice::GabberKick => s.gabber_kick.volume = v,
        }
        s
    }
}
