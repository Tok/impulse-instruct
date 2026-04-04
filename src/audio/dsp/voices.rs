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

// ─── Standalone Noise Voice ───────────────────────────────────────────────────

#[derive(Clone)]
pub(super) struct NoiseVoice {
    rng: NoiseGen,
    // Pink noise filter state (Paul Kellett's 3-stage pink filter)
    b0: f32,
    b1: f32,
    b2: f32,
    b3: f32,
    b4: f32,
    b5: f32,
    // Brown noise state
    brown: f32,
    // 1-pole LP filter state
    lp_state: f32,
}

impl NoiseVoice {
    pub(super) fn new(seed: u32) -> Self {
        Self {
            rng: NoiseGen::new(seed),
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            b3: 0.0,
            b4: 0.0,
            b5: 0.0,
            brown: 0.0,
            lp_state: 0.0,
        }
    }

    /// `color`: 0=white, 0.5=pink, 1=brown. `cutoff`: 0–1 LP filter (200–20000 Hz).
    pub(super) fn process(&mut self, volume: f32, color: f32, cutoff: f32, sr: f32) -> f32 {
        if volume < 0.001 {
            return 0.0;
        }

        let white = self.rng.next();

        // Pink noise (Paul Kellett)
        self.b0 = 0.99886 * self.b0 + white * 0.0555179;
        self.b1 = 0.99332 * self.b1 + white * 0.0750759;
        self.b2 = 0.96900 * self.b2 + white * 0.153_852;
        self.b3 = 0.86650 * self.b3 + white * 0.3104856;
        self.b4 = 0.55000 * self.b4 + white * 0.5329522;
        self.b5 = -0.7616 * self.b5 + white * 0.0168980;
        let pink =
            (self.b0 + self.b1 + self.b2 + self.b3 + self.b4 + self.b5 + white * 0.5362) * 0.11;

        // Brown noise (integrated white)
        self.brown = (self.brown + 0.02 * white) / 1.02;
        let brown = self.brown * 3.5;

        // Color crossfade: 0=white, 0.5=pink, 1=brown
        let out = if color < 0.5 {
            let t = color * 2.0;
            white * (1.0 - t) + pink * t
        } else {
            let t = (color - 0.5) * 2.0;
            pink * (1.0 - t) + brown * t
        };

        // 1-pole LP filter: cutoff 0–1 → 200–20000 Hz
        let fc = (200.0 * (100.0f32).powf(cutoff)).min(sr * 0.45);
        let a = 1.0 - (-std::f32::consts::TAU * fc / sr).exp();
        self.lp_state += a * (out - self.lp_state);

        self.lp_state * volume
    }
}

// ─── Hoover Lead Voice ────────────────────────────────────────────────────────
// Supersaw oscillator into a highpass filter that sweeps down from a high
// starting cutoff. Heavy resonance creates the "vacuum cleaner" sweep.
// Named after Human Resource "Dominator" (1991).

#[derive(Clone)]
pub(super) struct HooverVoice {
    phase: f32,
    unison_phases: [f32; 6],
    freq: f32,
    gate: bool,
    amp_env: f32,  // VCA envelope
    filt_env: f32, // Filter sweep: 1.0 = high HP cutoff, decays to 0.0
    svf_low: f32,
    svf_band: f32,
    lfo_phase: f32,
}

impl HooverVoice {
    pub(super) fn new() -> Self {
        Self {
            phase: 0.0,
            unison_phases: [0.0; 6],
            freq: 220.0,
            gate: false,
            amp_env: 0.0,
            filt_env: 0.0,
            svf_low: 0.0,
            svf_band: 0.0,
            lfo_phase: 0.0,
        }
    }

    pub(super) fn trigger(&mut self, note: u8) {
        self.freq = super::midi_to_hz(note);
        self.gate = true;
        self.amp_env = 0.0; // will rise on fast attack
        self.filt_env = 1.0; // start at max HP cutoff
        self.svf_low = 0.0;
        self.svf_band = 0.0;
    }

    pub(super) fn gate_off(&mut self) {
        self.gate = false;
    }

    pub(super) fn process(&mut self, sr: f32, p: &super::AudioParams) -> f32 {
        if self.amp_env < 1e-5 && !self.gate {
            return 0.0;
        }

        // Amplitude envelope: fast attack (~5 ms), release (~80 ms)
        if self.gate {
            let attack_coeff = (-1.0_f32 / (0.005 * sr)).exp();
            self.amp_env = 1.0 - (1.0 - self.amp_env) * attack_coeff;
        } else {
            let release_coeff = (-1.0_f32 / (0.08 * sr)).exp();
            self.amp_env *= release_coeff;
        }

        // Filter sweep: filt_env decays from 1.0 → 0.0 over sweep_time
        let sweep_coeff = (-1.0_f32 / (p.hoover_sweep_time * sr)).exp();
        if self.filt_env > 1e-5 {
            self.filt_env *= sweep_coeff;
        } else {
            self.filt_env = 0.0;
        }

        // Pitch LFO (sine, adds the wailing character)
        self.lfo_phase += p.hoover_pitch_lfo_rate / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo = (self.lfo_phase * std::f32::consts::TAU).sin();
        let freq_mod = 2.0_f32.powf(lfo * p.hoover_pitch_lfo_depth / 12.0);
        let eff_freq = self.freq * freq_mod;

        // Supersaw oscillator (same algorithm as Bass303)
        let n = p.hoover_voices.clamp(2, 7) as usize;
        let spread = p.hoover_detune; // semitone spread
        self.phase += eff_freq / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let mut osc_sum = self.phase * 2.0 - 1.0;
        for i in 0..(n - 1) {
            let t = if n > 2 {
                i as f32 / (n as f32 - 2.0)
            } else {
                0.5
            };
            let detune_st = (t - 0.5) * spread;
            let ratio = 2.0_f32.powf(detune_st / 12.0);
            self.unison_phases[i] += eff_freq * ratio / sr;
            if self.unison_phases[i] >= 1.0 {
                self.unison_phases[i] -= 1.0;
            }
            osc_sum += self.unison_phases[i] * 2.0 - 1.0;
        }
        let osc = osc_sum / n as f32 * 1.4;

        // Chamberlin SVF — highpass output
        // HP cutoff sweeps from filter_start (high) down to near-zero as filt_env decays.
        let hp_norm = (p.hoover_filter_start * self.filt_env).clamp(0.0, 1.0);
        let hp_hz = (200.0_f32 * 40.0_f32.powf(hp_norm)).min(sr * 0.45);
        let f = (std::f32::consts::PI * hp_hz / sr).clamp(0.001, 0.49);
        // q = damping; low q = high resonance. 0.92 gives q_min ≈ 0.08 at full resonance.
        let q = 1.0 - p.hoover_resonance * 0.92;
        let high = osc - self.svf_low - q * self.svf_band;
        let band_new = f * high + self.svf_band;
        let low_new = f * band_new + self.svf_low;
        self.svf_band = band_new;
        self.svf_low = low_new;

        high * self.amp_env * p.hoover_volume
    }
}
