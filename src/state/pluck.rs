// ─── state/pluck.rs ──────────────────────────────────────────────────────────
// Karplus-Strong plucked-string voice parameters.

use serde::{Deserialize, Serialize};

/// Parameters for the plucked-string voice.
///
/// The Karplus-Strong algorithm: a ring delay line of length
/// `sr / freq` is primed with white noise on each trigger, and the
/// feedback path applies a gentle lowpass so each pass through the
/// line dulls the spectrum a bit.  `damping` sets how fast the tone
/// decays (≈how much the feedback lowpass cuts per iteration),
/// `brightness` controls a one-pole LP on the output tap so the
/// voice can be tamed for dry acoustic textures without changing
/// the decay character.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluckState {
    /// Gate — enable the voice in the mix.
    pub enabled: bool,
    /// Decay / sustain length (0–1).  Low values fade fast; high
    /// values sustain for seconds.  Internally maps to a feedback
    /// coefficient close to 1.0.
    pub damping: f32,
    /// Output brightness — one-pole lowpass cutoff (0 = very dark,
    /// 1 = wide open).  Independent of the feedback damping so users
    /// can dial the raw pluck's edge without shortening the tail.
    pub brightness: f32,
    /// Output volume (0–1).
    pub volume: f32,
    /// Stereo pan (-1.0 = L, 0.0 = centre, 1.0 = R).
    #[serde(default)]
    pub pan: f32,
    /// Global pitch offset in semitones (-24..+24) — lets the user
    /// transpose the sequencer pattern without retyping every note.
    #[serde(default)]
    pub pitch_offset_semi: f32,
}

impl Default for PluckState {
    fn default() -> Self {
        Self {
            enabled: false,
            damping: 0.85,   // ~1 s decay at 440 Hz — musical default
            brightness: 0.7, // mildly tamed edge, still audibly bright
            volume: 0.7,
            pan: 0.0,
            pitch_offset_semi: 0.0,
        }
    }
}
