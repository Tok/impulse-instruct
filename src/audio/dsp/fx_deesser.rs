// ─── audio/dsp/fx_deesser.rs ──────────────────────────────────────────────────
// De-esser FX — sidechain HP detector → narrow ducker on the
// sibilant band.  Specialist tool for vocal / hat material;
// distinct from `FxCompressor` (which works broadband on the
// full signal).  Only the band above the sibilant cutoff is
// attenuated; everything below passes untouched, so the de-essed
// output sounds natural rather than dynamics-pumped.
//
// Implementation:
//   * One RBJ-cookbook HP biquad at the sibilant centre.  The
//     filter's output is *both* the detector signal (its envelope
//     decides ducking amount) and the band that gets ducked
//     (subtractively recombined with the dry minus the band).
//   * Peak-style envelope follower with fast attack (~3 ms) and
//     medium release (~50 ms) on the HP band.
//   * Gain reduction = `(env / threshold).powf(-amount)` clamped
//     to (0, 1] when env > threshold; otherwise gain = 1.0.
//   * Output = (dry - sibilant) + sibilant * gain, then wet/dry
//     mixed with the unprocessed dry.
//
// Allocation-free.  Coefficients refresh lazily when freq or sr
// move appreciably.

use super::dsp_util::MIX_BYPASS_THRESHOLD;
use super::dsp_util::nyquist_guard;
use std::f32::consts::TAU;

/// One RBJ-cookbook HP biquad.  Same shape as the one in
/// `fx_iso_eq` — kept inline rather than extracted into a shared
/// module because each FX has small differences in init / state
/// management that aren't worth abstracting out for two call sites.
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

    fn low_pass(fc: f32, sr: f32) -> Self {
        let q = 0.707; // Butterworth-flat — clean rolloff, no resonance.
        let w = TAU * fc.clamp(20.0, nyquist_guard(sr)) / sr;
        let cos_w = w.cos();
        let alpha = w.sin() / (2.0 * q);
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

pub(crate) struct DeEsserFx {
    /// LP biquad at the sibilant cutoff — its complement
    /// (`input - low`) gives a phase-coherent sibilant band that
    /// recombines back to the dry input exactly when the ducker
    /// gain is 1.0.  An HP biquad would give a *steady-state*
    /// sibilant band but the phase relationship with the dry
    /// input would let `input - sibilant` swing to ±2× the input
    /// at the cutoff frequency, which is not what we want.
    lp: Biquad,
    /// Envelope follower state (peak with attack/release).
    env: f32,
    /// Cached freq / sample rate so we recompute the biquad only
    /// on actual change.  Initialised to NaN; the `is_finite`
    /// check below forces first-call init.
    cached_freq: f32,
    cached_sr: f32,
}

impl DeEsserFx {
    pub(crate) fn new() -> Self {
        Self {
            lp: Biquad::new(),
            env: 0.0,
            cached_freq: f32::NAN,
            cached_sr: f32::NAN,
        }
    }

    /// `freq`:      0..1 → 3..12 kHz log-mapped.
    /// `threshold`: 0..1 — linear amplitude.  When the HP-band
    ///              envelope rises above this level, the ducker
    ///              engages.
    /// `amount`:    0..1 — how aggressive the ducking gets.
    ///              0 = no compression (FX is no-op even at full
    ///              mix); 1 = hard kill of the sibilant band on
    ///              every over-threshold sample.
    /// `mix`:       0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        freq: f32,
        threshold: f32,
        amount: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }
        // Lazy re-tune: freq knob moves ≪ audio rate; only refresh
        // the biquad when the user actually moves the knob enough
        // to matter musically.  NaN-safe comparison so first call
        // (cached_freq = NaN) always triggers init — `> 1.0` would
        // be false on NaN and leave the biquad as a passthrough.
        let fc = 3_000.0 * 4.0_f32.powf(freq.clamp(0.0, 1.0)); // 3..12 kHz log
        if !self.cached_freq.is_finite()
            || (fc - self.cached_freq).abs() > 1.0
            || (sr - self.cached_sr).abs() > 0.5
        {
            self.lp = Biquad::low_pass(fc, sr);
            self.cached_freq = fc;
            self.cached_sr = sr;
        }

        // Complementary band split: low-band passes through the LP
        // biquad; sibilant band is `input - low`.  Reassembly is
        // exact (low + sibilant = input) so the FX is bit-
        // transparent when the ducker gain is 1.0.
        let low = self.lp.process(input);
        let sibilant = input - low;
        // Peak envelope follower — fast attack / medium release.
        // sr-aware coefficients keep the time constants musical
        // across sample-rate changes.
        let attack_coef = (-1.0_f32 / (0.003 * sr)).exp(); // ~3 ms
        let release_coef = (-1.0_f32 / (0.05 * sr)).exp(); // ~50 ms
        let abs_sib = sibilant.abs();
        if abs_sib > self.env {
            self.env = abs_sib + (self.env - abs_sib) * attack_coef;
        } else {
            self.env = abs_sib + (self.env - abs_sib) * release_coef;
        }

        // Gain reduction — `(env/thr)^(-amount)` is the standard
        // soft-knee compressor curve mapped through the amount
        // knob.  amount=0 → exponent=0 → gain=1 (no compression).
        // amount=1 → exponent=-1 → gain=thr/env (hard kill of
        // any signal that hits threshold).
        let thr = threshold.clamp(0.001, 1.5);
        let a = amount.clamp(0.0, 1.0);
        let gain = if self.env > thr {
            (self.env / thr).powf(-a).clamp(0.0, 1.0)
        } else {
            1.0
        };

        // Recombine: low-band passes untouched; sibilant band is
        // multiplied by the ducker gain.  Sums back to the dry
        // input exactly when gain = 1.0 because of the
        // complementary split (low + sibilant = input).
        let wet = low + sibilant * gain;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for DeEsserFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = DeEsserFx::new();
        let out = fx.process(0.5, 0.5, 0.0, 1.0, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn passthrough_when_amount_zero() {
        // amount=0 → no ducking ever happens, so output ≈ input
        // (modulo the tiny biquad recombination error from
        // numerical roundoff).  Warm up the filters first.
        let mut fx = DeEsserFx::new();
        for _ in 0..200 {
            fx.process(0.5, 0.5, 0.0, 0.0, 1.0, 48_000.0);
        }
        let dry = 0.7_f32;
        let out = fx.process(dry, 0.5, 0.0, 0.0, 1.0, 48_000.0);
        assert!(
            (out - dry).abs() < 1e-3,
            "amount=0 transparent (got {out}, want {dry})"
        );
    }

    #[test]
    fn sibilant_signal_gets_ducked() {
        // Drive a 6 kHz sine (right in the sibilant band) at full
        // amplitude.  With low threshold + max amount + max mix,
        // the output peak should be noticeably below the input
        // peak — the FX is doing its job.
        let mut fx = DeEsserFx::new();
        let mut peak_in = 0.0_f32;
        let mut peak_out = 0.0_f32;
        for i in 0..4_800 {
            let sig = (i as f32 * TAU * 6_000.0 / 48_000.0).sin();
            let out = fx.process(sig, 0.5, 0.1, 1.0, 1.0, 48_000.0);
            // Skip the warmup window for the peak-out measurement.
            if i >= 480 {
                peak_in = peak_in.max(sig.abs());
                peak_out = peak_out.max(out.abs());
            }
        }
        // Sibilant band is reduced.  Bound chosen at 0.8 because at
        // the LP cutoff (6 kHz) the low band still carries ≈ -3 dB
        // of the input — phase relationships during recombination
        // mean the dropoff at the FX output is gentler than the
        // gain reduction applied to the band itself.
        assert!(
            peak_out < peak_in * 0.8,
            "sibilant gets ducked (peak in {peak_in}, out {peak_out})"
        );
    }

    #[test]
    fn low_frequency_signal_passes_unchanged() {
        // A 100 Hz sine has nothing in the sibilant band; the
        // de-esser should leave it alone regardless of amount.
        let mut fx = DeEsserFx::new();
        // Warmup.
        for i in 0..480 {
            let sig = (i as f32 * TAU * 100.0 / 48_000.0).sin();
            fx.process(sig, 0.5, 0.1, 1.0, 1.0, 48_000.0);
        }
        let mut max_diff = 0.0_f32;
        for i in 480..2_400 {
            let sig = (i as f32 * TAU * 100.0 / 48_000.0).sin();
            let out = fx.process(sig, 0.5, 0.1, 1.0, 1.0, 48_000.0);
            max_diff = max_diff.max((out - sig).abs());
        }
        assert!(
            max_diff < 0.05,
            "low-frequency signal passes through (max diff {max_diff})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = DeEsserFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 0.5, 0.1, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 1.5, "de-esser bounded at full drive (peak {peak})");
    }
}
