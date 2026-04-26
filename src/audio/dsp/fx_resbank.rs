// ─── audio/dsp/fx_resbank.rs ──────────────────────────────────────────────────
// Resonator bank FX — six tuned BPF biquads in parallel turn any
// input into a chord layer.  Karplus-on-input character: noisy
// inputs ring at the chord pitches, percussive transients pluck
// each pitch like a string.  Distinct from `FxComb` (one tuned
// delay-line resonator) — six simultaneous pitches at once,
// chord-knob-selectable, with pitch governed by the root knob
// rather than tracking the input's fundamental.
//
// V1 design:
//   * 6 RBJ-cookbook BPF biquads, one per voice in the chord.
//   * Chord knob 0..1 quantises into 6 preset interval sets:
//       0 minor 7        — root, m3, P5, m7, +oct root, +oct m3
//       1 major triad    — root, M3, P5, +oct root, +oct M3, +oct P5
//       2 dom 9          — root, M3, P5, m7, +oct root, M9
//       3 open fifths    — root, P5, +oct root, +oct P5, +2oct root, +2oct P5
//       4 octave stack   — root, +oct, +2oct, +3oct, +4oct, +5oct
//       5 cluster        — root, +M2, +P4, +P5, +M6, +oct
//   * Resonance knob 0..1 → Q 1..50 — controls how pingy /
//     sustained the resonators sound.
//   * Mix knob 0..1 wet/dry with cheap-bypass fast path.
//
// Output is the mean of the 6 BPF outputs scaled by 1/sqrt(Q) so
// high-Q patches don't run away — the BPF's peak gain is Q
// itself in the constant-skirt-gain form, so √Q normalisation
// gives consistent perceived loudness across the resonance knob.
// Allocation-free; coefficients refresh lazily on knob movement.

use std::f32::consts::TAU;

const NUM_VOICES: usize = 6;
const NUM_CHORDS: usize = 6;

/// Chord interval table — semitones above the root.  Order MUST
/// match the index list in the file header.
const CHORD_INTERVALS: [[i8; NUM_VOICES]; NUM_CHORDS] = [
    // 0 minor 7
    [0, 3, 7, 10, 12, 15],
    // 1 major triad spread
    [0, 4, 7, 12, 16, 19],
    // 2 dominant 9
    [0, 4, 7, 10, 12, 14],
    // 3 open fifths
    [0, 7, 12, 19, 24, 31],
    // 4 octave stack
    [0, 12, 24, 36, 48, 60],
    // 5 cluster / shimmer
    [0, 2, 5, 7, 9, 12],
];

/// One RBJ-cookbook constant-skirt-gain BPF.  Same shape as the
/// formant biquad in `vocal.rs`; kept inline rather than reused
/// because each FX has slightly different state-management
/// idioms.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    const fn new() -> Self {
        Self {
            b0: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn set(&mut self, freq_hz: f32, q: f32, sr: f32) {
        let f = freq_hz.clamp(20.0, sr * 0.45);
        let omega = TAU * f / sr;
        let q = q.max(0.5);
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q);
        let a0 = 1.0 + alpha;
        // Constant skirt gain BPF — `b1 = 0`, `b2 = -b0`, baked
        // into the (x - x[n-2]) product below.
        self.b0 = (sin_w * 0.5) / a0;
        self.a1 = (-2.0 * cos_w) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub(crate) struct ResBankFx {
    voices: [Biquad; NUM_VOICES],
    /// Cached knob state — coefficients only refresh when one of
    /// these moves appreciably (rare relative to audio rate).
    cached_root: f32,
    cached_chord: u8,
    cached_q: f32,
    cached_sr: f32,
}

impl ResBankFx {
    pub(crate) fn new() -> Self {
        Self {
            voices: [Biquad::new(); NUM_VOICES],
            cached_root: f32::NAN,
            cached_chord: u8::MAX,
            cached_q: f32::NAN,
            cached_sr: f32::NAN,
        }
    }

    /// `root`:  0..1 → MIDI 24..96 (C1..C7).
    /// `chord`: 0..1 → quantised to 0..5 chord preset.
    /// `res`:   0..1 → Q 1..50 log-mapped.
    /// `mix`:   0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        root: f32,
        chord: f32,
        res: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }

        // Quantise the chord knob.  `floor` keeps the knob's
        // 0..1 sweep evenly distributed across the presets.
        let chord_idx =
            ((chord.clamp(0.0, 0.9999) * NUM_CHORDS as f32) as usize).min(NUM_CHORDS - 1) as u8;
        // Q log-mapped 1..50: 1 = barely-resonant, 50 = singing.
        let q = 1.0 * 50.0_f32.powf(res.clamp(0.0, 1.0));
        let root_clamped = root.clamp(0.0, 1.0);

        // Lazy re-tune on knob change.  NaN-safe — first call
        // (cached_* = NaN) always triggers init since `>` against
        // NaN is false but the `is_finite` guards take precedence.
        if !self.cached_root.is_finite()
            || (root_clamped - self.cached_root).abs() > 1e-4
            || chord_idx != self.cached_chord
            || (q - self.cached_q).abs() > 0.01
            || (sr - self.cached_sr).abs() > 0.5
        {
            // root knob 0..1 → MIDI 24..96 (C1 .. C7).
            let root_midi = 24.0 + 72.0 * root_clamped;
            let intervals = &CHORD_INTERVALS[chord_idx as usize];
            for (i, &ivl) in intervals.iter().enumerate() {
                let midi = (root_midi + ivl as f32).clamp(0.0, 127.0);
                // Standard MIDI → Hz: 440 * 2^((m-69)/12).
                let hz = 440.0 * 2.0_f32.powf((midi - 69.0) / 12.0);
                self.voices[i].set(hz, q, sr);
            }
            self.cached_root = root_clamped;
            self.cached_chord = chord_idx;
            self.cached_q = q;
            self.cached_sr = sr;
        }

        // Sum the 6 BPFs.  Constant-skirt-gain BPF has peak gain
        // = Q at the resonance, so high-Q patches need
        // normalisation to stay near unity.  1/sqrt(Q) keeps the
        // perceived loudness roughly stable across the res knob.
        let mut acc = 0.0_f32;
        for v in &mut self.voices {
            acc += v.process(input);
        }
        let norm = (NUM_VOICES as f32 * q).sqrt().max(1.0);
        let wet = acc / norm;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for ResBankFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = ResBankFx::new();
        let out = fx.process(0.5, 0.5, 0.0, 0.5, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn produces_audible_output_from_impulse() {
        // Drive a single impulse and let the resonators ring.
        // The bank should produce non-zero audible output for at
        // least a few thousand samples after the impulse — one
        // of the ways the FX shines (Karplus-on-input
        // percussion → chord ring).
        let mut fx = ResBankFx::new();
        let mut peak = 0.0_f32;
        // Single click followed by silence.
        let _ = fx.process(1.0, 0.5, 0.0, 0.7, 1.0, 48_000.0);
        for _ in 0..6_000 {
            let out = fx.process(0.0, 0.5, 0.0, 0.7, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak > 0.001, "resonators should ring (peak {peak})");
    }

    #[test]
    fn every_chord_preset_resonates() {
        // Sanity-check every chord index produces an audible
        // ring.  Catches a regression where, e.g., a future
        // preset list a too-high octave stack hits the Nyquist
        // clamp and silences the bank.
        for c_idx in 0..NUM_CHORDS {
            let mut fx = ResBankFx::new();
            let chord_knob = (c_idx as f32 + 0.5) / NUM_CHORDS as f32;
            let _ = fx.process(1.0, 0.5, chord_knob, 0.7, 1.0, 48_000.0);
            let mut peak = 0.0_f32;
            for _ in 0..6_000 {
                let out = fx.process(0.0, 0.5, chord_knob, 0.7, 1.0, 48_000.0);
                assert!(out.is_finite());
                peak = peak.max(out.abs());
            }
            assert!(
                peak > 0.001,
                "chord preset {c_idx} should ring (peak {peak})"
            );
        }
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = ResBankFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 0.5, 0.0, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 2.0, "resbank bounded at full drive (peak {peak})");
    }
}
