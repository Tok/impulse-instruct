// ─── state/an1x.rs ───────────────────────────────────────────────────────────
// AN1X-style virtual analog voice �� BoC / warm VA aesthetic.
//
// Architecture: OSC1 + OSC2 → mix → Chamberlin SVF filter → amplitude envelope.
// Designed for Boards of Canada-style pads, leads, and melodic sequences:
// warm, slightly detuned, drifting, subtly wrong.

use crate::state::lfo::LfoWaveform;
use crate::state::synth_types::FilterMode;
use serde::{Deserialize, Serialize};

/// Per-oscillator waveform within the AN1X voice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum An1xWave {
    #[default]
    Saw,
    Square,
    Triangle,
    Sine,
    Noise,
}

/// LFO modulation target for the AN1X voice LFO.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum An1xLfoTarget {
    #[default]
    Pitch,
    FilterCutoff,
    Amplitude,
}

impl An1xLfoTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pitch => "PITCH",
            Self::FilterCutoff => "CUTOFF",
            Self::Amplitude => "AMP",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Pitch => Self::FilterCutoff,
            Self::FilterCutoff => Self::Amplitude,
            Self::Amplitude => Self::Pitch,
        }
    }
}

/// Parameters for the AN1X-style virtual analog synthesizer voice.
///
/// Two independent oscillators are mixed, then fed through a Chamberlin SVF
/// filter with full ADSR envelopes on both filter and amplitude.  A single
/// slow LFO, pitch drift, and glide complete the BoC sound palette.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct An1xState {
    pub enabled: bool,
    pub volume: f32,

    // ── Oscillators ────────────────────────────────────────────────────────
    pub osc1_wave: An1xWave,
    pub osc1_level: f32, // 0–1
    pub osc2_wave: An1xWave,
    pub osc2_level: f32, // 0–1
    /// OSC2 detune in semitones relative to OSC1. 0.5 = unison; 0..1 maps to -24..+24 st.
    pub osc2_detune: f32, // 0–1 (0.5 = unison)
    /// OSC2 octave offset (-2..+2) relative to OSC1.
    pub osc2_octave: i8,
    pub sub_level: f32, // 0–1 square wave one octave below OSC1
    pub ring_mod: bool, // mix OSC1 × OSC2 into signal

    // ── Filter ─────────────────────────────────────────────────────────────
    pub filter_cutoff: f32,    // 0–1 → 80–18 000 Hz
    pub filter_resonance: f32, // 0–1
    pub filter_mode: FilterMode,
    pub filter_key_track: f32, // 0–1 → 0%, 50%, 100% key tracking
    /// Filter envelope amount. 0–1, where 0.5 = zero, <0.5 = negative polarity.
    pub filter_env_amount: f32,
    pub filter_attack: f32,  // 0–1 → 1ms–8 s
    pub filter_decay: f32,   // 0–1 → 1ms–8 s
    pub filter_sustain: f32, // 0–1 sustain level
    pub filter_release: f32, // 0–1 → 1ms–8 s

    // ── Amplitude envelope (ADSR) ──────────────────────────────────────────
    pub amp_attack: f32,  // 0–1 → 1ms–8 s
    pub amp_decay: f32,   // 0–1 → 1ms–8 s
    pub amp_sustain: f32, // 0–1
    pub amp_release: f32, // 0–1 → 1ms–8 s

    // ── LFO (single, slow BoC-style) ───────────────────────────────────────
    pub lfo_rate: f32,  // 0–1 → 0.01–20 Hz
    pub lfo_depth: f32, // 0–1
    pub lfo_waveform: LfoWaveform,
    pub lfo_target: An1xLfoTarget,
    /// Fade-in time before LFO reaches full depth. 0–1 → 0–4 s.
    pub lfo_delay: f32,

    // ── Texture ────────────────────────────────────────────────────────────
    /// Pitch instability depth (random micro-LFO). 0–1 → 0–0.15 semitones.
    pub drift: f32,
    /// Glide time (exponential pitch glide on note change). 0–1 → 0–500 ms.
    pub glide_time: f32,
}

impl Default for An1xState {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.75,
            osc1_wave: An1xWave::Saw,
            osc1_level: 0.8,
            osc2_wave: An1xWave::Saw,
            osc2_level: 0.6,
            osc2_detune: 0.52, // slight detune above unison → gentle chorusing
            osc2_octave: 0,
            sub_level: 0.0,
            ring_mod: false,
            filter_cutoff: 0.45,
            filter_resonance: 0.30,
            filter_mode: FilterMode::Lowpass,
            filter_key_track: 0.5,
            filter_env_amount: 0.55, // slight positive envelope mod
            filter_attack: 0.08,
            filter_decay: 0.42,
            filter_sustain: 0.28,
            filter_release: 0.35,
            amp_attack: 0.08,
            amp_decay: 0.5,
            amp_sustain: 0.6,
            amp_release: 0.4,
            lfo_rate: 0.09, // ~0.12 Hz — very slow BoC breathing
            lfo_depth: 0.15,
            lfo_waveform: LfoWaveform::Sine,
            lfo_target: An1xLfoTarget::Pitch,
            lfo_delay: 0.3,
            drift: 0.12, // subtle pitch imperfection
            glide_time: 0.2,
        }
    }
}
