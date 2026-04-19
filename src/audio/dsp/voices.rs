// ─── Low-level voice state machines ──────────────────────────────────────────
// Pure numeric DSP structs — no allocations.

/// Moog-style 4-pole ladder filter state.
#[derive(Clone, Copy, Default)]
pub(crate) struct LadderFilter {
    pub(crate) s: [f32; 4],
}

impl LadderFilter {
    /// `g` is the per-stage filter coefficient (0–~0.99).
    /// Callers must map cutoff to a proper g before calling.
    pub(crate) fn process(&mut self, input: f32, g: f32, resonance: f32) -> f32 {
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
pub(crate) struct OnePole {
    pub(crate) state: f32,
}

impl OnePole {
    pub(crate) fn process(&mut self, input: f32, coeff: f32) -> f32 {
        self.state = self.state * coeff + input * (1.0 - coeff);
        self.state
    }
}

/// PRNG for noise generation (xorshift32 — no stdlib, no heap).
#[derive(Clone, Copy)]
pub(crate) struct NoiseGen {
    pub(crate) state: u32,
}

impl NoiseGen {
    pub(crate) fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }
    pub(crate) fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

// ─── Drum voice generic base ──────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
pub(crate) struct Envelope {
    pub(crate) value: f32,
    pub(crate) active: bool,
}

impl Envelope {
    pub(crate) fn trigger(&mut self) {
        self.value = 1.0;
        self.active = true;
    }
    pub(crate) fn tick(&mut self, decay_coeff: f32) -> f32 {
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
        clip: f32,
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
        let raw = sine * amp + click * 0.3;

        // Hard-clip gabber drive: boost then clamp to ±1.
        // clip=0 → clean; clip=1 → 10× boost, flat-topped waveform.
        let clipped = if clip > 0.001 {
            let drive = 1.0 + clip * 9.0;
            (raw * drive).clamp(-1.0, 1.0)
        } else {
            raw
        };

        clipped * volume
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
    brown: f32,     // Brown noise integrator
    lp_state: f32,  // 1-pole LP filter state
    env: f32,       // amplitude envelope level
    lfo_phase: f32, // filter LFO phase (0–1)
    sh_phase: f32,  // S&H clock phase (0–1)
    sh_held: f32,   // current S&H value (-1..+1)
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
            env: 0.0,
            lfo_phase: 0.0,
            sh_phase: 0.0,
            sh_held: 0.0,
        }
    }

    /// Full noise voice with envelope, filter LFO, and S&H modulation.
    pub(super) fn process(&mut self, sr: f32, p: &super::AudioParams) -> f32 {
        let volume = if p.noise_voice_enabled {
            p.noise_voice_volume
        } else {
            0.0
        };
        let color = p.noise_voice_color;
        let cutoff = p.noise_voice_cutoff;
        let attack = p.noise_attack;
        let release = p.noise_release;
        let lfo_rate = p.noise_filter_lfo_rate;
        let lfo_depth = p.noise_filter_lfo_depth;
        let sh_rate = p.noise_sh_rate;
        let sh_depth = p.noise_sh_depth;

        // AR envelope: ramp up during attack, hold at 1.0, ramp down on release
        let attack_coeff = if attack > 0.001 {
            let ms = 1.0 + attack * attack * 4999.0; // 1ms–5s
            (-1.0 / (ms * 0.001 * sr)).exp()
        } else {
            0.0 // instant attack
        };
        let release_coeff = {
            let ms = 1.0 + release * release * 9999.0; // 1ms–10s
            (-1.0 / (ms * 0.001 * sr)).exp()
        };
        if volume > 0.001 {
            // Attack: ramp toward 1.0
            self.env = 1.0 - (1.0 - self.env) * attack_coeff;
        } else {
            // Release: decay toward 0.0
            self.env *= release_coeff;
        }
        if self.env < 0.0001 {
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

        // Filter LFO: slow sine modulation on cutoff
        let lfo_hz = 0.05 + lfo_rate * 9.95; // 0.05–10 Hz
        self.lfo_phase = (self.lfo_phase + lfo_hz / sr) % 1.0;
        let lfo_mod = (self.lfo_phase * std::f32::consts::TAU).sin() * lfo_depth * 0.4;

        // S&H: sample a new random value at the S&H rate
        let sh_hz = 0.5 + sh_rate * 19.5; // 0.5–20 Hz
        self.sh_phase += sh_hz / sr;
        if self.sh_phase >= 1.0 {
            self.sh_phase -= 1.0;
            self.sh_held = self.rng.next(); // -1..+1
        }
        let sh_mod = self.sh_held * sh_depth * 0.4;

        // 1-pole LP filter with modulation
        let mod_cutoff = (cutoff + lfo_mod + sh_mod).clamp(0.0, 1.0);
        let fc = (200.0 * (100.0f32).powf(mod_cutoff)).min(sr * 0.45);
        let a = 1.0 - (-std::f32::consts::TAU * fc / sr).exp();
        self.lp_state += a * (out - self.lp_state);

        self.lp_state * self.env * volume
    }
}

// ─── Hoover Lead Voice ────────────────────────────────────────────────────────
// Supersaw oscillator into a resonant lowpass filter that starts open (bright)
// and sweeps DOWN as the envelope decays. High resonance creates a moving
// resonant peak — the authentic "vacuum cleaner" hoover sweep.
// Named after Human Resource "Dominator" (1991).

#[derive(Clone)]
pub(super) struct HooverVoice {
    phase: f32,
    unison_phases: [f32; 6],
    freq: f32,
    /// Glide target.  Equal to `freq` when no slide is in progress.
    target_freq: f32,
    gate: bool,
    amp_env: f32,  // VCA envelope
    filt_env: f32, // Filter sweep: 1.0 = LP wide open (bright), decays to 0.0 (dark)
    svf_low: f32,
    svf_band: f32,
    lfo_phase: f32,
    /// Per-step accent (0..=1).  Scales the final output level: 0 leaves the
    /// voice at its baseline, 1 lifts by `ACCENT_LIFT` (see `process`).
    /// Populated by melodic preecho's `accent_ramp`.
    accent: f32,
    /// Per-step slide (0..=1).  Non-zero enables an exponential pitch glide
    /// from `freq` → `target_freq`; zero snaps.  Populated by melodic
    /// preecho's `slide_cascade`.
    slide: f32,
}

impl HooverVoice {
    pub(super) fn new() -> Self {
        Self {
            phase: 0.0,
            unison_phases: [0.0; 6],
            freq: 220.0,
            target_freq: 220.0,
            gate: false,
            amp_env: 0.0,
            filt_env: 0.0,
            svf_low: 0.0,
            svf_band: 0.0,
            lfo_phase: 0.0,
            accent: 0.0,
            slide: 0.0,
        }
    }

    pub(super) fn trigger(&mut self, note: u8, tuning: u8, accent: f32, slide: f32) {
        let new_freq = super::dsp_util::midi_to_hz_tuned(note, tuning);
        self.accent = accent.clamp(0.0, 1.0);
        self.slide = slide.clamp(0.0, 1.0);
        // When slide > 0 we leave `self.freq` where it was so `process`
        // glides into `target_freq`; otherwise we snap.
        if self.slide > 0.0 && self.freq > 0.0 {
            self.target_freq = new_freq;
        } else {
            self.freq = new_freq;
            self.target_freq = new_freq;
        }
        self.gate = true;
        self.amp_env = 0.0; // will rise on fast attack
        self.filt_env = 1.0; // start wide open (LP fully bright)
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

        // Slide glide: exponential approach from self.freq → target_freq.
        // Glide time ramps with slide intensity so a `slide_cascade` step
        // produces an audibly-smeared lead-in; slide=0 short-circuits the
        // update path (target == freq).
        if self.slide > 0.0 && (self.freq - self.target_freq).abs() > 1e-3 {
            let glide_time_s = 0.01 + self.slide * 0.15; // 10..160 ms
            let coeff = (-1.0_f32 / (glide_time_s * sr)).exp();
            self.freq = self.target_freq - (self.target_freq - self.freq) * coeff;
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

        // Chamberlin SVF — LP + BP mix for authentic resonant sweep.
        //
        // The LP provides the low-frequency body; the BP adds the moving resonant
        // peak that gives the classic hoover its "singing" character. Mixing them
        // (controlled by resonance) recreates the Human Resource / Dominator
        // "oooh→dark" sweep without needing a separate BP-mix parameter.
        let lp_norm = (p.hoover_filter_start * self.filt_env).clamp(0.0, 1.0);
        // Exponential map: 0 → ~120 Hz (closed), 1 → ~12 kHz (open)
        let lp_hz = (120.0_f32 * 100.0_f32.powf(lp_norm)).min(sr * 0.45);
        let f = (std::f32::consts::PI * lp_hz / sr).clamp(0.001, 0.49);
        // q = damping; low q = high resonance. 0.97 tightens the peak vs 0.95.
        let q = (1.0 - p.hoover_resonance * 0.97).max(0.03);
        let high = osc - self.svf_low - q * self.svf_band;
        let band_new = f * high + self.svf_band;
        let low_new = f * band_new + self.svf_low;
        self.svf_band = band_new;
        self.svf_low = low_new;

        // Mix LP (body) with BP (resonant peak).  At resonance=0.82, bp_amount≈0.49
        // — enough peak to make the sweep sing without overpowering the bass.
        let bp_amount = (p.hoover_resonance * 0.6).clamp(0.0, 0.75);
        let mixed = low_new * (1.0 - bp_amount) + band_new * bp_amount;
        // Soft saturation via tanh to round off harsh clipping at high resonance
        let saturated = mixed.tanh() * 1.15;

        // Accent lift: 0 leaves baseline, 1 scales output up by ACCENT_LIFT.
        // Keeps legacy un-accented triggers at their original level.
        const ACCENT_LIFT: f32 = 0.3;
        let accent_gain = 1.0 + ACCENT_LIFT * self.accent;

        saturated * self.amp_env * p.hoover_volume * accent_gain
    }
}

// ─── AN1X-style virtual analog voice ─────────────────────────────────────────

/// ADSR envelope phase.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum AdsrPhase {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

// ADSR max times (ms). Attack and release are longer for glacial pads.
const ADSR_MAX_ATTACK_MS: f32 = 10_000.0; // 10 s
const ADSR_MAX_DECAY_MS: f32 = 8_000.0; // 8 s
const ADSR_MAX_RELEASE_MS: f32 = 30_000.0; // 30 s

/// Maps a 0–1 ADSR time knob to samples. Quadratic curve: 0→1ms, 1→max_ms.
#[inline]
pub(crate) fn adsr_samples(v: f32, sr: f32, max_ms: f32) -> f32 {
    let ms = 1.0 + v * v * (max_ms - 1.0);
    (ms * 0.001 * sr).max(1.0)
}

/// Step one ADSR sample; returns current envelope value.
pub(crate) fn adsr_tick(
    phase: &mut AdsrPhase,
    val: &mut f32,
    gate: bool,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    sr: f32,
) {
    if gate && *phase == AdsrPhase::Release {
        *phase = AdsrPhase::Attack; // re-trigger: re-enter attack from current level
    }
    match *phase {
        AdsrPhase::Idle => {}
        AdsrPhase::Attack => {
            let target = 1.0_f32;
            let coeff = (-1.0 / adsr_samples(attack, sr, ADSR_MAX_ATTACK_MS)).exp();
            *val = target - (target - *val) * coeff;
            if *val >= 0.9999 {
                *val = 1.0;
                *phase = AdsrPhase::Decay;
            }
        }
        AdsrPhase::Decay => {
            let coeff = (-1.0 / adsr_samples(decay, sr, ADSR_MAX_DECAY_MS)).exp();
            *val = sustain + (*val - sustain) * coeff;
            if (*val - sustain).abs() < 0.001 {
                *val = sustain;
                *phase = AdsrPhase::Sustain;
            }
        }
        AdsrPhase::Sustain => {
            *val = sustain;
            if !gate {
                *phase = AdsrPhase::Release;
            }
        }
        AdsrPhase::Release => {
            let coeff = (-1.0 / adsr_samples(release, sr, ADSR_MAX_RELEASE_MS)).exp();
            *val *= coeff;
            if *val < 0.0001 {
                *val = 0.0;
                *phase = AdsrPhase::Idle;
            }
        }
    }
}

/// Generate one sample from a waveform. `phase` is 0–1.
#[inline]
pub(crate) fn osc_sample(wave: u8, phase: f32, noise_state: &mut u32) -> f32 {
    match wave {
        0 => phase * 2.0 - 1.0, // saw
        1 => {
            if phase < 0.5 {
                1.0
            } else {
                -1.0
            }
        } // square
        2 => 1.0 - 4.0 * (phase - 0.5).abs(), // triangle (bipolar)
        3 => (phase * std::f32::consts::TAU).sin(), // sine
        _ => {
            // noise (LCG)
            *noise_state = noise_state.wrapping_mul(1664525).wrapping_add(1013904223);
            (*noise_state as i32) as f32 / i32::MAX as f32
        }
    }
}

/// Map DrumVoice to its index in the drum_velocity array (matches DrumVoice::ALL order).
pub(crate) fn drum_voice_idx(voice: &crate::state::DrumVoice) -> usize {
    use crate::state::DrumVoice::*;
    match voice {
        Kick808 => 0,
        Snare808 => 1,
        HihatClosed808 => 2,
        HihatOpen808 => 3,
        TomHi808 => 4,
        TomMid808 => 5,
        TomLo808 => 6,
        Kick909 => 7,
        Snare909 => 8,
        HihatClosed909 => 9,
        HihatOpen909 => 10,
        Clap909 => 11,
        Rim909 => 12,
        Amen => 13,
        GabberKick => 14,
    }
}

// Amen/WAV and Granular voices are in samplers.rs (split for line-count limit).
