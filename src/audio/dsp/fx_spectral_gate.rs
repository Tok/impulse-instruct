// ─── audio/dsp/fx_spectral_gate.rs ────────────────────────────────────────────
// Spectral Gate FX — per-band amplitude gating across either an
// 8-band parallel BPF bank (V1, fast pragmatic approximation) or a
// true STFT pipeline (V2 — textbook windowed-FFT → per-bin gate →
// IFFT → overlap-add).  The `stft` flag picks the path.
//
// V1 (BPF mode, default): 8 RBJ constant-0-dB-peak-gain BPF
// biquads spanning ~80 Hz to ~16 kHz, each with its own envelope
// follower + gate.  Subtractive recombination
// `output = input - Σ (1 - gate_i) * band_i` keeps reconstruction
// exact when every gate is open.
//
// V2 (STFT mode): 1024-point Hann-windowed FFT, hop 256 (75 %
// overlap, COLA-compliant for Hann), per-bin amplitude gate on
// the magnitude spectrum, IFFT, synthesis-windowed overlap-add
// into the output ring.  Higher freq resolution (~47 Hz / bin
// at 48 kHz) at the cost of ~21 ms latency.
//
// Distinct from `FxGate` (broadband single-band envelope gate)
// and `FxFreeze` (held buffer / spectral freeze of the entire
// signal).  Allocation-free in the audio callback; FFT plans +
// scratch sized at construction.

use std::f32::consts::TAU;
use std::sync::Arc;

use super::dsp_util::nyquist_guard;
use super::dsp_util::{AUDIBLE_HZ_MIN, MIX_BYPASS_THRESHOLD, one_pole_coef};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

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
        let f = fc.clamp(AUDIBLE_HZ_MIN, nyquist_guard(sr));
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

// ─── STFT path constants ─────────────────────────────────────────────────────
//
// FFT size 1024 + hop 256 → 75 % overlap.  Hann window is COLA at
// 50 % and 75 % overlap (a constant-1 sum), so weighting input ×
// Hann + output × Hann + 4-frame overlap-add reconstructs unity
// in the absence of any per-bin attenuation.  At 48 kHz this is
// ~21 ms / frame, ~5 ms / hop, ~47 Hz / bin.

const FFT_SIZE: usize = 1024;
const HOP_SIZE: usize = 256;
const N_BINS: usize = FFT_SIZE / 2 + 1;
/// `4 = FFT_SIZE / HOP_SIZE`.  Per-frame Hann-square sum is
/// constant; dividing by this normalisation factor keeps the
/// reconstructed amplitude at unity.  Encoded explicitly because
/// the `(window * window).sum() / HOP_SIZE` value depends on
/// both the analysis and synthesis windows being Hann.
const COLA_NORM: f32 = 1.5;

pub(crate) struct SpectralGateFx {
    // ── BPF path (V1) ───────────────────────────────────────────────────
    bands: [Biquad; NUM_BANDS],
    env: [f32; NUM_BANDS],
    /// Per-band gate state — smoothed toward 1 (open) or 0
    /// (closed) at the configured attack / release rates.
    gate: [f32; NUM_BANDS],
    cached_sr: f32,

    // ── STFT path (V2) ──────────────────────────────────────────────────
    /// Forward + inverse FFT plans, allocated once.
    fft_fwd: Arc<dyn Fft<f32>>,
    fft_inv: Arc<dyn Fft<f32>>,
    /// Hann window — used as both analysis and synthesis (the
    /// standard square-window scheme; `Hann²` is COLA at 75 %
    /// overlap).
    hann: [f32; FFT_SIZE],
    /// Input ring — newest sample at `in_pos`.
    in_buf: [f32; FFT_SIZE],
    in_pos: usize,
    /// Samples since the last FFT frame fired.  When this hits
    /// `HOP_SIZE`, run a frame and reset.
    hop_counter: usize,
    /// Output ring — overlap-add target.  Sized FFT_SIZE so a
    /// fresh frame's contribution fully decays before the slot
    /// is re-read.
    out_buf: [f32; FFT_SIZE],
    out_pos: usize,
    /// Per-bin gate envelope state — smoothed gate value, 0..1,
    /// per positive-frequency bin (DC + Nyquist included).
    bin_gate: [f32; N_BINS],
    /// Scratch + working buffers for FFT.  Owned by the struct
    /// so `process` doesn't allocate.
    fft_scratch: Vec<Complex<f32>>,
    fft_buf: Vec<Complex<f32>>,
}

impl SpectralGateFx {
    pub(crate) fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(FFT_SIZE);
        let scratch_len = fft_fwd
            .get_inplace_scratch_len()
            .max(fft_inv.get_inplace_scratch_len());
        let mut hann = [0.0_f32; FFT_SIZE];
        for (i, w) in hann.iter_mut().enumerate() {
            // Standard Hann: 0.5 (1 - cos(2π i / (N - 1))).  Using
            // (N - 1) in the denominator keeps the window
            // symmetric — first and last samples are exactly 0.
            let t = i as f32 / (FFT_SIZE - 1) as f32;
            *w = 0.5 * (1.0 - (TAU * t).cos());
        }
        Self {
            bands: [Biquad::new(); NUM_BANDS],
            env: [0.0; NUM_BANDS],
            // Start with all gates open so a freshly-inserted FX
            // is transparent until the user dials threshold up.
            gate: [1.0; NUM_BANDS],
            cached_sr: f32::NAN,
            fft_fwd,
            fft_inv,
            hann,
            in_buf: [0.0; FFT_SIZE],
            in_pos: 0,
            hop_counter: 0,
            out_buf: [0.0; FFT_SIZE],
            out_pos: 0,
            bin_gate: [1.0; N_BINS],
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            fft_buf: vec![Complex::new(0.0, 0.0); FFT_SIZE],
        }
    }

    /// `stft`:    false = 8-band BPF approximation (V1, fast),
    ///            true = true STFT path (windowed FFT, per-bin
    ///            gate, overlap-add).  STFT adds ~21 ms latency
    ///            but resolves down to ~47 Hz/bin at 48 kHz.
    /// `thresh`:  0..1 — linear amplitude threshold.  Bands/bins
    ///            whose envelope sits above this stay open; below,
    ///            they decay toward closed.
    /// `release`: 0..1 → 10..2000 ms.  Long = "freeze low-level
    ///            resonance" feel where bands that fired stay
    ///            open longer; short = quick spectral gating.
    /// `tilt`:    0..1 — threshold skew across the spectrum.
    ///            0.5 = uniform; <0.5 = highs gate more easily;
    ///            >0.5 = lows gate more easily.
    /// `mix`:     0..1 — wet/dry blend (0 = bypass).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        input: f32,
        stft: bool,
        thresh: f32,
        release: f32,
        tilt: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }

        if stft {
            return self.process_stft(input, thresh, release, tilt, mix, sr);
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
        let release_coef = one_pole_coef(release_ms * 0.001, sr);
        let attack_coef = one_pole_coef(0.005, sr); // ~5 ms attack — fast
        // env-follower attack (separate from gate attack).
        let env_attack = one_pole_coef(0.003, sr);

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

    /// STFT path — windowed-FFT analysis, per-bin amplitude gate,
    /// IFFT, synthesis-windowed overlap-add.  Per-sample API; the
    /// frame fires every `HOP_SIZE` samples internally.  Latency
    /// is roughly `FFT_SIZE - HOP_SIZE` samples (~16 ms at 48 k).
    fn process_stft(
        &mut self,
        input: f32,
        thresh: f32,
        release: f32,
        tilt: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        // Capture the input sample.
        self.in_buf[self.in_pos] = input;
        self.in_pos = (self.in_pos + 1) % FFT_SIZE;

        // Read this sample's wet output (consumed before the
        // overlap-add stomps it on the next frame).
        let wet = self.out_buf[self.out_pos];
        self.out_buf[self.out_pos] = 0.0; // clear so OLA can accumulate fresh
        self.out_pos = (self.out_pos + 1) % FFT_SIZE;

        // Frame-fire on hop boundary.
        self.hop_counter += 1;
        if self.hop_counter >= HOP_SIZE {
            self.hop_counter = 0;
            self.run_stft_frame(thresh, release, tilt, sr);
        }

        input * (1.0 - mix) + wet * mix
    }

    /// Run one analysis-process-synthesis cycle.  Reads the most
    /// recent FFT_SIZE input samples, gates the magnitude
    /// spectrum against threshold (with tilt skew), then OLAs the
    /// reconstructed time-domain frame into `out_buf`.
    fn run_stft_frame(&mut self, thresh: f32, release: f32, tilt: f32, sr: f32) {
        // Pack the windowed input into the FFT buffer.  Walk the
        // ring starting `FFT_SIZE` samples behind in_pos so the
        // most recent sample lands at index FFT_SIZE - 1.
        for k in 0..FFT_SIZE {
            let ring_idx = (self.in_pos + k) % FFT_SIZE;
            let s = self.in_buf[ring_idx] * self.hann[k];
            self.fft_buf[k] = Complex::new(s, 0.0);
        }
        self.fft_fwd
            .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);

        // Gate envelope smoothing.  Per-frame attack/release
        // works on `bin_gate`; release is configurable, attack is
        // fixed-fast (~one frame).
        let release_ms = 10.0 * 200.0_f32.powf(release.clamp(0.0, 1.0));
        // Frame-rate release coefficient — convert ms to frames.
        let frame_dt_s = HOP_SIZE as f32 / sr;
        let release_coef = (-frame_dt_s / (release_ms * 0.001)).exp();
        let attack_coef = (-frame_dt_s / 0.005).exp(); // ~5 ms attack
        let base_thr = thresh.clamp(0.0, 1.0);

        // Gate each positive-frequency bin (DC..Nyquist) against
        // a threshold; mirror to the conjugate-symmetric bin so
        // the IFFT stays real.
        for k in 0..N_BINS {
            // Linear magnitude — bin is complex; rustfft is
            // unnormalised so an FFT of a unity Hann frame has
            // peak magnitude of `Σ hann ≈ FFT_SIZE/2`.  Scale
            // back to a 0..1-ish range for the threshold knob.
            let mag = self.fft_buf[k].norm() / (FFT_SIZE as f32 * 0.5);
            // Tilt skew across the bin range.  band_t goes 0
            // (DC) → 1 (Nyquist).  Same gentle ±2× shape as the
            // BPF path so the knob feels consistent across modes.
            let band_t = k as f32 / (N_BINS - 1) as f32;
            let tilt_factor = if tilt < 0.5 {
                1.0 + (1.0 - 2.0 * tilt) * band_t
            } else {
                1.0 + (2.0 * tilt - 1.0) * (1.0 - band_t)
            };
            let band_thr = (base_thr * tilt_factor).clamp(0.0, 2.0);
            let target = if mag >= band_thr { 1.0 } else { 0.0 };
            let coef = if target > self.bin_gate[k] {
                attack_coef
            } else {
                release_coef
            };
            self.bin_gate[k] = target + (self.bin_gate[k] - target) * coef;
            // Apply gate to the bin and its conjugate twin.
            self.fft_buf[k] *= self.bin_gate[k];
            // Mirror for k > 0 && k < N_BINS-1; the boundary bins
            // (DC + Nyquist) are real-valued and don't need a
            // partner.
            if k > 0 && k < N_BINS - 1 {
                let mirror = FFT_SIZE - k;
                // Conjugate: real same sign, imag flipped.
                self.fft_buf[mirror] = Complex::new(self.fft_buf[k].re, -self.fft_buf[k].im);
            }
        }

        // IFFT — rustfft inverse is unnormalised; scale by
        // 1/FFT_SIZE.  The synthesis window (Hann again) +
        // 75 % overlap-add reconstructs to a constant sum;
        // dividing by COLA_NORM normalises to unity.
        self.fft_inv
            .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);
        let scale = 1.0 / (FFT_SIZE as f32 * COLA_NORM);
        // OLA: walk forward from `out_pos` by FFT_SIZE samples,
        // adding `windowed * scale` into the ring.  Note: we
        // already advanced out_pos one slot at the top of
        // process_stft, so the frame's first sample lands one
        // slot AHEAD of the read head — guarantees the freshly-
        // gated frame contributes to future reads, not the
        // sample we already returned.
        for k in 0..FFT_SIZE {
            let ola_idx = (self.out_pos + k) % FFT_SIZE;
            self.out_buf[ola_idx] += self.fft_buf[k].re * self.hann[k] * scale;
        }
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
        let out = fx.process(0.5, false, 0.5, 0.5, 0.5, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn passthrough_when_threshold_zero() {
        // Threshold = 0 → every band's env is always above
        // threshold → every gate stays at 1 → wet = input - 0 =
        // input.  Allow tiny numerical slack for the smoothing.
        let mut fx = SpectralGateFx::new();
        for _ in 0..1_000 {
            fx.process(0.5, false, 0.0, 0.5, 0.5, 1.0, 48_000.0);
        }
        let dry = 0.7_f32;
        let out = fx.process(dry, false, 0.0, 0.5, 0.5, 1.0, 48_000.0);
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
            let out = fx.process(sig, false, 0.5, 0.0, 0.5, 1.0, 48_000.0);
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
            let out = fx.process(sig, false, 0.05, 0.0, 0.5, 1.0, 48_000.0);
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
            let out = fx.process(sig, false, 0.5, 1.0, 0.5, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 2.0, "spec gate bounded at full drive (peak {peak})");
    }

    // ── STFT-mode tests ─────────────────────────────────────────────────
    //
    // The STFT path adds ~16 ms of latency (FFT_SIZE - HOP_SIZE
    // samples), so transparency assertions allow a longer warmup
    // and looser tolerances than the BPF path.

    #[test]
    fn stft_passthrough_when_threshold_zero() {
        // Threshold = 0 → every bin's gate stays at 1 → output
        // tracks the input after the STFT pipeline's latency.
        // Drive a 1 kHz sine through and confirm the output peak
        // approaches the input peak after enough hops.
        let mut fx = SpectralGateFx::new();
        let mut peak_late = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * TAU * 1_000.0 / 48_000.0).sin();
            let out = fx.process(sig, true, 0.0, 0.5, 0.5, 1.0, 48_000.0);
            assert!(out.is_finite());
            // Skip the first half — STFT latency + gate envelope
            // ramp.  After that the output should track unity.
            if i >= 8_000 {
                peak_late = peak_late.max(out.abs());
            }
        }
        assert!(
            peak_late > 0.7,
            "STFT threshold=0 should preserve signal (peak {peak_late})"
        );
    }

    #[test]
    fn stft_silences_quiet_signal_above_threshold() {
        // Quiet input + high threshold → all bins gate closed →
        // output decays to silence.
        let mut fx = SpectralGateFx::new();
        let mut peak_late = 0.0_f32;
        for i in 0..16_000 {
            // 1 kHz sine at amplitude 0.05 — every bin's
            // magnitude lands well below thresh=0.5.
            let sig = (i as f32 * TAU * 1_000.0 / 48_000.0).sin() * 0.05;
            let out = fx.process(sig, true, 0.5, 0.0, 0.5, 1.0, 48_000.0);
            if i >= 12_000 {
                peak_late = peak_late.max(out.abs());
            }
        }
        assert!(
            peak_late < 0.05,
            "STFT high threshold silences quiet signal (peak {peak_late})"
        );
    }

    #[test]
    fn stft_output_bounded_under_full_drive() {
        let mut fx = SpectralGateFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, true, 0.5, 1.0, 0.5, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 2.0, "STFT bounded at full drive (peak {peak})");
    }
}
