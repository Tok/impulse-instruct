// ─── state/hoover.rs ─────────────────────────────────────────────────────────
// Hoover lead voice parameters.

use serde::{Deserialize, Serialize};

/// Parameters for the Hoover lead voice.
///
/// The Hoover sound: supersaw oscillator → highpass filter that sweeps DOWN
/// from a high starting cutoff when a note is triggered. Heavy resonance
/// creates the signature "vacuum cleaner" sweep. Named after Human Resource
/// "Dominator" (1991).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HooverState {
    /// Gate — enable the voice in the mix.
    pub enabled: bool,
    /// Starting HP filter cutoff position (0–1 → 200–8000 Hz).
    /// The filter sweeps DOWN from here after each note trigger.
    pub filter_start: f32,
    /// Time for the HP cutoff to sweep from `filter_start` down to silence (0.1–4.0 s).
    pub sweep_time: f32,
    /// Filter resonance (0–1). High values produce the characteristic ringing sweep.
    pub resonance: f32,
    /// Supersaw detune spread in semitones (0–1).
    pub detune: f32,
    /// Number of supersaw unison voices (2–7).
    pub voices: u8,
    /// Pitch LFO rate in Hz (0–8). Adds the "wailing" character.
    pub pitch_lfo_rate: f32,
    /// Pitch LFO depth in semitones (0–2).
    pub pitch_lfo_depth: f32,
    /// Output volume (0–1).
    pub volume: f32,
}

impl Default for HooverState {
    fn default() -> Self {
        Self {
            enabled: false,
            filter_start: 0.82, // start high HP — thin, bright transient
            sweep_time: 0.55,   // ~550 ms sweep
            resonance: 0.76,    // heavy resonance = the hoover character
            detune: 0.42,       // noticeable supersaw shimmer
            voices: 5,
            pitch_lfo_rate: 1.3,   // 1.3 Hz — slow wail
            pitch_lfo_depth: 0.18, // ±0.18 semitones
            volume: 0.72,
        }
    }
}
