// ─── state/pendulum.rs ────────────────────────────────────────────────────────
// Pendulum voice — two near-tuned sine oscillators that beat
// acoustically.  Absurd-queue voice #5.
//
// As `detune_hz` drifts from 0, the beat frequency between the two
// oscillators rises from inaudible (chord territory: < 1 Hz) to a
// pulsing tremolo (1–10 Hz, the iconic Risset / Shepard pendulum
// feel) to an inharmonic drone (10+ Hz, the difference becomes its
// own subaudible tone).  The panel displays the live beat rate so
// the user can dial intentionally.
//
// V1 is knob-driven only — no sequencer hookup; same shape as the
// Theremin voice in `state::theremin`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendulumState {
    pub enabled: bool,
    /// Base pitch knob 0..1 → 30–800 Hz log-mapped (~4.7 octaves).
    /// Lower than the Theremin's 50–2000 Hz range because the
    /// beating effect is most musical in the low-mid register
    /// where the difference tone lands inside the audible band.
    pub base_pitch: f32,
    /// Detune separation 0..1 → 0–60 Hz between the two
    /// oscillators.  At 0 the two are exactly in phase (no beat);
    /// at 1 they're a perfect-fourth apart at 100 Hz (beating
    /// becomes its own tone).
    pub detune_hz: f32,
    /// Oscillator mix 0..1 → osc1 vs osc2 balance.  0.5 (default)
    /// gives the deepest beat amplitude modulation; pushing
    /// toward 0 or 1 fades one oscillator out and the beat with
    /// it.  Kept as a knob rather than fixed because it changes
    /// the feel from a deep wobble to a faint shimmer.
    pub mix: f32,
    /// Output volume 0..1.5.
    pub volume: f32,
    /// Stereo pan -1..+1 — when non-zero, the two oscillators
    /// pan-spread (osc1 toward the centre, osc2 toward the
    /// pan side).  Catches the iconic Riley-style "Pendulum
    /// Music" stereo feel.
    #[serde(default)]
    pub pan: f32,
}

impl Default for PendulumState {
    fn default() -> Self {
        Self {
            enabled: false,
            base_pitch: 0.4, // ~110 Hz with the 30–800 Hz log map
            detune_hz: 0.05, // ~3 Hz — a slow audible beat by default
            mix: 0.5,        // equal — deepest beat amplitude
            volume: 0.6,
            pan: 0.0,
        }
    }
}
