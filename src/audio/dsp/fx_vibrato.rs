// ─── audio/dsp/fx_vibrato.rs ──────────────────────────────────────────────────
// Vibrato FX — pitch-modulation cousin of `FxTremolo`.  Single
// delay-line tap whose read offset is modulated by an internal
// LFO, producing pitch wobble without level swing.
//
// V1 deliberately scoped:
//   * Same 4 knobs as the tremolo: rate, depth, shape, mix.  Knob
//     mappings differ from tremolo only in the depth scaling
//     (depth = peak delay-time deviation, which translates to
//     pitch swing) and in the rate cap (10 Hz instead of 12 Hz —
//     hyper-fast pitch wobble crosses into FM-sideband territory).
//   * Same sine→square shape morph, since a square LFO drives a
//     warbly two-pitch hop that's a distinct musical effect from
//     the smooth sine vibrato.
//   * Mix knob blends wet against dry — distinct from chorus,
//     where the wet path itself is dry+detuned.  Vibrato's wet
//     path is *only* the detuned tap, so the mix knob lets users
//     dial in pitch-wobble character without pure pitch shift.
//
// Distinct from `FxChorus` (multiple delay taps mixed with dry to
// thicken) — this is a single tap, no internal dry blend in the
// wet path.  Allocation-free; the delay line is a fixed-size
// array sized to the maximum-rate LFO swing at the highest
// supported sample rate.

use super::dsp_util::MIX_BYPASS_THRESHOLD;
use std::f32::consts::TAU;

/// Maximum delay-line length in samples — covers 10 ms at 96 kHz
/// (the highest sample rate the engine ever runs at) plus a small
/// guard band so the modulated read never reaches into the write
/// pointer's neighbourhood.  10 ms × 96 kHz = 960 samples; round
/// up to 1024 for power-of-two index masking.
const VIBRATO_BUFFER_LEN: usize = 1024;

/// Delay-line baseline in seconds — the modulator swings ±5 ms
/// either side of this, so the smallest possible read offset is
/// 5 ms − 5 ms = 0 (no delay) and the largest is 5 ms + 5 ms =
/// 10 ms (= VIBRATO_BUFFER_LEN at 96 kHz).
const VIBRATO_BASELINE_SEC: f32 = 0.005;

/// Maximum modulation depth in seconds.  At 5 Hz this gives
/// roughly ±50 cents pitch swing (peak), which is at the upper
/// end of musical vibrato.
const VIBRATO_MAX_SWING_SEC: f32 = 0.005;

pub(crate) struct VibratoFx {
    /// Circular delay buffer.  Sized at compile time so the FX
    /// doesn't need a `Vec` and stays allocation-free.
    buffer: [f32; VIBRATO_BUFFER_LEN],
    /// Write index into `buffer` — advances by 1 each sample,
    /// wraps via `& (VIBRATO_BUFFER_LEN - 1)`.
    write_idx: usize,
    /// LFO phase 0..TAU.  Same continuity guarantee as the
    /// tremolo's phase counter.
    phase: f32,
}

impl VibratoFx {
    pub(crate) fn new() -> Self {
        Self {
            buffer: [0.0; VIBRATO_BUFFER_LEN],
            write_idx: 0,
            phase: 0.0,
        }
    }

    /// `rate`:  0..1 → 0.1..10 Hz log-mapped.
    /// `depth`: 0..1 → 0..VIBRATO_MAX_SWING_SEC peak deviation.
    /// `shape`: 0..1 — sine→square LFO morph.
    /// `mix`:   0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        rate: f32,
        depth: f32,
        shape: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }
        // Write current sample into the delay line first so even
        // a baseline-only read (depth = 0) returns a delayed copy
        // of recent audio rather than zeros at the very start.
        self.buffer[self.write_idx] = input;

        // Log-mapped rate.  0.0 → 0.1 Hz; 1.0 → 10 Hz.  Vibrato
        // tops out lower than tremolo because hyper-fast pitch
        // wobble crosses into FM-sideband territory where the
        // effect stops reading as a vibrato.
        let r = rate.clamp(0.0, 1.0);
        let freq_hz = 0.1_f32 * 100.0_f32.powf(r);
        self.phase += TAU * freq_hz / sr;
        if self.phase >= TAU {
            self.phase -= TAU;
        }

        // Sine→square LFO morph — same shape as the tremolo's,
        // 16× tanh of the sine for crisp square corners.
        let s = self.phase.sin();
        let square = (16.0 * s).tanh();
        let sh = shape.clamp(0.0, 1.0);
        let lfo = (1.0 - sh) * s + sh * square;

        // Read offset in samples = baseline + depth * lfo * max_swing.
        // Depth=0 → static baseline delay (pitch unchanged).
        // Depth=1, lfo=±1 → ±5 ms swing around baseline.
        let baseline_samples = VIBRATO_BASELINE_SEC * sr;
        let swing_samples = VIBRATO_MAX_SWING_SEC * sr * depth.clamp(0.0, 1.0);
        // Read offset must stay strictly less than the buffer
        // length so the linear-interp index pair lands inside
        // the buffer.  Clamp defensively against extreme sample
        // rates that would push the baseline + swing past the
        // buffer end.
        let max_readback = (VIBRATO_BUFFER_LEN - 2) as f32;
        let read_offset = (baseline_samples + lfo * swing_samples)
            .max(0.5)
            .min(max_readback);

        // Linear-interp tap.  Floating read index counted backwards
        // from the write position.
        let read_pos = self.write_idx as f32 + (VIBRATO_BUFFER_LEN as f32) - read_offset;
        let i0 = read_pos as usize & (VIBRATO_BUFFER_LEN - 1);
        let i1 = (i0 + 1) & (VIBRATO_BUFFER_LEN - 1);
        let frac = read_pos - read_pos.floor();
        let wet = self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac;

        self.write_idx = (self.write_idx + 1) & (VIBRATO_BUFFER_LEN - 1);

        input * (1.0 - mix) + wet * mix
    }
}

impl Default for VibratoFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = VibratoFx::new();
        let out = fx.process(0.5, 0.5, 1.0, 1.0, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn zero_depth_passes_signal_through_with_static_delay() {
        // Depth = 0 → no LFO modulation; the signal still passes
        // through a 5 ms static delay.  After warmup, output
        // should match a sample from 5 ms earlier (or close to it,
        // accounting for the linear-interp tap).
        let mut fx = VibratoFx::new();
        // Warm up the delay line with a long DC input.
        for _ in 0..2_000 {
            fx.process(0.5, 0.5, 0.0, 0.0, 1.0, 48_000.0);
        }
        // Now the delay line has 0.5 stored throughout — output
        // must be 0.5 too.
        let out = fx.process(0.5, 0.5, 0.0, 0.0, 1.0, 48_000.0);
        assert!(
            (out - 0.5).abs() < 1e-3,
            "depth=0 + warmed buffer = transparent (got {out})"
        );
    }

    #[test]
    fn modulates_pitch_at_full_depth() {
        // Drive a 1 kHz sine through full-depth vibrato; the
        // output should NOT match a clean 1 kHz sine because the
        // pitch is being modulated.  Quantify the deviation by
        // comparing the first zero-crossing time relative to a
        // reference clean sine.
        let mut fx = VibratoFx::new();
        // Warm up.
        for i in 0..2_000 {
            let sig = (i as f32 * TAU / 48.0).sin();
            fx.process(sig, 1.0, 1.0, 0.0, 1.0, 48_000.0);
        }
        // Capture wet output for a window and verify it diverges
        // from the dry input.  A sample-by-sample diff > some
        // threshold means modulation is happening.
        let mut max_diff = 0.0_f32;
        for i in 2_000..4_000 {
            let sig = (i as f32 * TAU / 48.0).sin();
            let out = fx.process(sig, 1.0, 1.0, 0.0, 1.0, 48_000.0);
            max_diff = max_diff.max((out - sig).abs());
        }
        assert!(
            max_diff > 0.3,
            "vibrato should noticeably alter the input (max diff {max_diff})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = VibratoFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 1.0, 1.0, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Vibrato output max should track input — no internal gain.
        assert!(peak <= 1.5, "vibrato bounded at full drive (peak {peak})");
    }
}
