use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
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
