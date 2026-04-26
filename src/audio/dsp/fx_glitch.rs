// ─── audio/dsp/fx_glitch.rs ───────────────────────────────────────────────────
// Glitch / time-domain performance FX.  Three structs, each
// captures input into a buffer and replays it with some
// transformation that fundamentally re-times the output:
//
//   * `TapeStop` — slows the playback rate from 1.0 to 0.0,
//     simulating a deck spinning down.
//   * `Stutter`  — captures a beat-synced slice + loops it.
//   * `Freeze`   — FFT capture, random-phase resynth (spectral
//                  freeze; magnitudes hold, phases re-roll).
//
// Split out of `fx_extras.rs` to keep that file under the
// 1000-line cap; behaviour is unchanged.  Same `pub(crate)`
// exposure as the originals.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex};

// ─── Tape stop ───────────────────────────────────────────────────────────────
//
// Mix knob doubles as ramp progress — 0 = normal pass-through, 1 = fully
// stopped (silent).  Internally maintains a delay line; the read head's
// playback rate ramps from 1.0 down to 0.0 as `mix` rises, simulating the
// platter winding to a halt.  A tone-darkening lowpass that tracks the
// rate keeps the signal from sounding edgy as it slows.

const TAPESTOP_BUF: usize = 96_000; // 2 s @ 48 kHz

pub(crate) struct TapeStop {
    buf: Box<[f32; TAPESTOP_BUF]>,
    write: usize,
    /// Fractional read head — advances by `rate` per sample.  Re-anchors to
    /// `write` whenever mix drops back to 0 (preventing drift).
    read: f32,
    /// One-pole LP state for the dynamic darkening.
    lp_state: f32,
    /// Last-frame mix to detect rising-edge re-engage.
    last_mix: f32,
}

impl TapeStop {
    pub(crate) fn new() -> Self {
        Self {
            buf: Box::new([0.0; TAPESTOP_BUF]),
            write: 0,
            read: 0.0,
            lp_state: 0.0,
            last_mix: 0.0,
        }
    }

    /// `mix`: 0–1 — also acts as the ramp position (0 = pass-through, 1
    /// = silenced).  Curve is shaped so the perceived slow-down feels
    /// closer to logarithmic than linear.
    /// `time`: 0–1 → 0.05..2 s scratch-tail buffer length cap.
    pub(crate) fn process(&mut self, input: f32, mix: f32, _time: f32, _sr: f32) -> f32 {
        // Always write the dry signal so re-engagements can pull from
        // recent material without an attack-lag glitch.
        self.buf[self.write] = input;
        self.write = (self.write + 1) % TAPESTOP_BUF;

        // Re-anchor read head when mix returns from > 0 to 0.
        if self.last_mix > 0.001 && mix < 0.001 {
            self.read = self.write as f32;
            self.lp_state = 0.0;
        }
        self.last_mix = mix;

        if mix < 0.001 {
            return input;
        }

        // Ramp curve: rate = (1 - mix)^2 — slows perceptually.
        let rate = (1.0 - mix.clamp(0.0, 1.0)).powi(2);
        // Advance read by `rate` samples per output sample.
        let mut read_pos = self.read;
        // Linear-interp read.
        let idx = read_pos as usize % TAPESTOP_BUF;
        let frac = read_pos - read_pos.floor();
        let next = (idx + 1) % TAPESTOP_BUF;
        let raw = self.buf[idx] + (self.buf[next] - self.buf[idx]) * frac;

        read_pos += rate;
        if read_pos >= TAPESTOP_BUF as f32 {
            read_pos -= TAPESTOP_BUF as f32;
        }
        self.read = read_pos;

        // Lowpass darkens with the ramp.  alpha → 0 as mix → 1.
        let alpha = (1.0 - mix.clamp(0.0, 1.0)) * 0.6 + 0.05;
        self.lp_state += alpha * (raw - self.lp_state);
        // Output is the slowed+darkened wet, scaled by (1-mix) so it
        // smoothly trails to silence as the ramp completes.
        self.lp_state * (1.0 - mix.clamp(0.0, 1.0))
    }
}

// ─── Stutter / repeater ──────────────────────────────────────────────────────
//
// Captures a slice every `period` samples and loops it for the remainder of
// the period.  `period` is derived from BPM and the user's rate
// subdivision, so the stutter is automatically beat-synced.

const STUTTER_BUF: usize = 48_000; // 1 s @ 48 kHz captures plenty of slice

pub(crate) struct Stutter {
    /// Slice buffer — captured once per period, replayed across the period.
    slice: Box<[f32; STUTTER_BUF]>,
    slice_len: usize,
    /// Position within the slice for the current period playback.
    play_pos: usize,
    /// Counts samples since the slice was last captured.
    period_pos: usize,
}

impl Stutter {
    pub(crate) fn new() -> Self {
        Self {
            slice: Box::new([0.0; STUTTER_BUF]),
            slice_len: 0,
            play_pos: 0,
            period_pos: 0,
        }
    }

    /// `rate`: 0–1 → quantised to 1/4, 1/8, 1/16, 1/32 note divisions.
    /// `slice_frac`: 0–1 → fraction of the period that's captured (rest
    /// of the period replays the captured slice).
    /// `mix`: 0–1 wet/dry.
    /// `bpm`: passed in so the period stays musically aligned.
    pub(crate) fn process(
        &mut self,
        input: f32,
        rate: f32,
        slice_frac: f32,
        mix: f32,
        bpm: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            self.period_pos = 0;
            self.play_pos = 0;
            return input;
        }
        // Quantise rate to 1/4, 1/8, 1/16, 1/32.
        let div = match (rate.clamp(0.0, 0.999) * 4.0) as usize {
            0 => 4u32,  // quarter
            1 => 8u32,  // eighth
            2 => 16u32, // sixteenth
            _ => 32u32, // thirty-second
        };
        let beat_s = 60.0 / bpm.max(20.0);
        let period_s = beat_s * 4.0 / div as f32;
        let period = ((period_s * sr) as usize).clamp(64, STUTTER_BUF);
        let cap_len = ((period as f32 * slice_frac.clamp(0.05, 1.0)) as usize)
            .clamp(8, period.min(STUTTER_BUF));

        // Capture phase: write input into the slice buffer for the first
        // `cap_len` samples of the period.
        if self.period_pos < cap_len {
            self.slice[self.period_pos] = input;
            self.slice_len = cap_len;
            self.play_pos = 0;
        }

        let wet = if self.slice_len > 0 {
            let s = self.slice[self.play_pos % self.slice_len];
            self.play_pos = (self.play_pos + 1) % self.slice_len;
            s
        } else {
            input
        };

        self.period_pos += 1;
        if self.period_pos >= period {
            self.period_pos = 0;
        }

        input * (1.0 - mix) + wet * mix
    }
}

// ─── Spectral freezer (FFT magnitudes hold + random-phase resynth) ───────────
//
// Captures one FFT frame on the rising edge of `mix > 0`; thereafter
// regenerates output frames at the same magnitudes but with fresh random
// phases per frame, IFFT'd and overlap-add'd into the output buffer.
//
// FFT_SIZE = 1024 with a 256-sample hop (75 % overlap) — standard for
// spectral-domain effects.  Both forward + inverse FFTs are pre-planned at
// construction so process() never allocates.
//
// The user knob is just `mix`: > 0 engages capture-and-hold; back to 0
// resets and resumes pass-through.  Future enhancements: re-trigger knob
// (force a new capture mid-freeze), spread knob (smear magnitudes across
// neighbouring bins for chorus-like motion).

const FREEZE_FFT_SIZE: usize = 1024;
const FREEZE_HOP_SIZE: usize = 256;
const FREEZE_BINS: usize = FREEZE_FFT_SIZE / 2 + 1;

pub(crate) struct Freeze {
    /// Forward FFT plan (pre-allocated by FftPlanner).
    fft_fwd: Arc<dyn Fft<f32>>,
    /// Inverse FFT plan.
    fft_inv: Arc<dyn Fft<f32>>,
    /// Scratch buffer for in-place FFT (sized to the larger of fwd / inv).
    fft_scratch: Vec<Complex<f32>>,
    /// Working complex buffer for FFT input/output.
    work: Vec<Complex<f32>>,
    /// Pre-computed Hann window of length FREEZE_FFT_SIZE.
    hann: Vec<f32>,
    /// Input ring buffer — last FFT_SIZE samples, ordered with `in_pos`
    /// pointing at the next write position.
    in_buf: Vec<f32>,
    in_pos: usize,
    /// Output ring buffer for overlap-add.  Must be at least
    /// FFT_SIZE + HOP samples; we use 2 * FFT_SIZE for headroom.
    out_buf: Vec<f32>,
    out_read: usize,
    out_write_anchor: usize,
    /// Captured magnitude spectrum.  Only populated when mix > 0 and we've
    /// taken a snapshot.
    captured_mag: Vec<f32>,
    captured: bool,
    /// Sample counter since last hop.
    hop_counter: usize,
    /// xorshift RNG state for per-frame phase randomisation.
    rng_state: u32,
    /// Last-frame mix to detect rising edge.
    last_mix: f32,
}

impl Freeze {
    pub(crate) fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft_fwd = planner.plan_fft_forward(FREEZE_FFT_SIZE);
        let fft_inv = planner.plan_fft_inverse(FREEZE_FFT_SIZE);
        let scratch_len = fft_fwd
            .get_inplace_scratch_len()
            .max(fft_inv.get_inplace_scratch_len());
        let hann: Vec<f32> = (0..FREEZE_FFT_SIZE)
            .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / FREEZE_FFT_SIZE as f32).cos())
            .collect();
        Self {
            fft_fwd,
            fft_inv,
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            work: vec![Complex::new(0.0, 0.0); FREEZE_FFT_SIZE],
            hann,
            in_buf: vec![0.0; FREEZE_FFT_SIZE],
            in_pos: 0,
            out_buf: vec![0.0; FREEZE_FFT_SIZE * 2],
            out_read: 0,
            out_write_anchor: 0,
            captured_mag: vec![0.0; FREEZE_BINS],
            captured: false,
            hop_counter: 0,
            rng_state: 0x6D2B_79F5,
            last_mix: 0.0,
        }
    }

    /// xorshift32 — fast deterministic RNG for the random-phase resynth.
    /// Matches the style used elsewhere in the engine (NoiseGen).
    fn rand_u32(&mut self) -> u32 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        x
    }

    fn rand_phase(&mut self) -> f32 {
        // Uniform 0..2π.
        (self.rand_u32() as f32 / u32::MAX as f32) * std::f32::consts::TAU
    }

    /// Run one FFT capture on the most recent FFT_SIZE input samples,
    /// ordered linearly (oldest first).  Stores magnitude per bin in
    /// `captured_mag`.
    fn capture(&mut self) {
        // Copy in_buf into work in time order, applying the Hann window.
        for i in 0..FREEZE_FFT_SIZE {
            let idx = (self.in_pos + i) % FREEZE_FFT_SIZE;
            self.work[i] = Complex::new(self.in_buf[idx] * self.hann[i], 0.0);
        }
        self.fft_fwd
            .process_with_scratch(&mut self.work, &mut self.fft_scratch);
        for (i, m) in self.captured_mag.iter_mut().enumerate().take(FREEZE_BINS) {
            *m = self.work[i].norm();
        }
        self.captured = true;
    }

    /// Synthesise one output frame and overlap-add into `out_buf` starting
    /// at `out_write_anchor`.  Advances `out_write_anchor` by HOP_SIZE.
    fn synthesise_frame(&mut self) {
        // Build a complex spectrum with captured magnitudes + random phases,
        // mirrored for negative frequencies (Hermitian symmetry → real
        // output).
        for i in 0..FREEZE_BINS {
            let mag = self.captured_mag[i];
            let phase = if i == 0 || i == FREEZE_FFT_SIZE / 2 {
                0.0 // DC + Nyquist must be real
            } else {
                self.rand_phase()
            };
            let (s, c) = phase.sin_cos();
            self.work[i] = Complex::new(mag * c, mag * s);
        }
        // Mirror conjugate for the upper half.
        for i in 1..FREEZE_FFT_SIZE / 2 {
            let conj = self.work[i].conj();
            self.work[FREEZE_FFT_SIZE - i] = conj;
        }
        self.fft_inv
            .process_with_scratch(&mut self.work, &mut self.fft_scratch);
        // Overlap-add with Hann window into out_buf.  Normalise by FFT_SIZE
        // (rustfft's inverse is unscaled) and by the Hann² overlap-add gain
        // factor (≈ 1.5 at 75 % overlap with Hann window) to keep level
        // roughly equal to dry input.
        let norm = 1.0 / (FREEZE_FFT_SIZE as f32 * 1.5);
        for i in 0..FREEZE_FFT_SIZE {
            let idx = (self.out_write_anchor + i) % self.out_buf.len();
            self.out_buf[idx] += self.work[i].re * self.hann[i] * norm;
        }
        self.out_write_anchor = (self.out_write_anchor + FREEZE_HOP_SIZE) % self.out_buf.len();
    }

    pub(crate) fn process(&mut self, input: f32, mix: f32, _sr: f32) -> f32 {
        // Always write input into the input ring; we need it ready when
        // freeze first engages.
        self.in_buf[self.in_pos] = input;
        self.in_pos = (self.in_pos + 1) % FREEZE_FFT_SIZE;

        // Detect rising / falling edges of mix > 0.
        if self.last_mix < 0.001 && mix > 0.001 {
            // Engaging — schedule a capture on the next hop boundary.
            self.captured = false;
        } else if mix < 0.001 {
            // Disengaging — clear the captured snapshot and silence the
            // pending output so the next engagement starts fresh.
            self.captured = false;
            for s in &mut self.out_buf {
                *s = 0.0;
            }
            self.out_read = self.out_write_anchor;
        }
        self.last_mix = mix;

        // Read the next overlap-add output sample.
        let wet = self.out_buf[self.out_read];
        // Clear the slot we just consumed so subsequent overlap-adds don't
        // accumulate stale energy.
        self.out_buf[self.out_read] = 0.0;
        self.out_read = (self.out_read + 1) % self.out_buf.len();

        // Hop boundary: capture (if newly engaged) and synthesise one frame.
        self.hop_counter += 1;
        if self.hop_counter >= FREEZE_HOP_SIZE {
            self.hop_counter = 0;
            if mix > 0.001 {
                if !self.captured {
                    self.capture();
                }
                self.synthesise_frame();
            }
        }

        if mix < 0.001 {
            input
        } else {
            input * (1.0 - mix) + wet * mix
        }
    }
}
