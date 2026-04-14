use serde::{Deserialize, Serialize};

// ─── Free EG — drawable arbitrary-shape envelope ─────────────────────────────

/// An 8-step envelope with user-drawn levels, looped at a configurable period.
/// Each step level is 0–1. The envelope linearly interpolates between adjacent
/// steps and can be assigned to any LFO target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FreeEg {
    pub enabled: bool,
    /// 8 level values 0–1, one per step.
    pub values: [f32; 8],
    /// Period (0–1 → 0.5s – 32s, log-ish: actual = 0.5 * 64^period).
    pub period: f32,
    /// Modulation depth 0–1 (bipolar: 0.5 = centre / no mod).
    pub depth: f32,
    /// Target parameter.
    pub target: LfoTarget,
    /// When false, envelope runs once then holds the last value.
    pub loop_mode: bool,
}

impl Default for FreeEg {
    fn default() -> Self {
        // Default: a gentle single-arch shape (rise then fall)
        Self {
            enabled: false,
            values: [0.0, 0.25, 0.6, 0.9, 1.0, 0.7, 0.35, 0.1],
            period: 0.35, // ~2s
            depth: 0.5,
            target: LfoTarget::BassCutoff,
            loop_mode: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum LfoWaveform {
    #[default]
    Sine,
    Triangle,
    Saw,
    InvSaw,
    Square,
    SampleAndHold,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LfoTarget {
    #[default]
    None,
    BassCutoff,
    BassResonance,
    BassPitch,
    BassVolume,
    ReverbMix,
    DelayTime,
    DelayFeedback,
    ChorusMix,
    ChorusRate,
    Kick808Pitch,
    PhaserRate,
    PhaserDepth,
    DistortionDrive,
    MasterVolume,
    An1xCutoff,
    An1xPitch,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LfoSlot {
    pub enabled: bool,
    pub waveform: LfoWaveform,
    pub rate: f32,         // 0–1 → 0.01–20 Hz
    pub depth: f32,        // 0–1 bipolar mod depth
    pub phase_offset: f32, // 0–1 start phase
    pub target: LfoTarget,
}

impl Default for LfoSlot {
    fn default() -> Self {
        Self {
            enabled: false,
            waveform: LfoWaveform::Sine,
            rate: 0.2,
            depth: 0.3,
            phase_offset: 0.0,
            target: LfoTarget::None,
        }
    }
}
