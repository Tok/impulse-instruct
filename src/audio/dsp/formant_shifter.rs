// ─── audio/dsp/formant_shifter.rs ────────────────────────────────────────────
// Formant-preserving pitch shifter — phase vocoder with spectral
// envelope flatten/restore.  V2 Stage 8 of SampleInstrument.
//
// Algorithm per analysis frame:
//   1. STFT analyse the input (Hann-windowed, FFT_SIZE=512, 75% OLA).
//   2. Compute log-magnitude → smooth with a moving-average filter
//      (cepstral-domain lift approximation) → envelope.
//   3. Whiten: mag_excitation[k] = mag_input[k] / envelope[k].
//   4. Pitch-shift the excitation bins by `ratio` (linear interp
//      between source bins).  Phase vocoder phase coherence keeps
//      successive frames in sync.
//   5. Re-multiply by the *original* envelope at unshifted bin
//      positions — the spectral envelope (and therefore formants)
//      stays put while the harmonic excitation moves.
//   6. ISTFT back, Hann-window again, overlap-add into the output
//      ring.
//
// Cost per slot: one 512-FFT + one 512-IFFT per ~3 ms (75 % overlap).
// State: ~12 KB per shifter (in/out rings + phase memory + scratch).
// At 8 slots that's ~100 KB total for the polyphony pool — fine for
// the audio thread.  All allocations happen in `new`; the realtime
// `process` does no allocations.

use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

const FFT_SIZE: usize = 512;
const HOP_SIZE: usize = 128; // 75% overlap
const BINS: usize = FFT_SIZE / 2 + 1;
/// Half-width of the moving-average smoother applied to log-magnitude.
/// 7 bins ≈ 660 Hz at 48 kHz — wide enough that the envelope skips the
/// pitch's harmonic spacing (formants are smooth on that scale) but
/// narrow enough to track the formants themselves.
const ENVELOPE_SMOOTH_HALF: usize = 7;
/// Inverse-FFT rescale — rustfft's inverse isn't normalised.  Combined
/// with the per-frame Hann² OLA gain to produce unity-amplitude output
/// when ratio == 1 and envelope-preservation is a no-op.
const INV_FFT_NORM: f32 = 1.0 / FFT_SIZE as f32;

pub(crate) struct FormantShifter {
    fft_fwd: Arc<dyn Fft<f32>>,
    fft_inv: Arc<dyn Fft<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    /// FFT working buffer.
    work: Vec<Complex<f32>>,
    /// Pre-computed Hann window of length FFT_SIZE.
    hann: Vec<f32>,
    /// Hann² normalisation scale — since we apply the window twice
    /// (analysis + synthesis), the OLA gain at 75 % overlap is
    /// 1.5 × per sample.  Divided into the final write so the
    /// pass-through case (ratio = 1) is unity gain.
    ola_norm: f32,
    /// Input ring — last FFT_SIZE samples, written linearly via
    /// `in_pos` and wrapping at FFT_SIZE.
    in_buf: Vec<f32>,
    in_pos: usize,
    /// Output ring — overlap-add accumulator.  At least
    /// FFT_SIZE + HOP_SIZE long; we use 2 × FFT_SIZE for headroom.
    out_buf: Vec<f32>,
    out_read: usize,
    out_write: usize,
    /// Sample counter since last frame trigger.
    hop_counter: usize,
    /// Per-bin analysis-phase memory (last frame's phase, used to
    /// compute the bin's instantaneous frequency).
    last_phase_in: Vec<f32>,
    /// Per-bin synthesis-phase accumulator — phase coherence keeps
    /// successive frames consistent.
    sum_phase_out: Vec<f32>,
    /// Reusable magnitude / envelope scratch.
    mag_in: Vec<f32>,
    log_mag: Vec<f32>,
    envelope: Vec<f32>,
    mag_shifted: Vec<f32>,
    phase_shifted: Vec<f32>,
}

impl FormantShifter {
    pub(crate) fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(FFT_SIZE);
        let scratch_len = fft_fwd
            .get_inplace_scratch_len()
            .max(fft_inv.get_inplace_scratch_len());
        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / FFT_SIZE as f32).cos())
            .collect();
        // Sum of Hann² values across one period at 75 % overlap = 1.5.
        // Normalising by this gives unity OLA gain.
        let ola_norm = 1.0 / 1.5;
        Self {
            fft_fwd,
            fft_inv,
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            work: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            hann,
            ola_norm,
            in_buf: vec![0.0; FFT_SIZE],
            in_pos: 0,
            out_buf: vec![0.0; FFT_SIZE * 2],
            out_read: 0,
            out_write: 0,
            hop_counter: 0,
            last_phase_in: vec![0.0; BINS],
            sum_phase_out: vec![0.0; BINS],
            mag_in: vec![0.0; BINS],
            log_mag: vec![0.0; BINS],
            envelope: vec![0.0; BINS],
            mag_shifted: vec![0.0; BINS],
            phase_shifted: vec![0.0; BINS],
        }
    }

    /// Reset all state — called when a slot retriggers so the new
    /// note doesn't carry state from a previous trigger.
    pub(crate) fn reset(&mut self) {
        self.in_buf.iter_mut().for_each(|s| *s = 0.0);
        self.out_buf.iter_mut().for_each(|s| *s = 0.0);
        self.last_phase_in.iter_mut().for_each(|s| *s = 0.0);
        self.sum_phase_out.iter_mut().for_each(|s| *s = 0.0);
        self.in_pos = 0;
        self.out_read = 0;
        self.out_write = 0;
        self.hop_counter = 0;
    }

    /// Process one input sample and return one output sample.  When
    /// `ratio == 1.0` (or close to it) the path collapses to a delayed
    /// pass-through — useful for early sustain when the user hasn't
    /// detuned yet.  Otherwise the spectrum is whitened, shifted, and
    /// re-enveloped.
    pub(crate) fn process(&mut self, input: f32, ratio: f32) -> f32 {
        // Push the incoming sample into the input ring.
        self.in_buf[self.in_pos] = input;
        self.in_pos = (self.in_pos + 1) % FFT_SIZE;

        self.hop_counter += 1;
        if self.hop_counter >= HOP_SIZE {
            self.hop_counter = 0;
            self.run_frame(ratio);
        }

        // Read from the output ring.  Pre-clear the slot we just read
        // so the next OLA write starts from zero (avoids accumulator
        // drift over long runs).
        let out = self.out_buf[self.out_read] * self.ola_norm;
        self.out_buf[self.out_read] = 0.0;
        self.out_read = (self.out_read + 1) % self.out_buf.len();
        out
    }

    fn run_frame(&mut self, ratio: f32) {
        let ratio = ratio.clamp(0.25, 4.0); // ±2 octaves keeps the bin
        // shift well-defined; well outside that, FFT-bin pitch
        // shifting starts to alias badly.

        // Window in_buf into work in time order (oldest first).
        for i in 0..FFT_SIZE {
            let idx = (self.in_pos + i) % FFT_SIZE;
            self.work[i] = Complex::new(self.in_buf[idx] * self.hann[i], 0.0);
        }
        self.fft_fwd
            .process_with_scratch(&mut self.work, &mut self.fft_scratch);

        // Magnitude + log-magnitude per bin.
        for k in 0..BINS {
            let m = self.work[k].norm();
            self.mag_in[k] = m;
            self.log_mag[k] = m.max(1e-9).ln();
        }

        // Cepstral-domain envelope approximation: smooth log-magnitude
        // with a moving-average filter.  Wider window = smoother
        // envelope (fewer wiggles tracking individual harmonics).
        smooth_log_magnitude(&self.log_mag, &mut self.envelope, ENVELOPE_SMOOTH_HALF);
        for k in 0..BINS {
            self.envelope[k] = self.envelope[k].exp();
        }

        // Whiten + pitch-shift bins.  Each output bin k pulls from
        // input bin k / ratio (linearly interpolated between two
        // source bins).  Phase vocoder coherence: track each input
        // bin's phase advance vs. its expected free-running advance,
        // accumulate the *true* freq advance × ratio into the synth
        // phase.
        let two_pi = std::f32::consts::TAU;
        let expected_advance = two_pi * HOP_SIZE as f32 / FFT_SIZE as f32;
        for k in 0..BINS {
            let mag = self.mag_in[k];
            let phase = self.work[k].arg();
            let phase_diff = phase - self.last_phase_in[k];
            self.last_phase_in[k] = phase;
            // Wrap phase difference to [-π, π].
            let expected_for_bin = expected_advance * k as f32;
            let dev = phase_diff - expected_for_bin;
            let dev_wrapped = wrap_pi(dev);
            let true_freq_advance = expected_for_bin + dev_wrapped;
            // Synthesised bin position.
            let src_bin_f = k as f32 / ratio;
            let src_lo = src_bin_f.floor() as usize;
            let src_hi = src_lo + 1;
            if src_hi >= BINS {
                self.mag_shifted[k] = 0.0;
                self.phase_shifted[k] = 0.0;
                continue;
            }
            let frac = src_bin_f - src_lo as f32;
            // Whitened magnitude: source mag / source envelope.  Two
            // bins, linearly interpolated.
            let mag_lo = mag_at(&self.mag_in, &self.envelope, src_lo);
            let mag_hi = mag_at(&self.mag_in, &self.envelope, src_hi);
            let exc_mag = mag_lo + (mag_hi - mag_lo) * frac;
            // Re-apply ENVELOPE at the destination bin (not the source
            // bin) — this is the crucial step that keeps formants
            // anchored in place while harmonics move.
            self.mag_shifted[k] = exc_mag * self.envelope[k];
            // Synth phase: previous synth phase + ratio-scaled true
            // freq advance.  Wrapping happens in the output `arg`.
            self.sum_phase_out[k] += true_freq_advance * ratio;
            self.phase_shifted[k] = self.sum_phase_out[k];
            // Suppress unused warning when ratio = 1 makes mag/phase
            // moot — the log_mag scratch is only valid through this
            // pass.  Touch it so the compiler keeps the binding.
            let _ = mag;
        }

        // Reconstruct complex spectrum (Hermitian-symmetric for real
        // output) + IFFT.
        for k in 0..BINS {
            let mag = self.mag_shifted[k];
            let phase = self.phase_shifted[k];
            let (s, c) = phase.sin_cos();
            self.work[k] = Complex::new(mag * c, mag * s);
        }
        for k in 1..FFT_SIZE / 2 {
            self.work[FFT_SIZE - k] = self.work[k].conj();
        }
        // DC + Nyquist must be real-valued; force.
        self.work[0].im = 0.0;
        self.work[FFT_SIZE / 2].im = 0.0;

        self.fft_inv
            .process_with_scratch(&mut self.work, &mut self.fft_scratch);

        // Window again (synthesis) + overlap-add into the output ring.
        for i in 0..FFT_SIZE {
            let idx = (self.out_write + i) % self.out_buf.len();
            self.out_buf[idx] += self.work[i].re * INV_FFT_NORM * self.hann[i];
        }
        self.out_write = (self.out_write + HOP_SIZE) % self.out_buf.len();
    }
}

/// Smooth `log_mag` with a centred moving-average of half-width `half`.
/// Edges use shorter windows (clamp) so the boundary bins don't go to
/// zero — important because DC and Nyquist often carry real spectral
/// energy.
fn smooth_log_magnitude(log_mag: &[f32], out: &mut [f32], half: usize) {
    let n = log_mag.len();
    for k in 0..n {
        let lo = k.saturating_sub(half);
        let hi = (k + half + 1).min(n);
        let mut sum = 0.0;
        let mut count = 0;
        for &v in &log_mag[lo..hi] {
            sum += v;
            count += 1;
        }
        out[k] = if count > 0 {
            sum / count as f32
        } else {
            log_mag[k]
        };
    }
}

/// Return `mag_in[idx] / envelope[idx]` with safe clamping.  Used by
/// the bin-interpolation pitch shift to whiten the source magnitude.
fn mag_at(mag_in: &[f32], envelope: &[f32], idx: usize) -> f32 {
    if idx >= mag_in.len() {
        return 0.0;
    }
    let env = envelope[idx].max(1e-6);
    mag_in[idx] / env
}

/// Wrap `x` to [-π, π].  Uses rem_euclid instead of nested branches
/// so the path stays consistent for very negative or very positive
/// inputs.
fn wrap_pi(x: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut y = (x + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI;
    // Floating-point edge: rem_euclid can return -PI exactly; nudge
    // to +PI for symmetry with downstream consumers.
    if y < -std::f32::consts::PI + 1e-9 {
        y += two_pi;
    }
    y
}
