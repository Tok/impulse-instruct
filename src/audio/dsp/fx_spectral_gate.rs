// ─── audio/dsp/fx_spectral_gate.rs ────────────────────────────────────────────
// Spectral Gate FX — per-band amplitude gating across a log-
// spaced filter bank.  V1 takes the pragmatic route: 8 RBJ
// constant-0-dB-peak-gain BPF biquads spanning ~80 Hz to ~16 kHz,
// each with its own envelope follower + gate.  When a band's
// envelope falls below threshold, it's smoothly decayed toward
// silence; when it rises above, it attacks back toward unity.
//
// Output uses the *subtractive* recombination idiom:
//   output = input - sum_i((1 - gate_i) * band_i)
// This guarantees an exact passthrough when every gate is 1.0,
// regardless of the BPF bank's reconstruction accuracy — a real
// STFT spectral gate would be the textbook answer, but the
// codebase doesn't have FFT machinery yet, so this approximation
// gets the spectrally-selective character without the new
// infrastructure.
//
// Distinct from `FxGate` (broadband single-band envelope gate)
// and `FxFreeze` (held buffer / spectral freeze of the entire
// signal).  Allocation-free; coefficients refresh lazily on
// sample-rate change.

use std::f32::consts::TAU;

const NUM_BANDS: usize = 8;
/// Per-band Q.  ≈4 gives reasonable spectral selectivity while
/// keeping the band-overlap small enough that subtractive
/// recombination doesn't over-cancel adjacent bands.
const BAND_Q: f32 = 4.0;
/// Frequency span of the bank — log-spaced.  80 Hz catches the
/// kick fundamental; 16 kHz catches air / hi-hat sizzle.
const FREQ_LO: f32 = 80.0;
const FREQ_HI: f32 = 16_000.0;

/// One RBJ constant-0-dB-peak-gain BPF.  Peak gain = 1
/// regardless of Q — distinct from the constant-skirt-gain form
/// in `fx_resbank` where peak gain = Q.  Constant 0 dB is what
/// we want here so the band magnitudes are spectrally
/// comparable for envelope-based gating.
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

    /// RBJ BPF (0 dB peak gain, peak gain = 1):
    ///   b0 =  α / a0,  b1 = 0,  b2 = -α / a0
    ///   a0 =  1 + α,   a1 = -2 cos(ω) / a0,  a2 = (1 - α) / a0
    fn band_pass(fc: f32, q: f32, sr: f32) -> Self {
        let f = fc.clamp(20.0, sr * 0.45);
        let omega = TAU * f / sr;
        let q = q.max(0.5);
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        // y = b0·(x - x[n-2]) - a1·y[n-1] - a2·y[n-2]
        // (b1 = 0, b2 = -b0 baked into the (x - x[n-2]) product.)
        let y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub(crate) struct SpectralGateFx {
    bands: [Biquad; NUM_BANDS],
    env: [f32; NUM_BANDS],
    /// Per-band gate state — smoothed toward 1 (open) or 0
    /// (closed) at the configured attack / release rates.
    gate: [f32; NUM_BANDS],
    cached_sr: f32,
}

impl SpectralGateFx {
    pub(crate) fn new() -> Self {
        Self {
            bands: [Biquad::new(); NUM_BANDS],
            env: [0.0; NUM_BANDS],
            // Start with all gates open so a freshly-inserted FX
            // is transparent until the user dials threshold up.
            gate: [1.0; NUM_BANDS],
            cached_sr: f32::NAN,
        }
    }

    /// `thresh`:  0..1 — linear amplitude threshold.  Bands whose
    ///            envelope sits above this stay open; below, they
    ///            decay toward closed.
    /// `release`: 0..1 → 10..2000 ms.  Long = "freeze low-level
    ///            resonance" feel where bands that fired stay
    ///            open longer; short = quick spectral gating.
    /// `tilt`:    0..1 — threshold skew across the spectrum.
    ///            0.5 = uniform; <0.5 = highs gate more easily;
    ///            >0.5 = lows gate more easily.
    /// `mix`:     0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        thresh: f32,
        release: f32,
        tilt: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }

        // Lazy biquad refresh on sample-rate change.  NaN-safe via
        // is_finite gate.
        if !self.cached_sr.is_finite() || (sr - self.cached_sr).abs() > 0.5 {
            // Log-space the centre frequencies.
            let span = (FREQ_HI / FREQ_LO).ln();
            for i in 0..NUM_BANDS {
                let t = i as f32 / (NUM_BANDS - 1) as f32;
                let fc = FREQ_LO * (t * span).exp();
                self.bands[i] = Biquad::band_pass(fc, BAND_Q, sr);
            }
            self.cached_sr = sr;
        }

        // Knob mappings.
        let base_thr = thresh.clamp(0.0, 1.0);
        let release_ms = 10.0 * 200.0_f32.powf(release.clamp(0.0, 1.0));
        // 10..2000 ms log span.
        let release_coef = (-1.0_f32 / (release_ms * 0.001 * sr)).exp();
        let attack_coef = (-1.0_f32 / (0.005 * sr)).exp(); // ~5 ms attack — fast
        // env-follower attack (separate from gate attack).
        let env_attack = (-1.0_f32 / (0.003 * sr)).exp();

        // Total killed amount = sum of (1 - gate_i) * band_i —
        // the subtractive cancellation idiom keeps reconstruction
        // exact when every gate is 1.0.
        let mut killed = 0.0_f32;
        for i in 0..NUM_BANDS {
            let band_sample = self.bands[i].process(input);
            // Per-band envelope follower.
            let abs = band_sample.abs();
            if abs > self.env[i] {
                self.env[i] = abs + (self.env[i] - abs) * env_attack;
            } else {
                self.env[i] = abs + (self.env[i] - abs) * release_coef;
            }
            // Per-band threshold with tilt skew.  When tilt = 0.5,
            // every band sees the same `base_thr`.  Tilt < 0.5
            // raises the threshold for high bands (so they gate
            // more aggressively); tilt > 0.5 raises the threshold
            // for low bands.  Skew is gentle (max ±2× of base) so
            // the knob feels musical rather than clipping bands
            // into silence.
            let band_t = i as f32 / (NUM_BANDS - 1) as f32;
            let tilt_factor = if tilt < 0.5 {
                // tilt 0 → high bands threshold * 2.
                1.0 + (1.0 - 2.0 * tilt) * band_t
            } else {
                // tilt 1 → low bands threshold * 2.
                1.0 + (2.0 * tilt - 1.0) * (1.0 - band_t)
            };
            let band_thr = (base_thr * tilt_factor).clamp(0.0, 2.0);

            // Gate target: 1 if env above threshold, else 0.
            let target = if self.env[i] >= band_thr { 1.0 } else { 0.0 };
            // Smooth gate state toward target.  Attack is fast so
            // bands open quickly when a transient hits; release is
            // user-controlled so the user can freeze low-level
            // resonance.
            let coef = if target > self.gate[i] {
                attack_coef
            } else {
                release_coef
            };
            self.gate[i] = target + (self.gate[i] - target) * coef;
            killed += (1.0 - self.gate[i]) * band_sample;
        }

        let wet = input - killed;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for SpectralGateFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = SpectralGateFx::new();
        let out = fx.process(0.5, 0.5, 0.5, 0.5, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn passthrough_when_threshold_zero() {
        // Threshold = 0 → every band's env is always above
        // threshold → every gate stays at 1 → wet = input - 0 =
        // input.  Allow tiny numerical slack for the smoothing.
        let mut fx = SpectralGateFx::new();
        for _ in 0..1_000 {
            fx.process(0.5, 0.0, 0.5, 0.5, 1.0, 48_000.0);
        }
        let dry = 0.7_f32;
        let out = fx.process(dry, 0.0, 0.5, 0.5, 1.0, 48_000.0);
        assert!(
            (out - dry).abs() < 1e-3,
            "threshold=0 transparent (got {out})"
        );
    }

    #[test]
    fn high_threshold_silences_low_level_signal() {
        // Threshold above the band envelope → every band gates
        // closed → wet ≈ 0.  Drive a quiet signal so all band
        // envelopes stay below threshold.
        let mut fx = SpectralGateFx::new();
        let mut peak = 0.0_f32;
        for i in 0..6_000 {
            // 1 kHz sine at amplitude 0.05.
            let sig = (i as f32 * TAU * 1_000.0 / 48_000.0).sin() * 0.05;
            let out = fx.process(sig, 0.5, 0.0, 0.5, 1.0, 48_000.0);
            // Skip warmup window where bands haven't gated yet.
            if i >= 4_000 {
                peak = peak.max(out.abs());
            }
        }
        assert!(
            peak < 0.05,
            "high threshold gates a low-level signal (peak {peak})"
        );
    }

    #[test]
    fn loud_signal_passes_when_threshold_below() {
        // Threshold low + loud signal → most bands sit above and
        // pass.  Output should track the input within reasonable
        // ripple (filter bank isn't perfect).
        let mut fx = SpectralGateFx::new();
        let mut peak_in = 0.0_f32;
        let mut peak_out = 0.0_f32;
        for i in 0..6_000 {
            let sig = (i as f32 * TAU * 1_000.0 / 48_000.0).sin();
            let out = fx.process(sig, 0.05, 0.0, 0.5, 1.0, 48_000.0);
            if i >= 1_000 {
                peak_in = peak_in.max(sig.abs());
                peak_out = peak_out.max(out.abs());
            }
        }
        assert!(
            peak_out > peak_in * 0.5,
            "loud signal passes (in {peak_in}, out {peak_out})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = SpectralGateFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 0.5, 1.0, 0.5, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 2.0, "spec gate bounded at full drive (peak {peak})");
    }
}
