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

    pub(super) fn process(&mut self, input: f32, room_size: f32, damp: f32, freeze: bool) -> f32 {
        // Freeze: feedback = 1.0, no new input → reverb tail holds indefinitely
        let (feedback, input) = if freeze {
            (1.0, 0.0)
        } else {
            (room_size * 0.28 + 0.7, input) // 0.7–0.98
        };
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
    wow_phase: f32,     // slow sine LFO for wow (0–1)
    flutter_phase: f32, // faster LFO for flutter (0–1)
}

impl DelayLine {
    pub(super) fn new() -> Self {
        Self {
            buf: vec![0.0; MAX_DELAY_SAMPLES],
            ptr: 0,
            wow_phase: 0.0,
            flutter_phase: 0.0,
        }
    }

    /// Tape-style delay with wow/flutter modulation and feedback saturation.
    /// `wow_flutter`: 0–1 depth of pitch wobble. `sat`: 0–1 tape saturation on feedback.
    pub(super) fn process_tape(
        &mut self,
        input: f32,
        delay_samples: usize,
        feedback: f32,
        wow_flutter: f32,
        sat: f32,
        sr: f32,
    ) -> f32 {
        let max = MAX_DELAY_SAMPLES - 2; // leave room for interpolation
        let base_delay = (delay_samples as f32).clamp(1.0, max as f32);

        // Wow: slow ~0.5 Hz sine, Flutter: faster ~6 Hz sine
        let wow_rate = 0.5 / sr;
        let flutter_rate = 6.0 / sr;
        self.wow_phase = (self.wow_phase + wow_rate) % 1.0;
        self.flutter_phase = (self.flutter_phase + flutter_rate) % 1.0;

        let mod_depth = wow_flutter * base_delay * 0.003; // up to 0.3% of delay time
        let wow = (self.wow_phase * std::f32::consts::TAU).sin() * mod_depth;
        let flutter = (self.flutter_phase * std::f32::consts::TAU).sin() * mod_depth * 0.3;
        let modulated_delay = (base_delay + wow + flutter).clamp(1.0, max as f32);

        // Fractional read with linear interpolation
        let read_pos = (self.ptr as f32 + MAX_DELAY_SAMPLES as f32 - modulated_delay)
            % MAX_DELAY_SAMPLES as f32;
        let idx = read_pos as usize % MAX_DELAY_SAMPLES;
        let frac = read_pos - read_pos.floor();
        let next = (idx + 1) % MAX_DELAY_SAMPLES;
        let delayed = self.buf[idx] + (self.buf[next] - self.buf[idx]) * frac;

        // Tape saturation on feedback — soft clip
        let fb_signal = if sat > 0.001 {
            let drive = 1.0 + sat * 3.0;
            super::tanh(delayed * drive * feedback) / drive
        } else {
            delayed * feedback
        };

        self.buf[self.ptr] = input + fb_signal;
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

    /// Read a sample at a fractional delay position (0–1 → 0–MAX_CHORUS_SIZE).
    /// Used for stereo decorrelation.
    pub(super) fn read_tap(&self, pos: f32) -> f32 {
        let off = ((pos * MAX_CHORUS_SIZE as f32) as usize).clamp(1, MAX_CHORUS_SIZE - 1);
        let r = (self.write + MAX_CHORUS_SIZE - off) % MAX_CHORUS_SIZE;
        self.buf[r]
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

// ─── Compressor / limiter ─────────────────────────────────────────────────────

pub(super) struct Compressor {
    env: f32, // peak envelope follower state
    // Multiband: per-band envelope followers + crossover filter state
    band_env: [f32; 3],
    lp_state: [f32; 2], // 2-pole LP for low band
    hp_state: [f32; 2], // 2-pole HP for high band
}

impl Compressor {
    pub(super) fn new() -> Self {
        Self {
            env: 0.0,
            band_env: [0.0; 3],
            lp_state: [0.0; 2],
            hp_state: [0.0; 2],
        }
    }

    /// Single-band compressor (static — no &self to avoid borrow conflicts).
    fn compress_band(input: f32, env: &mut f32, threshold: f32, ratio: f32, sr: f32) -> f32 {
        let thresh_db = -40.0 * (1.0 - threshold);
        let ratio_val = 1.0 + ratio * 19.0;
        let level = input.abs();
        let att = (-1.0 / (sr * 0.001)).exp();
        let rel = (-1.0 / (sr * 0.08)).exp();
        *env = if level > *env {
            *env * att + level * (1.0 - att)
        } else {
            *env * rel + level * (1.0 - rel)
        };
        let env_db = 20.0 * env.max(1e-9).log10();
        let gain_db = if env_db > thresh_db {
            (env_db - thresh_db) * (1.0 - 1.0 / ratio_val)
        } else {
            0.0
        };
        input * 10.0f32.powf(-gain_db / 20.0)
    }

    /// `threshold`: 0–1 → −40..0 dB. `ratio`: 0–1 → 1:1..20:1. `mix`: 0–1 wet/dry.
    /// `multiband`: 0 = single band, >0 = 3-band split (low/mid/high).
    pub(super) fn process(
        &mut self,
        input: f32,
        threshold: f32,
        ratio: f32,
        mix: f32,
        multiband: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }

        let compressed = if multiband > 0.001 {
            // 3-band crossover: LP at 200 Hz, HP at 3000 Hz, mid = input - low - high
            let lp_fc = 200.0 / sr;
            let hp_fc = 3000.0 / sr;
            let lp_a = (std::f32::consts::TAU * lp_fc).min(0.99);
            let hp_a = (std::f32::consts::TAU * hp_fc).min(0.99);

            // 2-pole LP
            self.lp_state[0] += lp_a * (input - self.lp_state[0]);
            self.lp_state[1] += lp_a * (self.lp_state[0] - self.lp_state[1]);
            let low = self.lp_state[1];

            // 2-pole HP
            self.hp_state[0] += hp_a * (input - self.hp_state[0]);
            self.hp_state[1] += hp_a * (self.hp_state[0] - self.hp_state[1]);
            let high = input - self.hp_state[1];

            let mid = input - low - high;

            // Compress each band independently
            let c_low = Self::compress_band(low, &mut self.band_env[0], threshold, ratio, sr);
            let c_mid = Self::compress_band(mid, &mut self.band_env[1], threshold, ratio, sr);
            let c_high = Self::compress_band(high, &mut self.band_env[2], threshold, ratio, sr);

            c_low + c_mid + c_high
        } else {
            Self::compress_band(input, &mut self.env, threshold, ratio, sr)
        };

        input * (1.0 - mix) + compressed * mix
    }
}

// ─── Tape saturation ──────────────────────────────────────────────────────────

pub(super) struct TapeSat {
    flutter_phase: f32,
}

impl TapeSat {
    pub(super) fn new() -> Self {
        Self { flutter_phase: 0.0 }
    }

    /// `drive`: 0–1 saturation amount. `mix`: 0–1 wet/dry. `flutter`: 0–1 wow depth (~2.5 Hz).
    pub(super) fn process(
        &mut self,
        input: f32,
        drive: f32,
        mix: f32,
        flutter: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }
        // Wow/flutter: ±4% amplitude modulation at ~2.5 Hz
        self.flutter_phase += 2.5 / sr;
        if self.flutter_phase >= 1.0 {
            self.flutter_phase -= 1.0;
        }
        let wow = 1.0 + flutter * 0.04 * (self.flutter_phase * std::f32::consts::TAU).sin();

        // Arctan saturation: softer knee than tanh, distinct harmonic content
        let d = drive * 5.0 + 1.0;
        let x = input * d * wow;
        let sat = x.atan() * std::f32::consts::FRAC_2_PI; // normalised to −1..+1
        let sat =
            sat / d.atan() * std::f32::consts::FRAC_2_PI.recip() * (std::f32::consts::PI / 2.0);

        input * (1.0 - mix) + sat * mix
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

// ─── Autotune — grain-based pitch shifter ─────────────────────────────────────
//
// Two-grain overlap-add pitch shifter.  Writes into a pre-allocated ring buffer;
// two read heads advance at `pitch_ratio` samples per input sample.  A triangular
// crossfade envelope blends the grains to suppress the periodic amplitude flutter.
//
// `amount`: 0–1 → 0..+12 semitones (upward).  At 0 the effect is bypassed.
// `mix`:    0–1 wet/dry blend.

const AUTOTUNE_BUF: usize = 4096; // ~93 ms @ 44.1 kHz — no heap alloc
const AUTOTUNE_GRAIN: f32 = 1024.0; // ~23 ms crossfade period

pub(super) struct Autotune {
    buf: [f32; AUTOTUNE_BUF],
    write: usize,
    r1: f32,  // grain-1 read head (fractional sample index into ring buffer)
    r2: f32,  // grain-2 read head (offset by half grain)
    env: f32, // crossfade envelope phase 0..1, wraps every AUTOTUNE_GRAIN samples
}

impl Autotune {
    pub(super) fn new() -> Self {
        Self {
            buf: [0.0; AUTOTUNE_BUF],
            write: 0,
            r1: 0.0,
            r2: AUTOTUNE_GRAIN * 0.5,
            env: 0.0,
        }
    }

    /// Process one sample.  `amount`: 0–1.  `mix`: 0–1 wet/dry.
    pub(super) fn process(&mut self, input: f32, amount: f32, mix: f32) -> f32 {
        if mix < 0.001 || amount < 0.001 {
            return input;
        }

        // Write input into ring buffer (no allocation — buffer is a fixed array).
        self.buf[self.write % AUTOTUNE_BUF] = input;
        self.write = self.write.wrapping_add(1);

        // Pitch ratio: 2^(amount * 12 / 12) = 2^amount → 1.0 .. 2.0
        let ratio = 2.0_f32.powf(amount);

        // Advance both read heads at `ratio` samples per input sample.
        self.r1 += ratio;
        self.r2 += ratio;
        self.env += 1.0 / AUTOTUNE_GRAIN;

        // Wrap grain-1 read head: keep it within one buffer length of the write head.
        let write_f = self.write as f32;
        if write_f - self.r1 > AUTOTUNE_BUF as f32 {
            self.r1 = write_f - AUTOTUNE_GRAIN;
        }
        if write_f - self.r2 > AUTOTUNE_BUF as f32 {
            self.r2 = write_f - AUTOTUNE_GRAIN;
        }
        if self.env >= 1.0 {
            self.env -= 1.0;
        }

        // Linearly interpolated read from ring buffer.
        let read_sample = |pos: f32| -> f32 {
            let i0 = pos as usize % AUTOTUNE_BUF;
            let i1 = (i0 + 1) % AUTOTUNE_BUF;
            let frac = pos - pos.floor();
            self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac
        };
        let s1 = read_sample(self.r1);
        let s2 = read_sample(self.r2);

        // Triangular crossfade: w(e) = 1 − |2e − 1|
        let w1 = 1.0 - (2.0 * self.env - 1.0).abs();
        let w2 = 1.0 - (2.0 * (self.env + 0.5).rem_euclid(1.0) - 1.0).abs();
        let wet = s1 * w1 + s2 * w2;

        input * (1.0 - mix) + wet * mix
    }
}
