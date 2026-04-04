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
