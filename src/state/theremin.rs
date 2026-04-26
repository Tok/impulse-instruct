// ─── state/theremin.rs ────────────────────────────────────────────────────────
// Theremin voice — XY-pad-driven continuous-pitch oscillator with
// portamento.  Absurd-queue item #2.
//
// The state surface is intentionally tiny: x / y from the XY pad
// drive pitch / volume directly; portamento + brightness shape the
// glide and spectrum; volume / pan + enabled match the rest of the
// voice palette so the rack treats it like any other voice.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThereminState {
    pub enabled: bool,
    /// XY pad horizontal position 0..1.  Mapped to a log-frequency
    /// range 50–2000 Hz at audio time, so an octave covers ~25 % of
    /// the pad — feels close to a real Theremin's two-and-a-half
    /// octave sweep across the antenna's effective range.
    pub x: f32,
    /// XY pad vertical position 0..1.  Direct gain — bottom = silent,
    /// top = full output.  Real Theremins have a logarithmic volume
    /// loop antenna; a linear control feels expressive enough for V1.
    pub y: f32,
    /// Portamento time 0..1 → 1 ms..500 ms one-pole smoothing on the
    /// target frequency.  The defining gesture of a Theremin is the
    /// glide between notes; without smoothing a knob drag sounds
    /// like a step-quantiser.
    pub portamento: f32,
    /// Brightness 0..1 — adds 3rd + 5th harmonics on top of the
    /// fundamental sine.  At 0 the voice is a pure sine (the
    /// classic eerie tone); at 1 it picks up a "talking" character
    /// from the odd harmonics that approximates the heterodyning
    /// squeal of the real instrument's tube circuitry.
    pub brightness: f32,
    /// Output gain 0..1.5 — pre-pan, post per-sample envelope
    /// follow.  Allows the rack mixer to balance the Theremin
    /// against louder voices like the gabber kick.
    pub volume: f32,
    /// Stereo pan -1..+1.
    #[serde(default)]
    pub pan: f32,
}

impl Default for ThereminState {
    fn default() -> Self {
        Self {
            enabled: false,
            x: 0.5,           // mid-range pitch (~316 Hz, around D#4)
            y: 0.0,           // start silent — the user "lifts" the pad
            portamento: 0.15, // ~75 ms — natural-feeling glide
            brightness: 0.0,  // pure sine by default
            volume: 0.7,
            pan: 0.0,
        }
    }
}
