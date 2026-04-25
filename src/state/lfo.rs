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
    // Bass voice
    BassCutoff,
    BassResonance,
    BassPitch,
    BassVolume,
    BassPan,
    // Hoover voice
    HooverPan,
    // Noise voice
    NoisePan,
    // Drum voices — 808
    Kick808Pitch,
    Kick808Decay,
    Kick808Pan,
    Snare808Tone,
    Snare808Decay,
    Snare808Pan,
    Hihat808Pan,
    // Drum voices — 909
    Kick909Pitch,
    Kick909Decay,
    Kick909Pan,
    Snare909Tone,
    Snare909Decay,
    Snare909Pan,
    Hihat909Pan,
    Clap909Decay,
    Clap909Pan,
    // AN1X
    An1xCutoff,
    An1xPitch,
    An1xPan,
    // Sampler / texture / TTS voices
    AmenVolume,
    AmenStart,
    AmenGate,
    GranularVolume,
    GranularDensity,
    GranularGrain,
    GranularPos,
    /// NeuTts output bus volume — scales the TTS ring-buffer signal
    /// before FX / master mixing.  Modulating this lets agents fade
    /// their voice in/out under LFO control.
    NeuTtsVolume,
    // FX: Reverb
    ReverbMix,
    ReverbSize,
    ReverbDamp,
    // FX: Delay
    DelayTime,
    DelayFeedback,
    DelayMix,
    // FX: Chorus
    ChorusRate,
    ChorusDepth,
    ChorusMix,
    // FX: Phaser
    PhaserRate,
    PhaserDepth,
    PhaserMix,
    // FX: Flanger
    FlangerRate,
    FlangerDepth,
    FlangerFeedback,
    FlangerMix,
    // FX: Limiter
    LimiterThreshold,
    LimiterCeiling,
    LimiterRelease,
    LimiterLookahead,
    // FX: State-variable filter (FxFilter module)
    SvfCutoff,
    SvfResonance,
    SvfDrive,
    SvfMix,
    // FX: Comb resonator
    CombPitch,
    CombFeedback,
    CombDamp,
    CombMix,
    // FX: Tilt EQ
    TiltTilt,
    TiltPivot,
    TiltMix,
    // FX: Transient designer
    TransientAttack,
    TransientSustain,
    TransientMix,
    // FX: Exciter
    ExciterAmount,
    ExciterFreq,
    ExciterMix,
    // FX: Multitap delay
    MultitapTime,
    MultitapSpread,
    MultitapFeedback,
    MultitapMix,
    // FX: Reverse delay
    RevDelayTime,
    RevDelayFeedback,
    RevDelayMix,
    // FX: Tape stop
    TapeStopMix,
    // FX: Stutter
    StutterRate,
    StutterSlice,
    StutterMix,
    // FX: Spectral freezer
    FreezeMix,
    // FX: Waveshaper (pre-FX soft clipper)
    WaveshaperDrive,
    WaveshaperMix,
    // FX: Drive (distortion)
    DistortionDrive,
    DistortionMix,
    // FX: Bitcrush
    BitcrushBits,
    BitcrushRate,
    BitcrushMix,
    // FX: Ring Mod
    RingModFreq,
    RingModMix,
    // FX: EQ
    EqLow,
    EqMid,
    EqHigh,
    // FX: Compressor
    CompThresh,
    CompRatio,
    CompMix,
    // FX: Gate (sidechain ducker)
    GateThreshold,
    GateAttack,
    GateRelease,
    GateDepth,
    GateMix,
    // FX: Vocoder (sidechain modulator)
    VocoderBands,
    VocoderCarrierMix,
    VocoderSense,
    VocoderMix,
    // FX: Widener (master-stage Haas + side-scaling)
    WidenHaas,
    WidenSide,
    WidenMix,
    // FX: Tape Sat
    TapeDrive,
    TapeMix,
    TapeFlutter,
    // FX: Autotune
    AutotuneAmount,
    AutotuneMix,
    // Gabber kick (dedicated hardcore voice)
    GabberKickPitch,
    GabberKickDecay,
    GabberKickClip,
    GabberKickPan,
    // Master
    MasterVolume,
    StereoWidth,
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
