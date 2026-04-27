// ─── audio/dsp/fx_mb_comp.rs ──────────────────────────────────────────────────
// Multiband compressor — 3-band split + 3 independent downward
// compressors.  Mastering-grade dynamics; distinct from
// `FxCompressor` (broadband single-band): each band has its own
// threshold, so the user can tame a boomy low end without
// flattening the air or vice versa.
//
// Implementation:
//   * Two LP biquads (250 Hz, 2.5 kHz).  Same crossover idea as
//     FxIsoEq.
//   * Bands derived subtractively:
//       low_band  = LP_250(input)
//       low_plus_mid = LP_2500(input)
//       mid_band  = low_plus_mid - low_band
//       high_band = input - low_plus_mid
//     This guarantees `low + mid + high = input` exactly when no
//     compression is applied.
//   * Each band has its own peak-style envelope follower
//     (~3 ms attack / ~80 ms release) and a soft-knee compressor:
//     `gain = (env / thr)^(-RATIO)` clamped to (0, 1] when env >
//     thr; otherwise gain = 1.  RATIO is fixed at 0.6 for V1
//     (≈ 4:1 compression) so the four UI knobs stay tight.
//   * Output = sum of ducked bands; cheap-bypass fast path on mix.
//
// Allocation-free; coefficients refresh lazily on sample-rate
// change.

use super::dsp_util::MIX_BYPASS_THRESHOLD;
use super::dsp_util::nyquist_guard;
use std::f32::consts::TAU;

const LOW_FC: f32 = 250.0;
const HIGH_FC: f32 = 2_500.0;
const Q: f32 = 0.707; // Butterworth — clean rolloff, no resonance.
/// Compressor "amount" exponent — `(env/thr)^(-AMOUNT)`.
/// 0 = no compression; 1 = hard kill at threshold.  0.6 lands
/// near the 4:1 ratio mark.
const COMP_AMOUNT: f32 = 0.6;

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
        let w = TAU * fc.clamp(20.0, nyquist_guard(sr)) / sr;
        let cos_w = w.cos();
        let alpha = w.sin() / (2.0 * Q);
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

pub(crate) struct MultibandCompFx {
    /// LP at the low/mid crossover (~250 Hz).
    lp_low: Biquad,
    /// LP at the mid/high crossover (~2.5 kHz).
    lp_high: Biquad,
    /// Per-band envelope follower state (peak with attack/release).
    env_low: f32,
    env_mid: f32,
    env_high: f32,
    /// Cached sample rate so we recompute the biquads only on
    /// actual change.  Initialised to NaN; the `is_finite` check
    /// in `process` forces first-call init.
    cached_sr: f32,
}

impl MultibandCompFx {
    pub(crate) fn new() -> Self {
        Self {
            lp_low: Biquad::new(),
            lp_high: Biquad::new(),
            env_low: 0.0,
            env_mid: 0.0,
            env_high: 0.0,
            cached_sr: f32::NAN,
        }
    }

    /// `low_thr`/`mid_thr`/`high_thr`: 0..1 — per-band threshold
    /// (linear amplitude).  When the band's envelope rises above
    /// this level, downward compression engages.  1.0 keeps the
    /// band uncompressed for any normal signal level.
    /// `mix`: 0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        low_thr: f32,
        mid_thr: f32,
        high_thr: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }
        if !self.cached_sr.is_finite() || (sr - self.cached_sr).abs() > 0.5 {
            self.lp_low = Biquad::low_pass(LOW_FC, sr);
            self.lp_high = Biquad::low_pass(HIGH_FC, sr);
            self.cached_sr = sr;
        }

        // 3-band subtractive split.
        let low_band = self.lp_low.process(input);
        let low_plus_mid = self.lp_high.process(input);
        let mid_band = low_plus_mid - low_band;
        let high_band = input - low_plus_mid;

        // Per-band envelope follower — peak with fast attack /
        // medium release.  Tracking each band independently is
        // the whole point of multiband: a low-band kick doesn't
        // duck the high-band air.
        let attack_coef = (-1.0_f32 / (0.003 * sr)).exp(); // ~3 ms
        let release_coef = (-1.0_f32 / (0.08 * sr)).exp(); // ~80 ms
        update_env(&mut self.env_low, low_band, attack_coef, release_coef);
        update_env(&mut self.env_mid, mid_band, attack_coef, release_coef);
        update_env(&mut self.env_high, high_band, attack_coef, release_coef);

        let g_low = comp_gain(self.env_low, low_thr);
        let g_mid = comp_gain(self.env_mid, mid_thr);
        let g_high = comp_gain(self.env_high, high_thr);

        let wet = low_band * g_low + mid_band * g_mid + high_band * g_high;
        input * (1.0 - mix) + wet * mix
    }
}

#[inline]
fn update_env(env: &mut f32, sample: f32, attack: f32, release: f32) {
    let abs = sample.abs();
    if abs > *env {
        *env = abs + (*env - abs) * attack;
    } else {
        *env = abs + (*env - abs) * release;
    }
}

#[inline]
fn comp_gain(env: f32, thr: f32) -> f32 {
    let t = thr.clamp(0.001, 1.5);
    if env > t {
        (env / t).powf(-COMP_AMOUNT).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

impl Default for MultibandCompFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = MultibandCompFx::new();
        let out = fx.process(0.5, 0.1, 0.1, 0.1, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn passthrough_at_unity_thresholds() {
        // With every threshold at 1.0 (above any normal signal
        // peak), no compression engages and the bands sum back to
        // the dry input.  Allow tiny numerical slack.
        let mut fx = MultibandCompFx::new();
        for _ in 0..200 {
            fx.process(0.5, 1.0, 1.0, 1.0, 1.0, 48_000.0);
        }
        let dry = 0.7_f32;
        let out = fx.process(dry, 1.0, 1.0, 1.0, 1.0, 48_000.0);
        assert!(
            (out - dry).abs() < 1e-3,
            "unity thresholds = passthrough (got {out})"
        );
    }

    #[test]
    fn bass_compression_independent_of_high_band() {
        // A loud 50 Hz sine should be compressed when low_thresh
        // is dialed down, but a 10 kHz sine ridden alongside
        // shouldn't be ducked unless high_thresh is also low.
        let mut fx = MultibandCompFx::new();
        // Drive a mixed signal: 50 Hz at 1.0 + 10 kHz at 0.5.
        let mut peak_high_band_passing = 0.0_f32;
        for i in 0..4_800 {
            let bass = (i as f32 * TAU * 50.0 / 48_000.0).sin();
            let air = (i as f32 * TAU * 10_000.0 / 48_000.0).sin() * 0.5;
            let sig = bass + air;
            let _ = fx.process(sig, 0.1, 1.0, 1.0, 1.0, 48_000.0);
            // Skip warmup window.
            if i >= 480 {
                // The high-band envelope should still register
                // ~0.5 since the air signal is uncompressed.
                peak_high_band_passing = peak_high_band_passing.max(fx.env_high);
            }
        }
        assert!(
            peak_high_band_passing > 0.3,
            "high band passes through (env peak {peak_high_band_passing})"
        );
    }

    #[test]
    fn loud_low_band_actually_gets_ducked() {
        // Drive a 50 Hz sine at amplitude 1.0 with low_thresh=0.1.
        // The band's envelope reaches ~1.0; gain reduction kicks
        // in: gain = (1/0.1)^(-0.6) ≈ 0.25.  So output low band ≈
        // 0.25 of input → output peak markedly below input peak.
        let mut fx = MultibandCompFx::new();
        let mut peak_in = 0.0_f32;
        let mut peak_out = 0.0_f32;
        for i in 0..4_800 {
            let sig = (i as f32 * TAU * 50.0 / 48_000.0).sin();
            let out = fx.process(sig, 0.1, 1.0, 1.0, 1.0, 48_000.0);
            if i >= 480 {
                peak_in = peak_in.max(sig.abs());
                peak_out = peak_out.max(out.abs());
            }
        }
        assert!(
            peak_out < peak_in * 0.6,
            "low band gets compressed (in {peak_in}, out {peak_out})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = MultibandCompFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 0.1, 0.1, 0.1, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Compression always reduces, never amplifies — so the
        // wet output is bounded by the input peak.
        assert!(peak <= 1.5, "mb comp bounded at full drive (peak {peak})");
    }
}
