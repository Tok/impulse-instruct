// ─── state/vocal.rs ───────────────────────────────────────────────────────────
// Vocal formant synth — sings vowels (A / E / I / O / U) by
// passing a sawtooth source through three parallel resonant
// bandpass filters tuned to the formant frequencies of the
// selected vowel.  Distinct from `NeuTts`: that voice loads a
// neural phoneme model and synthesises actual speech; this voice
// is a pure DSP formant bank — no model, no language, just the
// resonance pattern that makes vowel sounds recognisable.
//
// The 5-preset table is for an average male speaker (taken from
// Peterson & Barney 1952, the canonical vowel-formant reference).
// A `formant_shift` knob uniformly scales every formant up or
// down so the user can move toward female / child / monster
// timbres without changing the played pitch.

use serde::{Deserialize, Serialize};

/// Number of vowel presets — A / E / I / O / U.  Must match the
/// table in `audio/dsp/vocal.rs::VOWEL_FORMANTS`.
pub const VOCAL_VOWEL_PRESETS: u8 = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VocalState {
    pub enabled: bool,
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    /// Active vowel preset index 0..=4 — A / E / I / O / U.  The
    /// `morph` knob below blends from this preset toward the
    /// next-in-cycle one so the user can hold a vowel or sweep
    /// between adjacent ones.  Clamped at apply time.
    pub vowel: u8,
    /// Morph amount 0..1 from `vowel` toward `(vowel + 1) % N`.
    /// 0 = pure preset; 1 = pure next preset; 0.5 = halfway
    /// between the two formant tables.
    pub morph: f32,
    /// Source brightness 0..1 → spectral tilt on the sawtooth
    /// source.  Low = darker source (more like a hummed vowel),
    /// high = brighter source (more like a sung vowel with
    /// strong upper harmonics for the formants to filter).
    pub brightness: f32,
    /// Formant shift 0..1 → uniform scale on F1/F2/F3 frequencies.
    /// 0.5 = no shift (male-average); 0 = ~-50 % (deeper / more
    /// ogre-like); 1 = ~+50 % (higher / more child-like).
    /// Pitch of the sung note is unchanged; only the formants
    /// move, so the apparent vocal-tract size changes.
    pub formant_shift: f32,
    /// Voice-wide ADSR — same map as the FM-ops / SAMPLER+
    /// envelopes for consistent feel.
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for VocalState {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.7,
            pan: 0.0,
            vowel: 0,           // A — open / bright by default
            morph: 0.0,         // pure A
            brightness: 0.7,    // bright source for clear formant articulation
            formant_shift: 0.5, // male-average, no shift
            attack: 0.05,       // small attack so the vowel doesn't click
            decay: 0.4,
            sustain: 0.7,
            release: 0.4,
        }
    }
}
