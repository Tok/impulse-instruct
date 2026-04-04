// ─── Low-level voice state machines ──────────────────────────────────────────
// Pure numeric DSP structs — no allocations.

/// Moog-style 4-pole ladder filter state.
#[derive(Clone, Copy, Default)]
pub(super) struct LadderFilter {
    pub(super) s: [f32; 4],
}

impl LadderFilter {
    /// `g` is the per-stage filter coefficient (0–~0.99).
    /// Callers must map cutoff to a proper g before calling.
    pub(super) fn process(&mut self, input: f32, g: f32, resonance: f32) -> f32 {
        let f = g.clamp(0.001, 0.99);
        let k = resonance * 4.0; // self-oscillation at k=4.0

        // Feedback
        let fb = k * self.s[3];
        let x = input - fb;

        // 4 one-pole stages
        self.s[0] = self.s[0] + f * (super::tanh(x) - super::tanh(self.s[0]));
        self.s[1] = self.s[1] + f * (super::tanh(self.s[0]) - super::tanh(self.s[1]));
        self.s[2] = self.s[2] + f * (super::tanh(self.s[1]) - super::tanh(self.s[2]));
        self.s[3] = self.s[3] + f * (super::tanh(self.s[2]) - super::tanh(self.s[3]));
        self.s[3]
    }
}

/// Simple one-pole smoothing filter.
#[derive(Clone, Copy, Default)]
pub(super) struct OnePole {
    pub(super) state: f32,
}

impl OnePole {
    pub(super) fn process(&mut self, input: f32, coeff: f32) -> f32 {
        self.state = self.state * coeff + input * (1.0 - coeff);
        self.state
    }
}

/// PRNG for noise generation (xorshift32 — no stdlib, no heap).
#[derive(Clone, Copy)]
pub(super) struct NoiseGen {
    pub(super) state: u32,
}

impl NoiseGen {
    pub(super) fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }
    pub(super) fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

// ─── Drum voice generic base ──────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
pub(super) struct Envelope {
    pub(super) value: f32,
    pub(super) active: bool,
}

impl Envelope {
    pub(super) fn trigger(&mut self) {
        self.value = 1.0;
        self.active = true;
    }
    pub(super) fn tick(&mut self, decay_coeff: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        self.value *= decay_coeff;
        if self.value < 1e-6 {
            self.active = false;
            self.value = 0.0;
        }
        self.value
    }
}

// ─── 808/909 Kick ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Kick {
    pub(super) phase: f32,
    pub(super) pitch_env: Envelope,
    pub(super) amp_env: Envelope,
    pub(super) noise_gen: NoiseGen,
    pub(super) punch_env: Envelope,
}

impl Kick {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            phase: 0.0,
            pitch_env: Envelope::default(),
            amp_env: Envelope::default(),
            noise_gen: NoiseGen::new(seed),
            punch_env: Envelope::default(),
        }
    }

    pub(super) fn trigger(&mut self) {
        self.pitch_env.trigger();
        self.amp_env.trigger();
        self.punch_env.trigger();
        self.phase = 0.0;
    }

    pub(super) fn process(
        &mut self,
        base_pitch: f32,
        decay: f32,
        punch: f32,
        volume: f32,
        pitch_env_depth: f32,
        pitch_env_time: f32,
        sr: f32,
    ) -> f32 {
        let base_hz = 40.0 + base_pitch * 40.0; // 40–80 Hz
        let decay_coeff = (-1.0 / (sr * (decay * 1.8 + 0.2))).exp();
        let pitch_decay_time = 0.01 + pitch_env_time * 0.19; // 10ms–200ms
        let pitch_decay = (-1.0 / (sr * pitch_decay_time)).exp();
        let punch_decay = (-1.0 / (sr * 0.005)).exp(); // 5ms punch

        let amp = self.amp_env.tick(decay_coeff);
        let pitch_mod = self.pitch_env.tick(pitch_decay);
        let punch_amp = self.punch_env.tick(punch_decay);

        let freq = base_hz + pitch_mod * base_hz * (1.0 + pitch_env_depth * 9.0);
        self.phase += freq / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let sine = (self.phase * std::f32::consts::TAU).sin();
        let click = self.noise_gen.next() * punch_amp * punch;

        (sine * amp + click * 0.3) * volume
    }
}

// ─── 808/909 Snare ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Snare {
    pub(super) phase: f32,
    pub(super) tone_env: Envelope,
    pub(super) noise_env: Envelope,
    pub(super) noise_filter: OnePole,
    pub(super) noise_gen: NoiseGen,
}

impl Snare {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            phase: 0.0,
            tone_env: Envelope::default(),
            noise_env: Envelope::default(),
            noise_filter: OnePole::default(),
            noise_gen: NoiseGen::new(seed),
        }
    }

    pub(super) fn trigger(&mut self) {
        self.tone_env.trigger();
        self.noise_env.trigger();
    }

    pub(super) fn process(
        &mut self,
        tone: f32,
        snappy: f32,
        decay: f32,
        volume: f32,
        sr: f32,
    ) -> f32 {
        let tone_hz = 100.0 + tone * 200.0;
        let decay_coeff = (-1.0 / (sr * (decay * 0.4 + 0.05))).exp();
        let noise_decay = (-1.0 / (sr * (decay * 0.3 + 0.03))).exp();

        let tone_amp = self.tone_env.tick(decay_coeff);
        let noise_amp = self.noise_env.tick(noise_decay);

        self.phase += tone_hz / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let tone_out = (self.phase * std::f32::consts::TAU).sin() * tone_amp;
        let noise_raw = self.noise_gen.next();
        // High-pass noise: subtract low-pass
        let lp = self.noise_filter.process(noise_raw, 0.7);
        let noise_hp = noise_raw - lp;
        let noise_out = noise_hp * noise_amp * snappy;

        (tone_out * (1.0 - snappy * 0.5) + noise_out) * volume
    }
}

// ─── HiHat ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct HiHat {
    pub(super) amp_env: Envelope,
    pub(super) noise_filter: OnePole,
    pub(super) noise_gen: NoiseGen,
}

impl HiHat {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            amp_env: Envelope::default(),
            noise_filter: OnePole::default(),
            noise_gen: NoiseGen::new(seed),
        }
    }

    pub(super) fn trigger(&mut self) {
        self.amp_env.trigger();
    }

    pub(super) fn process(&mut self, decay: f32, tone: f32, volume: f32, sr: f32) -> f32 {
        let decay_time = decay * decay * 0.5 + 0.005; // 5ms–500ms
        let decay_coeff = (-1.0 / (sr * decay_time)).exp();
        let amp = self.amp_env.tick(decay_coeff);

        let noise = self.noise_gen.next();
        // Band-pass via two one-poles
        let lp = self.noise_filter.process(noise, 0.95 - tone * 0.15);
        let hp = noise - lp;

        hp * amp * volume
    }
}

// ─── 909 Clap ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct Clap {
    pub(super) amp_env: Envelope,
    pub(super) burst_env: Envelope,
    pub(super) noise_gen: NoiseGen,
    pub(super) burst_count: u8,
    pub(super) burst_timer: u32,
}

impl Clap {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            amp_env: Envelope::default(),
            burst_env: Envelope::default(),
            noise_gen: NoiseGen::new(seed),
            burst_count: 0,
            burst_timer: 0,
        }
    }

    pub(super) fn trigger(&mut self) {
        self.amp_env.trigger();
        self.burst_env.trigger();
        self.burst_count = 3;
        self.burst_timer = 0;
    }

    pub(super) fn process(&mut self, decay: f32, volume: f32, sr: f32) -> f32 {
        let decay_coeff = (-1.0 / (sr * (decay * 0.5 + 0.05))).exp();
        let burst_coeff = (-1.0 / (sr * 0.005)).exp();

        // Multi-burst characteristic of 909 clap
        if self.burst_count > 0 {
            self.burst_timer += 1;
            if self.burst_timer > (sr * 0.008) as u32 {
                self.burst_count -= 1;
                self.burst_timer = 0;
                self.burst_env.trigger();
            }
        }

        let amp = self.amp_env.tick(decay_coeff);
        let burst = self.burst_env.tick(burst_coeff);

        let noise = self.noise_gen.next();
        noise * (amp + burst * 0.5) * volume
    }
}
