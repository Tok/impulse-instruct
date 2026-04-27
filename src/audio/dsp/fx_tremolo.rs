// ─── audio/dsp/fx_tremolo.rs ──────────────────────────────────────────────────
// Tremolo FX — periodic amplitude modulation by an internal LFO.
// V1 deliberately scoped:
//   * Sine→square shape morph driven by `shape` knob.  Shape lerp
//     uses `tanh(scaled_sine)` so both endpoints (smooth swell and
//     hard chop) and every shape between them are continuous.
//   * Rate 0.1..12 Hz log-mapped so the knob covers slow swell
//     through helicopter-chop without an exponential cliff.
//   * Depth 0..1 — at 0 the LFO is unity (no AM); at 1 the wet
//     signal swings between full volume and silence.
//   * Mix knob blends wet against dry — the cheap-bypass fast
//     path checks this value first so an inserted-but-disengaged
//     tremolo costs almost nothing per sample.
//
// Distinct from `FxPan` (left/right balance LFO, no level swing)
// and from chorus / flanger (delay-line modulation, not AM).
//
// Allocation-free.  One FX instance keeps a single phase counter
// and a tiny LCG-free advance — no buffers, no lookups.

use super::dsp_util::MIX_BYPASS_THRESHOLD;
use std::f32::consts::TAU;

pub(crate) struct TremoloFx {
    /// LFO phase in 0..TAU — advances by `2π·rate / sr` each sample.
    /// Kept across `process` calls so the LFO stays continuous when
    /// the user sweeps rate or toggles bypass.
    phase: f32,
}

impl TremoloFx {
    pub(crate) fn new() -> Self {
        Self { phase: 0.0 }
    }

    /// `rate`:  0..1 → 0.1..12 Hz log-mapped (1.0 = 12 Hz).
    /// `depth`: 0..1 — modulation depth (0 = no AM, 1 = full chop).
    /// `shape`: 0..1 — sine→square morph via tanh-clamping a
    ///          scaled sine.  0 = pure sine; 1 = near-square.
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
        // Log-mapped rate so the knob is musical: 0.0 = 0.1 Hz
        // (one cycle per 10 s), 1.0 = 12 Hz (helicopter chop).
        let r = rate.clamp(0.0, 1.0);
        let freq_hz = 0.1_f32 * 120.0_f32.powf(r);
        self.phase += TAU * freq_hz / sr;
        if self.phase >= TAU {
            self.phase -= TAU;
        }

        // Shape morph — lerp between pure sine and a tanh-clamped
        // sine that approximates a square wave.  Both endpoints
        // span ±1, so the depth knob's swing stays consistent
        // regardless of where shape sits.  16× tanh gain gives
        // crisp square corners without numerical saturation.
        let s = self.phase.sin();
        let square = (16.0 * s).tanh();
        let sh = shape.clamp(0.0, 1.0);
        let lfo = (1.0 - sh) * s + sh * square;
        // Map ±1 LFO → 1.0 ± depth gain.  Depth=0 leaves the
        // signal untouched (gain=1 always); depth=1 swings the
        // gain between 0 and 2 — full chop on one side, +6 dB
        // boost on the other.  Using ±depth (rather than the
        // half-and-half "0..1 swing" some hardware tremolos do)
        // keeps unity gain at the LFO mid-point regardless of
        // the depth knob.
        let d = depth.clamp(0.0, 1.0);
        let gain = 1.0 + d * lfo;

        let wet = input * gain;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for TremoloFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = TremoloFx::new();
        let out = fx.process(0.5, 0.5, 1.0, 1.0, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn modulates_amplitude_at_full_depth() {
        // Drive a constant input through full-depth tremolo at the
        // upper rate; capture min and max gain over multiple cycles.
        // Depth = 1 should swing between 0 and 2× the input.
        let mut fx = TremoloFx::new();
        let dry = 0.5_f32;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        // rate=1.0 → 12 Hz; 48 kHz / 12 Hz = 4000 samples per
        // cycle.  16 000 samples = 4 full cycles, plenty of room
        // for the LFO to reach both extremes.
        for _ in 0..16_000 {
            let out = fx.process(dry, 1.0, 1.0, 0.0, 1.0, 48_000.0);
            min = min.min(out);
            max = max.max(out);
        }
        assert!(
            min < dry * 0.1,
            "tremolo should reach near-silence (min {min})"
        );
        assert!(
            max > dry * 1.5,
            "tremolo should reach near-double (max {max})"
        );
    }

    #[test]
    fn zero_depth_leaves_signal_unchanged() {
        // Depth = 0 means the LFO has no effect — output = input
        // regardless of mix (modulo the constant -0 dB centre gain
        // we built in to keep unity at the LFO midpoint).
        let mut fx = TremoloFx::new();
        let dry = 0.5_f32;
        for _ in 0..1_000 {
            let out = fx.process(dry, 0.5, 0.0, 0.0, 1.0, 48_000.0);
            assert!((out - dry).abs() < 1e-5, "depth=0 transparent (got {out})");
        }
    }

    #[test]
    fn shape_one_produces_near_square_lfo() {
        // At shape=1 the LFO should spend most of its time at ±1
        // and very little in transit.  Drive constant input; the
        // distribution of output gain values should be bimodal at
        // the extremes (low and high cluster) with very few
        // intermediate samples.
        let mut fx = TremoloFx::new();
        let dry = 1.0_f32;
        let mut extreme_count = 0;
        let mut middle_count = 0;
        for _ in 0..16_000 {
            let out = fx.process(dry, 1.0, 1.0, 1.0, 1.0, 48_000.0);
            // Extreme: gain near 0 (output near 0) or near 2 (output near 2).
            if out.abs() < 0.1 || (out - 2.0).abs() < 0.1 {
                extreme_count += 1;
            } else if (out - 1.0).abs() < 0.1 {
                middle_count += 1;
            }
        }
        // Square-ish LFO spends most of its time at the extremes.
        assert!(
            extreme_count > middle_count * 2,
            "square shape should dwell at extremes ({} extreme vs {} middle)",
            extreme_count,
            middle_count,
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = TremoloFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 1.0, 1.0, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Worst case: input=1 and gain=2 → output=2.  Allow a small
        // margin for the tanh shape's overshoot near the corners.
        assert!(peak <= 2.5, "tremolo bounded at full drive (peak {peak})");
    }
}
