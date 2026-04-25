// ─── audio/dsp/fx_freq_shift.rs ──────────────────────────────────────────────
// Single-sideband frequency shifter built on a Hilbert transform.
//
// Unlike a pitch shifter (which preserves harmonic ratios), a frequency
// shifter adds the same Hz to every spectral component — so harmonics
// stop being integer multiples of the fundamental and the timbre
// becomes inharmonic / metallic.  Classic radio-jamming, bell-tine,
// and Sean-Costello-shimmer effects all live here.
//
// Algorithm (real-time IIR, no buffering latency):
//   1. Two parallel cascades of 2nd-order allpass sections produce a
//      90°-apart pair from a single real input — together they form
//      the real + imaginary parts of the analytic signal.
//   2. Multiply the analytic pair by a complex exponential
//      e^{j 2π f_shift t}: the real-output projection
//      (x_re·cosφ − x_im·sinφ for upshift, or +sinφ for downshift)
//      gives the SSB-shifted output.
//
// Phase-network coefficients are the Hartmann ¼-section pair: 4
// sections per branch, ~1° phase error in 100 Hz – 20 kHz at 44.1 kHz.
// The 90° offset between branches is what makes the analytic signal
// usable; the small phase ripple shows up as tiny amplitude wobble
// across the spectrum (acceptable for a creative FX, not for an SSB
// modem).
//
// Stable as long as every section's coefficient is in [0, 1) — both
// banks are; their poles are well inside the unit circle.

/// Real branch — squared 'a' coefficients for H(z) = (a + z⁻²) / (1 + a·z⁻²).
const HILBERT_A: [f32; 4] = [0.4794, 0.8780, 0.9764, 0.9955];
/// Imaginary branch — same allpass form, different `a` so the cumulative
/// phase across the cascade lands ~90° offset from `HILBERT_A`.
const HILBERT_B: [f32; 4] = [0.1617, 0.7307, 0.9484, 0.9941];

/// One 2nd-order allpass section.  Difference equation
/// `y[n] = a·x[n] + x[n-2] − a·y[n-2]`.  No `z⁻¹` term, so we only
/// keep two state samples for x and y.
#[derive(Clone, Copy, Default)]
struct AllpassSection {
    a: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl AllpassSection {
    fn new(a: f32) -> Self {
        Self {
            a,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn step(&mut self, x: f32) -> f32 {
        let y = self.a * x + self.x2 - self.a * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub(crate) struct FreqShift {
    branch_re: [AllpassSection; 4],
    branch_im: [AllpassSection; 4],
    /// Carrier phase in cycles (0..1) — wraps each sample.
    phase: f32,
    /// One-sample feedback delay so the user-controlled regen knob
    /// doesn't create instantaneous self-reference (would
    /// cascade-blow-up in seconds).
    fb_prev: f32,
}

impl FreqShift {
    pub(crate) fn new() -> Self {
        Self {
            branch_re: [
                AllpassSection::new(HILBERT_A[0]),
                AllpassSection::new(HILBERT_A[1]),
                AllpassSection::new(HILBERT_A[2]),
                AllpassSection::new(HILBERT_A[3]),
            ],
            branch_im: [
                AllpassSection::new(HILBERT_B[0]),
                AllpassSection::new(HILBERT_B[1]),
                AllpassSection::new(HILBERT_B[2]),
                AllpassSection::new(HILBERT_B[3]),
            ],
            phase: 0.0,
            fb_prev: 0.0,
        }
    }

    /// Run one sample through the Hilbert pair.  Returns
    /// `(real, imag)` — the analytic-signal pair.  Real is the input
    /// passed through `branch_re` (a smoothed, all-pass-shaped
    /// version of the input); imag is the input through `branch_im`,
    /// 90° phase-shifted.
    fn analytic(&mut self, x: f32) -> (f32, f32) {
        let mut re = x;
        for s in &mut self.branch_re {
            re = s.step(re);
        }
        let mut im = x;
        for s in &mut self.branch_im {
            im = s.step(im);
        }
        (re, im)
    }

    /// `shift_norm`: 0..1 normalised, mapped to ±1000 Hz around centre
    /// (0.5 = 0 Hz / no shift).  `feedback`: 0..1 → 0..0.85 of the
    /// previous output sample mixed into the input (creates Sean
    /// Costello "shimmer-style" reflective ladders when paired with
    /// reverb).  Capped at 0.85 + tanh-clamped on the feedback tap so
    /// the cumulative analytic-signal headroom can't run away under
    /// long sustained input.  `mix`: 0..1 wet/dry.
    pub(crate) fn process(
        &mut self,
        input: f32,
        shift_norm: f32,
        feedback: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            // Bypass — but still advance state so the shifted output
            // doesn't pop when the user opens the mix knob mid-bar.
            let (_re, _im) = self.analytic(input);
            return input;
        }
        let shift_hz = (shift_norm.clamp(0.0, 1.0) - 0.5) * 2000.0;
        // 0.85 max — Hilbert IIR + complex multiply produce small
        // amplitude excursions on transients; 0.95 leaves no margin.
        let fb = feedback.clamp(0.0, 1.0) * 0.85;
        // Soft-clip the feedback tap so the regen loop can't build
        // past unit amplitude even if the FX hits a resonant peak in
        // the cascade response.
        let fb_safe = self.fb_prev.tanh();
        let driven = input + fb_safe * fb;
        let (re, im) = self.analytic(driven);
        self.phase += shift_hz / sr;
        // Wrap to [-1, 1) — keeps cos/sin precision sane even after
        // long runs.  Plain modulo on f32 introduces drift; rem_euclid
        // is the right call.
        self.phase = self.phase.rem_euclid(1.0);
        let (sin_p, cos_p) = (self.phase * std::f32::consts::TAU).sin_cos();
        // Sign of shift_hz controls direction: positive = upshift
        // (subtract imaginary projection), negative = downshift (add).
        let wet = if shift_hz >= 0.0 {
            re * cos_p - im * sin_p
        } else {
            re * cos_p + im * sin_p
        };
        self.fb_prev = wet;
        input * (1.0 - mix) + wet * mix
    }
}
