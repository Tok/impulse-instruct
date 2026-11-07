// ─── state/chiptune.rs ────────────────────────────────────────────────────────
// Chiptune voice — SID-flavoured (Commodore 64 6581/8580).  Three
// oscillators each selecting one of saw / triangle / pulse / noise,
// per-oscillator ADSR + level, shared pulse-width + resonant
// filter (LP/BP/HP), plus the SID-defining ring-mod and hard-sync
// toggles between adjacent voices.  Sequencer-driven.
//
// Goes for the SID *sound* rather than cycle-accurate emulation:
// the 16-step triangle staircase is reproduced (defining grit of
// SID triangles vs smooth analogue), the LFSR provides the
// metallic noise the SID is known for, and the filter is a
// resonant SVF (which sounds close to the 6581's 12 dB/oct).
// The 6581's chip-to-chip filter variation isn't modelled — what
// you get is closer to the cleaner 8580 voicing.

use serde::{Deserialize, Serialize};

/// Number of oscillators — fixed at 3 to match the SID.
pub const CHIPTUNE_OSCS: usize = 3;

/// Number of waveform options per oscillator.  Must match the
/// `SidWave` enum and the dispatch in `audio/dsp/chiptune.rs`.
pub const CHIPTUNE_WAVEFORMS: u8 = 4;

/// Number of filter modes.
pub const CHIPTUNE_FILTER_MODES: u8 = 3;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SidOsc {
    /// Waveform 0..=3 — Saw / Triangle / Pulse / Noise.  Stored
    /// as u8 so the JSON apply path can write a plain integer.
    /// Clamped at apply time.
    pub waveform: u8,
    /// Output level 0..1.
    pub level: f32,
    /// Per-oscillator ADSR — same map as the FM-ops / SAMPLER+
    /// envelopes (knob 0..1 → musically-spaced time).
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl Default for SidOsc {
    fn default() -> Self {
        Self {
            waveform: 0, // Saw — bright + harmonic-rich, classic SID lead
            level: 0.0,
            attack: 0.0,
            decay: 0.4,
            sustain: 0.6,
            release: 0.3,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChiptuneState {
    pub enabled: bool,
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    pub osc1: SidOsc,
    pub osc2: SidOsc,
    pub osc3: SidOsc,
    /// Shared pulse width 0..1 — only audible on oscillators in
    /// pulse mode.  0.5 is a square wave (odd harmonics only);
    /// off-centre values produce the classic PWM character that
    /// defines so much SID lead writing.
    pub pulse_width: f32,
    /// Filter cutoff 0..1 → log-mapped 80 Hz..16 kHz.
    pub filter_cutoff: f32,
    /// Filter resonance 0..1 → Q ≈ 0.5..18.
    pub filter_resonance: f32,
    /// Filter mode 0..=2 — LP / BP / HP.  Clamped at apply time.
    pub filter_mode: u8,
    /// Filter wet/dry mix 0..1 (0 = bypass).  Bypassed by default
    /// so a freshly-enabled chiptune voice doesn't sound dull —
    /// the user dials the filter in deliberately.
    pub filter_mix: f32,
    /// Ring modulate osc 1 by osc 2 — clangy / metallic timbres.
    /// SID-authentic: ring mod only meaningfully colours
    /// triangle waves on the original chip, but we apply it to
    /// whatever osc 1 is producing for simplicity.
    pub ring_mod: bool,
    /// Hard-sync osc 2's phase to osc 1 — when osc 1 wraps, osc
    /// 2's accumulator resets to 0.  Combined with osc 2 at a
    /// non-integer ratio this produces the "sync sweep" lead
    /// (Hello, Sandberg/Hubbard).
    pub sync: bool,
}

impl Default for ChiptuneState {
    fn default() -> Self {
        // 3-osc lead default: osc 1 saw at full level, osc 2
        // pulse at lower level for classic lead detune, osc 3
        // silent.  Filter bypassed (mix = 0) so the bare
        // oscillator brightness reads first; users dial in the
        // filter when they want SID-style sweeps.
        Self {
            enabled: false,
            volume: 0.7,
            pan: 0.0,
            osc1: SidOsc {
                level: 0.9,
                ..SidOsc::default()
            },
            osc2: SidOsc {
                level: 0.3,
                waveform: 2, // Pulse — pairs with osc 1 saw for
                // the classic SID lead detune.
                ..SidOsc::default()
            },
            osc3: SidOsc::default(),
            pulse_width: 0.5,   // Square — odd harmonics only by default
            filter_cutoff: 0.8, // Mostly open
            filter_resonance: 0.2,
            filter_mode: 0,  // LP
            filter_mix: 0.0, // Bypassed by default — user dials in
            ring_mod: false,
            sync: false,
        }
    }
}
