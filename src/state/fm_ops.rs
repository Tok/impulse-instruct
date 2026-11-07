// ─── state/fm_ops.rs ──────────────────────────────────────────────────────────
// FM operator synth — 4-op DX7-flavoured voice.  Plugs the gap
// between the existing AN1X (subtractive) and SAMPLER+ (samples) for
// bell / E-piano / FM-bass tones that don't reproduce well from the
// current voice palette.
//
// Each operator is a sine oscillator with its own ADSR + level; the
// `algorithm` selector picks one of four operator-routing topologies
// (stack / multimod / parallel pairs / additive).  Feedback applies
// to op 4, the topmost modulator in the chain algorithms — gives the
// classic FM saw-like spectral richness when dialled in.
//
// Sequencer-driven: a step trigger sets every op's carrier frequency
// from the played note (scaled by op `ratio`) and resets every
// envelope to Attack.  Distinct from the absurd-queue voices
// (Theremin / Pendulum) which are knob-driven only.

use serde::{Deserialize, Serialize};

/// Per-operator parameters.  Six fields × four ops = 24 fields on
/// the parent struct, plus the global five (enabled / volume / pan
/// / algorithm / feedback) — total 29.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FmOp {
    /// Frequency ratio knob 0..1 — log-mapped to 0.5..8.0× the
    /// played note (so 0.5 on the knob = 1.0× unison, 0 = ½×
    /// octave down, 1 = 8× three octaves up).  Integer ratios
    /// produce classical harmonic FM; non-integer ratios produce
    /// metallic inharmonic timbres (bells, gongs, FM percussion).
    pub ratio: f32,
    /// Output level 0..1.  For carrier ops this is the audio gain;
    /// for modulator ops it's the modulation index — controls the
    /// brightness of the resulting timbre.  Modulator level scales
    /// to a max index of 8.0 internally (4× the standard DX7 unit
    /// which is enough to reach FM-bass / bell territory without
    /// the clipping you'd get at higher indices).
    pub level: f32,
    /// Attack / Decay / Sustain / Release.  Per-op envelopes are
    /// what gives FM patches their distinctive evolving timbres —
    /// modulators decaying faster than carriers makes a bell tail
    /// soften from bright to mellow, etc.
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for FmOp {
    fn default() -> Self {
        Self {
            ratio: 0.5,   // 1.0× — unison with the played note
            level: 0.0,   // silent until the user dials in
            attack: 0.0,  // instant attack — FM patches typically punchy
            decay: 0.4,   // medium decay
            sustain: 0.6, // mid-level sustain
            release: 0.3, // moderate release tail
        }
    }
}

/// Algorithm count for the V1 ship.  DX7 has 32 algorithms; we ship
/// four that span the most common shapes (stack / multimod /
/// parallel pairs / additive).  Adding more is a follow-up — the
/// audio code dispatches on this index, so growing the list only
/// requires extending the match arm in `audio/dsp/fm_ops.rs`.
pub const FM_ALGORITHM_COUNT: u8 = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FmOpsState {
    pub enabled: bool,
    /// Output volume 0..1.5.
    pub volume: f32,
    /// Stereo pan -1..+1.
    #[serde(default)]
    pub pan: f32,
    /// Algorithm selector 0..=3.  Clamped at apply time so any
    /// out-of-range value from the LLM lands on a valid topology
    /// instead of falling through to silence.
    ///
    /// 0 — **Stack**: 4→3→2→1.  Op 1 is the only carrier.  Rich
    ///     harmonic cascade — the FM-bass / FM-lead workhorse.
    /// 1 — **Multimod**: 4→1, 3→1, 2→1.  Op 1 is the carrier with
    ///     three parallel modulators.  Bell / mallet timbres.
    /// 2 — **Parallel pairs**: 4→3, 2→1.  Ops 1 and 3 are carriers,
    ///     each with one modulator.  Layered two-tone patches.
    /// 3 — **Additive**: all four ops are carriers.  Pure sine
    ///     stack — organ / Hammond / clean leads.
    pub algorithm: u8,
    /// Feedback 0..1 on op 4 — the topmost modulator in the chain
    /// algorithms.  Higher feedback adds saw-like spectral richness
    /// to FM bells; at extreme settings op 4 self-oscillates into a
    /// noise-y waveshape.  In additive mode (algo 3) feedback has
    /// no audible effect since op 4 isn't routed to anything.
    #[serde(default)]
    pub feedback: f32,
    pub op1: FmOp,
    pub op2: FmOp,
    pub op3: FmOp,
    pub op4: FmOp,
}

impl Default for FmOpsState {
    fn default() -> Self {
        // 2-op stack default: op 1 carrier at unison, op 2 modulator
        // at unison with moderate index.  Produces a clean simple FM
        // tone the user can immediately dial — empty modulators
        // (level 0) on op 3 / op 4 so the default doesn't sound
        // muddy / over-modulated.
        Self {
            enabled: false,
            volume: 0.7,
            pan: 0.0,
            algorithm: 0, // stack — the most musical default for a 2-op start
            feedback: 0.0,
            // Op 1 carrier full output, op 2 modulator at moderate
            // FM index — produces a clean simple FM tone the user
            // can immediately dial without any pre-twisting.
            op1: FmOp {
                level: 1.0,
                ..FmOp::default()
            },
            op2: FmOp {
                level: 0.5,
                ..FmOp::default()
            },
            op3: FmOp::default(),
            op4: FmOp::default(),
        }
    }
}
