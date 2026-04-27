// ─── audio/dsp/fx_iso_eq.rs ───────────────────────────────────────────────────
// 3-band ISO / kill EQ — DJ-style hard-kill bands at low / mid /
// high crossovers.  Performance FX, distinct from `FxEq` (3-band
// fixed-shelf with continuous gain knobs):
//   * ISO knobs are unipolar 0..1 — no boost, only kill / pass.
//   * Mid band is computed *subtractively* (mid = dry - low -
//     high) so the three bands sum to the input EXACTLY when all
//     three knobs are at 1.0.  No phase-cancellation surprises
//     when the user dials between extremes.
//   * Crossovers are fixed (~250 Hz, ~2.5 kHz) — DJ standard,
//     and exposing them as additional knobs would push past the
//     four-knob 2×1 grid.
//
// Implementation: two RBJ-cookbook biquads per FX instance (one
// LP at 250 Hz for the low band, one HP at 2.5 kHz for the high
// band).  The mid band needs no filter at all — it's just
// `dry - low - high`.  Allocation-free.

use super::dsp_util::nyquist_guard;
use std::f32::consts::TAU;

/// Low/mid crossover (Hz).  Standard DJ-mixer choice; the bass
/// kick + bassline live below this point.
const ISO_LOW_FC: f32 = 250.0;
/// Mid/high crossover (Hz).  The "air" band starts above this —
/// hi-hats, snares, presence.
const ISO_HIGH_FC: f32 = 2_500.0;
/// Q for the crossover biquads.  ~0.7 = Butterworth-flat; sharp
/// enough for clean kills without ringing.
const ISO_Q: f32 = 0.707;

/// One RBJ-cookbook biquad — kept inline here rather than reused
/// from the global `Biquad` because that one only exposes shelf /
/// peak coefficients; the kill EQ specifically needs LP and HP.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
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
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// RBJ low-pass: 12 dB/oct rolloff above `fc`.
    fn low_pass(fc: f32, q: f32, sr: f32) -> Self {
        let w = TAU * fc.clamp(20.0, nyquist_guard(sr)) / sr;
        let cos_w = w.cos();
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos_w) * 0.5) / a0,
            b1: (1.0 - cos_w) / a0,
            b2: ((1.0 - cos_w) * 0.5) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// RBJ high-pass: 12 dB/oct rolloff below `fc`.
    fn high_pass(fc: f32, q: f32, sr: f32) -> Self {
        let w = TAU * fc.clamp(20.0, nyquist_guard(sr)) / sr;
        let cos_w = w.cos();
        let alpha = w.sin() / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w) * 0.5) / a0,
            b1: (-(1.0 + cos_w)) / a0,
            b2: ((1.0 + cos_w) * 0.5) / a0,
            a1: (-2.0 * cos_w) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub(crate) struct IsoEqFx {
    /// LP at 250 Hz — extracts the low band.
    low: Biquad,
    /// HP at 2.5 kHz — extracts the high band.
    high: Biquad,
    /// Cached engine sample rate so we recompute the biquads only
    /// when the rate actually changes.  Initialised to NaN so the
    /// first sample triggers a refresh.
    cached_sr: f32,
}

impl IsoEqFx {
    pub(crate) fn new() -> Self {
        Self {
            low: Biquad::new(),
            high: Biquad::new(),
            cached_sr: f32::NAN,
        }
    }

    /// `low`:  0..1 — gain on the band below ~250 Hz.
    /// `mid`:  0..1 — gain on the mid band (subtractive).
    /// `high`: 0..1 — gain on the band above ~2.5 kHz.
    /// `mix`:  0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        low: f32,
        mid: f32,
        high: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }
        // Recompute the biquads only when the engine sample rate
        // changes (rare relative to the audio rate).  At every
        // other sample we just run the cached coefficients.
        if (sr - self.cached_sr).abs() > 0.5 {
            self.low = Biquad::low_pass(ISO_LOW_FC, ISO_Q, sr);
            self.high = Biquad::high_pass(ISO_HIGH_FC, ISO_Q, sr);
            self.cached_sr = sr;
        }
        let low_band = self.low.process(input);
        let high_band = self.high.process(input);
        // Subtractive mid — sum of bands == dry when low/mid/high
        // gains are all 1.  No filter needed for the mid band
        // itself.
        let mid_band = input - low_band - high_band;
        let l = low.clamp(0.0, 1.0);
        let m = mid.clamp(0.0, 1.0);
        let h = high.clamp(0.0, 1.0);
        let wet = low_band * l + mid_band * m + high_band * h;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for IsoEqFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = IsoEqFx::new();
        let out = fx.process(0.5, 0.0, 0.0, 0.0, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn passthrough_when_all_bands_at_unity() {
        // Sum of bands should equal dry when low=mid=high=1.
        // Allow tiny numerical slack for biquad transient.
        let mut fx = IsoEqFx::new();
        // Run a few hundred warmup samples to settle the filters.
        for _ in 0..200 {
            fx.process(0.5, 1.0, 1.0, 1.0, 1.0, 48_000.0);
        }
        // Now drive a fresh sample and check the wet equals dry.
        let dry = 0.7_f32;
        let out = fx.process(dry, 1.0, 1.0, 1.0, 1.0, 48_000.0);
        assert!(
            (out - dry).abs() < 1e-3,
            "passthrough at unity gains (got {out}, want {dry})"
        );
    }

    #[test]
    fn killing_low_band_silences_low_frequency_input() {
        // Drive a 50 Hz sine (well into the low band) with low=0.
        // Output should be close to silence.
        let mut fx = IsoEqFx::new();
        let mut peak = 0.0_f32;
        // Skip the first 200 samples for filter warmup.
        for i in 0..2_400 {
            let sig = (i as f32 * TAU * 50.0 / 48_000.0).sin();
            let out = fx.process(sig, 0.0, 1.0, 1.0, 1.0, 48_000.0);
            if i >= 200 {
                peak = peak.max(out.abs());
            }
        }
        // The crossover is at 250 Hz; at 50 Hz the LP passes
        // virtually everything, so killing low should silence the
        // signal almost entirely.
        assert!(
            peak < 0.2,
            "killing low band at 50 Hz should silence (peak {peak})"
        );
    }

    #[test]
    fn killing_high_band_silences_high_frequency_input() {
        // Drive a 8 kHz sine (well into the high band) with high=0.
        let mut fx = IsoEqFx::new();
        let mut peak = 0.0_f32;
        for i in 0..2_400 {
            let sig = (i as f32 * TAU * 8_000.0 / 48_000.0).sin();
            let out = fx.process(sig, 1.0, 1.0, 0.0, 1.0, 48_000.0);
            if i >= 200 {
                peak = peak.max(out.abs());
            }
        }
        assert!(
            peak < 0.2,
            "killing high band at 8 kHz should silence (peak {peak})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = IsoEqFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 1.0, 1.0, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // No internal gain — output should track input magnitude.
        assert!(peak <= 1.5, "iso eq bounded at full drive (peak {peak})");
    }
}
