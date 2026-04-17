use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxState {
    pub reverb_size: f32, // 0–1 room size
    pub reverb_damp: f32, // 0–1 damping
    pub reverb_mix: f32,  // 0–1 wet/dry
    #[serde(default)]
    pub reverb_gate_time: f32, // 0 = no gate; 0.01–2.0 s gate close time (gated reverb)
    #[serde(default)]
    pub reverb_freeze: bool, // true = infinite hold, tail loops indefinitely
    /// Reverb time direction: 0=FWD (normal), 1=REV (preverb — reverb of a
    /// reversed input buffer; sounds like reverb that builds INTO the hit),
    /// 2=MIRROR (sum of forward + reverse).  Reverse and mirror require a
    /// 1 s circular buffer of past input.
    #[serde(default)]
    pub reverb_dir: u8,
    /// Beat division for the REV/MIRROR rewind cycle.  0=free 1 s,
    /// 1=1/4 bar (1 beat), 2=1/2 bar, 3=1 bar, 4=2 bars.  Snaps the
    /// reverse-tap loop length to the active BPM.
    #[serde(default)]
    pub reverb_rev_quant: u8,
    pub delay_time: f32,     // 0–1 → 0–2000 ms
    pub delay_feedback: f32, // 0–1
    pub delay_mix: f32,      // 0–1 wet/dry
    /// Delay time direction: 0=FWD (echoes after the dry hit), 1=REV
    /// (anti-echoes preceding the hit, via reversed input buffer),
    /// 2=MIRROR.
    #[serde(default)]
    pub delay_dir: u8,
    /// Same beat-division snap as `reverb_rev_quant` but for the delay's
    /// reverse tap.
    #[serde(default)]
    pub delay_rev_quant: u8,
    #[serde(default)]
    pub delay_wow_flutter: f32, // 0–1 tape wow/flutter depth
    #[serde(default)]
    pub delay_saturation: f32, // 0–1 tape saturation on feedback
    pub distortion_drive: f32,     // 0–1
    pub distortion_mix: f32,       // 0–1 wet/dry
    pub compressor_threshold: f32, // 0–1 → -40–0 dB
    pub compressor_ratio: f32,     // 0–1 → 1:1–20:1
    pub compressor_mix: f32,       // 0–1 wet/dry (0 = bypassed)
    #[serde(default)]
    pub compressor_multiband: f32, // 0 = single band, >0 = 3-band (low/mid/high)
    pub master_volume: f32,        // 0–1
    #[serde(default)]
    pub stereo_width: f32, // 0–1: 0=mono, 0.5=normal, 1=wide
    #[serde(default)]
    pub tuning: u8, // 0=12-TET, 1=just intonation, 2=slendro, 3=pelog
    #[serde(default)]
    pub xmod_bass_to_an1x_pitch: f32, // 0–1 bass osc → AN1X pitch FM depth
    #[serde(default)]
    pub xmod_noise_to_filter: f32, // 0–1 noise → bass filter cutoff mod depth
    #[serde(default)]
    pub sidechain_amount: f32, // 0–1 sidechain compression depth (kick ducks bass/pad)
    #[serde(default)]
    pub sidechain_attack: f32, // 0–1 → 0.1–50 ms attack
    #[serde(default)]
    pub sidechain_release: f32, // 0–1 → 10–500 ms release
    pub tape_drive: f32,           // 0–1 saturation amount
    pub tape_mix: f32,             // 0–1 wet/dry
    pub tape_flutter: f32,         // 0–1 wow/flutter depth
    #[serde(default)]
    pub master_pitch_st: f32, // -12..+12 semitones: global pitch offset for melodic voices
    pub bitcrush_bits: f32,        // 0–1: 1.0 = full quality (bypass), 0.0 = 1-bit
    pub bitcrush_rate: f32,        // 0–1: 0.0 = no decimation, 1.0 = extreme downsampling
    pub bitcrush_mix: f32,         // 0–1: wet/dry
    pub chorus_rate: f32,          // 0–1 → 0.1–8 Hz LFO rate
    pub chorus_depth: f32,         // 0–1 modulation depth
    pub chorus_mix: f32,           // 0–1 wet/dry
    pub phaser_rate: f32,          // 0–1 → 0.05–5 Hz LFO rate
    pub phaser_depth: f32,         // 0–1 sweep depth
    pub phaser_mix: f32,           // 0–1 wet/dry
    pub waveshaper_drive: f32,     // 0–1 → soft-clip drive amount (pre-FX)
    pub waveshaper_mix: f32,       // 0–1 wet/dry
    pub ring_mod_freq: f32,        // 0–1 → 50–500 Hz carrier frequency
    pub ring_mod_mix: f32,         // 0–1 wet/dry
    pub eq_low_gain: f32,          // -1..+1 → -12..+12 dB low shelf (~200 Hz)
    pub eq_mid_gain: f32,          // -1..+1 → -12..+12 dB mid peak (~1 kHz)
    pub eq_hi_gain: f32,           // -1..+1 → -12..+12 dB high shelf (~5 kHz)
    #[serde(default)]
    pub autotune_amount: f32, // 0–1 → 0..+12 semitones upward pitch shift
    #[serde(default)]
    pub autotune_mix: f32, // 0–1 wet/dry
    /// Pan FX: centre bias (-1..+1), hard-pan at the extremes.
    #[serde(default)]
    pub fx_pan_pos: f32,
    /// Pan FX: auto-pan depth 0..1 (0 = static pan at fx_pan_pos).
    #[serde(default)]
    pub fx_pan_width: f32,
    /// Pan FX: auto-pan LFO rate 0..1 → 0.05..8 Hz.
    #[serde(default = "default_fx_pan_rate")]
    pub fx_pan_rate: f32,
}

fn default_fx_pan_rate() -> f32 {
    0.3
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            reverb_size: 0.4,
            reverb_damp: 0.5,
            reverb_mix: 0.0,
            reverb_gate_time: 0.0,
            reverb_freeze: false,
            reverb_dir: 0,
            reverb_rev_quant: 0,
            delay_time: 0.375,
            delay_feedback: 0.4,
            delay_mix: 0.0,
            delay_dir: 0,
            delay_rev_quant: 0,
            delay_wow_flutter: 0.0,
            delay_saturation: 0.0,
            distortion_drive: 0.0,
            distortion_mix: 0.0,
            compressor_threshold: 0.7,
            compressor_ratio: 0.3,
            compressor_mix: 0.0,
            compressor_multiband: 0.0,
            master_volume: 0.85,
            stereo_width: 0.5,
            tuning: 0,
            xmod_bass_to_an1x_pitch: 0.0,
            xmod_noise_to_filter: 0.0,
            sidechain_amount: 0.0,
            sidechain_attack: 0.1,
            sidechain_release: 0.3,
            tape_drive: 0.3,
            tape_mix: 0.0,
            tape_flutter: 0.2,
            master_pitch_st: 0.0,
            bitcrush_bits: 1.0,
            bitcrush_rate: 0.0,
            bitcrush_mix: 0.0,
            chorus_rate: 0.3,
            chorus_depth: 0.5,
            chorus_mix: 0.0,
            phaser_rate: 0.3,
            phaser_depth: 0.5,
            phaser_mix: 0.0,
            waveshaper_drive: 0.0,
            waveshaper_mix: 0.0,
            ring_mod_freq: 0.2,
            ring_mod_mix: 0.0,
            eq_low_gain: 0.0,
            eq_mid_gain: 0.0,
            eq_hi_gain: 0.0,
            autotune_amount: 0.0,
            autotune_mix: 0.0,
            fx_pan_pos: 0.0,
            fx_pan_width: 0.5,
            fx_pan_rate: default_fx_pan_rate(),
        }
    }
}
