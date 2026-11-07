// ─── state/additive.rs ────────────────────────────────────────────────────────
// Additive synth voice — 16-partial harmonic series with per-
// harmonic level sliders.  Distinct from the Wavetable voice in
// shape: wavetable scans single-cycle frames (the user picks a
// pre-baked spectrum), additive lets the user *draw* the spectrum
// directly by setting each harmonic's amplitude.
//
// Sequencer-driven: a step trigger sets the fundamental from the
// played MIDI note; harmonics 1..=16 play at integer multiples of
// that frequency.  Voice-wide ADSR shapes the summed output —
// per-harmonic envelopes deferred (a follow-up could add a
// per-partial decay rate to mimic real instruments where high
// partials die first).

use serde::{Deserialize, Serialize};

/// Number of harmonics this voice exposes.  16 covers organ /
/// Hammond / harmonic-rich tones with room to spare; an upgrade
/// to 32 / 64 would only need this constant + the array sizes.
pub const ADDITIVE_HARMONICS: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdditiveState {
    pub enabled: bool,
    /// Output volume 0..1.5.
    pub volume: f32,
    /// Stereo pan -1..+1.
    #[serde(default)]
    pub pan: f32,
    /// Per-harmonic level 0..1.  Index 0 = fundamental, index N =
    /// (N+1)th harmonic (so index 1 plays at 2× the played note,
    /// index 7 at 8×, etc.).  Sum is normalised at process time so
    /// fully-pegged levels still produce a bounded output.
    pub levels: [f32; ADDITIVE_HARMONICS],
    /// Voice-wide amp envelope.  Same 0..1 knob → time mapping as
    /// the FM-ops / SAMPLER+ ADSRs so users get consistent feel.
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for AdditiveState {
    fn default() -> Self {
        // Default partial bank: a sawtooth-like 1/n falloff so a
        // freshly enabled module produces a recognisable harmonic-
        // rich tone (rather than a single sine, which would feel
        // disappointingly flat next to the other voices' defaults).
        let mut levels = [0.0_f32; ADDITIVE_HARMONICS];
        for (i, slot) in levels.iter_mut().enumerate() {
            *slot = 1.0 / ((i + 1) as f32);
        }
        Self {
            enabled: false,
            volume: 0.7,
            pan: 0.0,
            levels,
            attack: 0.0,
            decay: 0.4,
            sustain: 0.6,
            release: 0.3,
        }
    }
}
