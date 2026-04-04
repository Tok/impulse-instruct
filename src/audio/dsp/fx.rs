// ─── FX DSP structs ───────────────────────────────────────────────────────────
// Pure numeric DSP — no allocations inside process().

pub(super) const MAX_DELAY_SAMPLES: usize = 96_000; // 2s @ 48kHz
pub(super) const REVERB_COMBS: usize = 8;
pub(super) const REVERB_ALLPASS: usize = 4;
pub(super) const MAX_CHORUS_SIZE: usize = 4096; // ~85ms @ 48kHz

// ─── Simple Schroeder reverb ──────────────────────────────────────────────────

pub(super) struct Reverb {
    pub(super) comb_delays: [Vec<f32>; REVERB_COMBS],
    pub(super) comb_ptrs: [usize; REVERB_COMBS],
    pub(super) comb_filters: [f32; REVERB_COMBS],
    pub(super) allpass_delays: [Vec<f32>; REVERB_ALLPASS],
    pub(super) allpass_ptrs: [usize; REVERB_ALLPASS],
}

// Freeverb-inspired delay lengths (prime-ish, tuned for ~44.1kHz)
const COMB_LENGTHS: [usize; REVERB_COMBS] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_LENGTHS: [usize; REVERB_ALLPASS] = [556, 441, 341, 225];

impl Reverb {
    pub(super) fn new() -> Self {
        Self {
            comb_delays: std::array::from_fn(|i| vec![0.0f32; COMB_LENGTHS[i]]),
            comb_ptrs: [0; REVERB_COMBS],
            comb_filters: [0.0; REVERB_COMBS],
            allpass_delays: std::array::from_fn(|i| vec![0.0f32; ALLPASS_LENGTHS[i]]),
            allpass_ptrs: [0; REVERB_ALLPASS],
        }
    }

    pub(super) fn process(&mut self, input: f32, room_size: f32, damp: f32) -> f32 {
        let feedback = room_size * 0.28 + 0.7; // 0.7–0.98
        let damp1 = damp * 0.4;
        let damp2 = 1.0 - damp1;

        // Parallel comb filters
        let mut out = 0.0f32;
        for (i, &len) in COMB_LENGTHS.iter().enumerate() {
            let ptr = self.comb_ptrs[i];
            let delayed = self.comb_delays[i][ptr];

            // Low-pass filtered feedback
            self.comb_filters[i] = delayed * damp2 + self.comb_filters[i] * damp1;
            self.comb_delays[i][ptr] = input + self.comb_filters[i] * feedback;

            self.comb_ptrs[i] = (ptr + 1) % len;
            out += delayed;
        }
        out *= 0.125; // scale by num combs

        // Series all-pass filters
        for (i, &len) in ALLPASS_LENGTHS.iter().enumerate() {
            let ptr = self.allpass_ptrs[i];
            let delayed = self.allpass_delays[i][ptr];

            self.allpass_delays[i][ptr] = out + delayed * 0.5;
            self.allpass_ptrs[i] = (ptr + 1) % len;
            out = delayed - out;
        }

        out
    }
}

// ─── Delay line ───────────────────────────────────────────────────────────────

pub(super) struct DelayLine {
    pub(super) buf: Vec<f32>,
    pub(super) ptr: usize,
}

impl DelayLine {
    pub(super) fn new() -> Self {
        Self {
            buf: vec![0.0; MAX_DELAY_SAMPLES],
            ptr: 0,
        }
    }

    pub(super) fn process(&mut self, input: f32, delay_samples: usize, feedback: f32) -> f32 {
        let delay_samples = delay_samples.clamp(1, MAX_DELAY_SAMPLES - 1);
        let read_ptr = (self.ptr + MAX_DELAY_SAMPLES - delay_samples) % MAX_DELAY_SAMPLES;
        let delayed = self.buf[read_ptr];
        self.buf[self.ptr] = input + delayed * feedback;
        self.ptr = (self.ptr + 1) % MAX_DELAY_SAMPLES;
        delayed
    }
}

// ─── Chorus / ensemble effect ─────────────────────────────────────────────────

pub(super) struct Chorus {
    pub(super) buf: [f32; MAX_CHORUS_SIZE],
    pub(super) write: usize,
    pub(super) lfo_phase: f32,
}

impl Chorus {
    pub(super) fn new() -> Self {
        Self {
            buf: [0.0; MAX_CHORUS_SIZE],
            write: 0,
            lfo_phase: 0.0,
        }
    }

    /// Two-voice BBD-style chorus: base ±10ms modulated by two phase-offset LFOs.
    /// `rate`: 0–1 → 0.1–8 Hz, `depth`: 0–1 → 0–10ms, `mix`: 0–1 wet/dry.
    pub(super) fn process(&mut self, input: f32, rate: f32, depth: f32, mix: f32, sr: f32) -> f32 {
        if mix < 0.001 {
            return input;
        }
        self.buf[self.write] = input;

        let rate_hz = 0.1 + rate * 7.9;
        self.lfo_phase += rate_hz / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        let tau = std::f32::consts::TAU;
        let lfo1 = (self.lfo_phase * tau).sin();
        let lfo2 = ((self.lfo_phase + 0.333) * tau).sin(); // 120° offset

        let base_s = (0.015 * sr) as usize; // 15ms base delay
        let depth_s = (depth * 0.010 * sr) as usize; // 0–10ms depth

        let off1 = base_s
            .saturating_add_signed((depth_s as f32 * lfo1) as isize)
            .clamp(1, MAX_CHORUS_SIZE - 1);
        let off2 = base_s
            .saturating_add_signed((depth_s as f32 * lfo2) as isize)
            .clamp(1, MAX_CHORUS_SIZE - 1);

        let r1 = (self.write + MAX_CHORUS_SIZE - off1) % MAX_CHORUS_SIZE;
        let r2 = (self.write + MAX_CHORUS_SIZE - off2) % MAX_CHORUS_SIZE;

        let wet = (self.buf[r1] + self.buf[r2]) * 0.5;
        self.write = (self.write + 1) % MAX_CHORUS_SIZE;

        input * (1.0 - mix) + wet * mix
    }
}

// ─── 3-band parametric EQ (biquad shelves + peak) ─────────────────────────────

/// One biquad filter in direct form II transposed.
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl Biquad {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }

    /// Low shelf at `fc` Hz with `gain_db` boost/cut.
    fn low_shelf(fc: f32, gain_db: f32, sr: f32) -> Self {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * fc / sr;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / 2.0 * (a + 1.0 / a).sqrt();
        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// High shelf at `fc` Hz with `gain_db` boost/cut.
    fn high_shelf(fc: f32, gain_db: f32, sr: f32) -> Self {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * fc / sr;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / 2.0 * (a + 1.0 / a).sqrt();
        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * a.sqrt() * alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * a.sqrt() * alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Peaking EQ at `fc` Hz, bandwidth Q, with `gain_db` boost/cut.
    fn peak(fc: f32, q: f32, gain_db: f32, sr: f32) -> Self {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * fc / sr;
        let alpha = w0.sin() / (2.0 * q);
        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * w0.cos();
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * w0.cos();
        let a2 = 1.0 - alpha / a;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            s1: 0.0,
            s2: 0.0,
        }
    }
}

/// 3-band EQ: low shelf 200 Hz · mid peak 1 kHz · high shelf 5 kHz.
/// Coefficients are recomputed only when gain values change (per-block comparison).
pub(super) struct EqBands {
    low: Biquad,
    mid: Biquad,
    hi: Biquad,
    sr: f32,
    last_low: f32,
    last_mid: f32,
    last_hi: f32,
}

impl EqBands {
    pub(super) fn new(sr: f32) -> Self {
        Self {
            low: Biquad::low_shelf(200.0, 0.0, sr),
            mid: Biquad::peak(1000.0, 1.0, 0.0, sr),
            hi: Biquad::high_shelf(5000.0, 0.0, sr),
            sr,
            last_low: 0.0,
            last_mid: 0.0,
            last_hi: 0.0,
        }
    }

    /// `low/mid/hi_gain`: -1..+1 → -12..+12 dB.
    pub(super) fn process(
        &mut self,
        input: f32,
        low_gain: f32,
        mid_gain: f32,
        hi_gain: f32,
    ) -> f32 {
        // Recompute coefficients only when a band changes (avoids per-sample trig)
        let low_db = low_gain * 12.0;
        let mid_db = mid_gain * 12.0;
        let hi_db = hi_gain * 12.0;
        if (low_gain - self.last_low).abs() > 0.001 {
            self.low = Biquad::low_shelf(200.0, low_db, self.sr);
            self.last_low = low_gain;
        }
        if (mid_gain - self.last_mid).abs() > 0.001 {
            self.mid = Biquad::peak(1000.0, 1.0, mid_db, self.sr);
            self.last_mid = mid_gain;
        }
        if (hi_gain - self.last_hi).abs() > 0.001 {
            self.hi = Biquad::high_shelf(5000.0, hi_db, self.sr);
            self.last_hi = hi_gain;
        }
        self.hi.process(self.mid.process(self.low.process(input)))
    }
}

// ─── Phaser (4-stage all-pass cascade) ────────────────────────────────────────

pub(super) struct Phaser {
    /// All-pass filter states (one per stage, Chamberlin transposed-direct-form-II).
    stages: [f32; 4],
    lfo_phase: f32,
}

impl Phaser {
    pub(super) fn new() -> Self {
        Self {
            stages: [0.0; 4],
            lfo_phase: 0.0,
        }
    }

    /// `rate`: 0–1 → 0.05–5 Hz LFO rate.
    /// `depth`: 0–1 sweep width (0 = narrow, 1 = full 300–4000 Hz sweep).
    /// `mix`: 0–1 wet/dry.
    pub(super) fn process(&mut self, input: f32, rate: f32, depth: f32, mix: f32, sr: f32) -> f32 {
        if mix < 0.001 {
            return input;
        }

        let rate_hz = 0.05 + rate * 4.95;
        self.lfo_phase += rate_hz / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }

        let lfo = (self.lfo_phase * std::f32::consts::TAU).sin();

        // LFO sweeps center frequency: 300–4000 Hz
        let fc = (1150.0 + lfo * depth * 1850.0).clamp(50.0, sr * 0.45);

        // First-order all-pass coefficient: d = (tan(π·fc/sr) − 1) / (tan(π·fc/sr) + 1)
        let t = (std::f32::consts::PI * fc / sr).tan().clamp(0.001, 999.0);
        let d = (t - 1.0) / (t + 1.0);

        // 4 all-pass stages in series (transposed form: s stores x[n-1] − d·y[n-1])
        let mut x = input;
        for s in &mut self.stages {
            let y = d * x + *s;
            *s = x - d * y;
            x = y;
        }

        // Classic phaser: summing dry + wet creates comb notches
        input * (1.0 - mix) + x * mix
    }
}
