// ─── state/hoover.rs ─────────────────────────────────────────────────────────
// Hoover lead voice parameters.

use serde::{Deserialize, Serialize};

/// Parameters for the Hoover lead voice.
///
/// The Hoover sound: supersaw oscillator → resonant lowpass filter that starts
/// wide open (bright) and sweeps DOWN as the envelope decays. High resonance
/// creates a moving resonant peak — the authentic "vacuum cleaner" sweep.
/// Named after Human Resource "Dominator" (1991).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HooverState {
    /// Gate — enable the voice in the mix.
    pub enabled: bool,
    /// Starting LP filter openness (0–1 → ~100 Hz – ~12 kHz).
    /// The filter sweeps DOWN from this point after each note trigger.
    /// High values (0.8–1.0) give a bright, wide-open start; lower values darken the transient.
    pub filter_start: f32,
    /// Time for the LP cutoff to sweep from `filter_start` down to dark/closed (0.1–4.0 s).
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
            filter_start: 0.88, // LP wide open — bright resonant start
            sweep_time: 0.85,   // ~850 ms sweep gives plenty of "oooh" travel
            resonance: 0.82,    // heavy resonance = the moving resonant peak
            detune: 0.45,       // noticeable supersaw shimmer
            voices: 5,
            pitch_lfo_rate: 1.2,   // 1.2 Hz — slow wail
            pitch_lfo_depth: 0.15, // ±0.15 semitones
            volume: 0.72,
        }
    }
}
