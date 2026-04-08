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
// Supersaw oscillator into a resonant lowpass filter that starts open (bright)
// and sweeps DOWN as the envelope decays. High resonance creates a moving
// resonant peak — the authentic "vacuum cleaner" hoover sweep.
// Named after Human Resource "Dominator" (1991).

#[derive(Clone)]
pub(super) struct HooverVoice {
    phase: f32,
    unison_phases: [f32; 6],
    freq: f32,
    gate: bool,
    amp_env: f32,  // VCA envelope
    filt_env: f32, // Filter sweep: 1.0 = LP wide open (bright), decays to 0.0 (dark)
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

        saturated * self.amp_env * p.hoover_volume
    }
}

// ─── AN1X-style virtual analog voice ─────────────────────────────────────────

/// ADSR envelope phase.
#[derive(Clone, Copy, PartialEq)]
enum AdsrPhase {
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
fn adsr_samples(v: f32, sr: f32, max_ms: f32) -> f32 {
    let ms = 1.0 + v * v * (max_ms - 1.0);
    (ms * 0.001 * sr).max(1.0)
}

/// Step one ADSR sample; returns current envelope value.
fn adsr_tick(
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
fn osc_sample(wave: u8, phase: f32, noise_state: &mut u32) -> f32 {
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

pub(super) struct An1xVoice {
    active: bool,
    gate: bool,
    note: f32,          // current target MIDI note (float for glide)
    current_pitch: f32, // actual pitch in semitones (gliding toward note)

    osc1_phase: f32,
    osc2_phase: f32,
    sub_phase: f32,
    noise_state: u32,

    amp_phase: AdsrPhase,
    amp_val: f32,
    filt_phase: AdsrPhase,
    filt_val: f32,
    pitch_phase: AdsrPhase, // AD-only pitch envelope
    pitch_val: f32,

    svf_low: f32,
    svf_band: f32,

    lfo_phase: f32,
    lfo_delay_samples: u32, // counts down; LFO fades in as this approaches 0
    lfo_depth_cur: f32,     // current LFO depth (fades in from 0)

    drift_target: f32,
    drift_prev: f32,
    drift_step_counter: u32,
}

impl An1xVoice {
    pub(super) fn new() -> Self {
        Self {
            active: false,
            gate: false,
            note: 60.0,
            current_pitch: 60.0,
            osc1_phase: 0.0,
            osc2_phase: 0.0,
            sub_phase: 0.0,
            noise_state: 0xDEAD_BEEF,
            amp_phase: AdsrPhase::Idle,
            amp_val: 0.0,
            filt_phase: AdsrPhase::Idle,
            filt_val: 0.0,
            pitch_phase: AdsrPhase::Idle,
            pitch_val: 0.0,
            svf_low: 0.0,
            svf_band: 0.0,
            lfo_phase: 0.0,
            lfo_delay_samples: 0,
            lfo_depth_cur: 0.0,
            drift_target: 0.0,
            drift_prev: 0.0,
            drift_step_counter: 0,
        }
    }

    pub(super) fn trigger(&mut self, note: u8, sr: f32, p: &super::AudioParams) {
        let new_note = note as f32;
        // Legato mode: snap to new note only when gate was off (staccato).
        // Always-glide mode: never snap — glide from wherever current_pitch is.
        if p.an1x_glide_legato && !self.gate {
            self.current_pitch = new_note;
        }
        self.note = new_note;
        self.gate = true;
        self.active = true;
        self.amp_phase = AdsrPhase::Attack;
        self.filt_phase = AdsrPhase::Attack;
        self.pitch_phase = AdsrPhase::Attack;
        self.pitch_val = 0.0;
        // Reset LFO delay counter
        let delay_secs = p.an1x_lfo_delay * 4.0; // 0–4 s
        self.lfo_delay_samples = (delay_secs * sr) as u32;
        self.lfo_depth_cur = 0.0;
    }

    pub(super) fn gate_off(&mut self) {
        self.gate = false;
        if self.amp_phase != AdsrPhase::Idle {
            self.amp_phase = AdsrPhase::Release;
        }
        if self.filt_phase != AdsrPhase::Idle {
            self.filt_phase = AdsrPhase::Release;
        }
    }

    pub(super) fn process(&mut self, sr: f32, p: &super::AudioParams) -> f32 {
        if !self.active || (!self.gate && self.amp_val < 1e-5) {
            self.active = false;
            return 0.0;
        }

        // ── Glide ────────────────────────────────────────────────────────────
        let glide_time = p.an1x_glide_time * 0.5; // 0–500 ms
        if glide_time > 0.001 {
            let coeff = (-1.0_f32 / (glide_time * sr)).exp();
            self.current_pitch = self.note - (self.note - self.current_pitch) * coeff;
        } else {
            self.current_pitch = self.note;
        }

        // ── Drift (random micro-pitch wobble) ────────────────────────────────
        self.drift_step_counter += 1;
        let drift_period = (sr * 0.1) as u32; // new drift target every 100ms
        if self.drift_step_counter >= drift_period {
            self.drift_step_counter = 0;
            self.drift_prev = self.drift_target;
            self.noise_state = self
                .noise_state
                .wrapping_mul(1664525)
                .wrapping_add(1013904223);
            self.drift_target = (self.noise_state as i32) as f32 / i32::MAX as f32;
        }
        let drift_t = self.drift_step_counter as f32 / drift_period as f32;
        let drift_st = (self.drift_prev + (self.drift_target - self.drift_prev) * drift_t)
            * p.an1x_drift
            * 0.15; // max ±0.15 semitones

        // ── LFO ─────────────────────────────────────────────────────────────
        self.lfo_phase += p.an1x_lfo_rate_hz / sr;
        if self.lfo_phase >= 1.0 {
            self.lfo_phase -= 1.0;
        }
        let lfo_raw = (self.lfo_phase * std::f32::consts::TAU).sin();

        // LFO delay fade-in
        if self.lfo_delay_samples > 0 {
            self.lfo_delay_samples -= 1;
            // Fade depth in over the last quarter of the delay period
            // (keep depth_cur = 0 until delay expires, then ramp up)
        } else {
            self.lfo_depth_cur = (self.lfo_depth_cur + 0.0005).min(p.an1x_lfo_depth);
        }
        let lfo = lfo_raw * self.lfo_depth_cur;

        // ── Pitch envelope (AD only — sustain=0, release=decay) ─────────────
        adsr_tick(
            &mut self.pitch_phase,
            &mut self.pitch_val,
            false, // gate=false → env decays immediately after attack
            p.an1x_pitch_env_attack,
            p.an1x_pitch_env_decay,
            0.0, // sustain = 0
            p.an1x_pitch_env_decay,
            sr,
        );
        let pitch_env_st = (p.an1x_pitch_env_amount - 0.5) * 48.0 * self.pitch_val; // ±24 st

        // ── OSC frequencies ─────────────────────────────────────────────────
        let pitch_lfo_st = if p.an1x_lfo_target == 0 {
            lfo * 2.0
        } else {
            0.0
        }; // ±2 st
        let pitch_st = self.current_pitch + drift_st + pitch_lfo_st + pitch_env_st;
        let base_freq = super::midi_to_hz(pitch_st.round() as u8)
            * 2.0_f32.powf((pitch_st - pitch_st.round()) / 12.0);

        let osc2_detune_st = (p.an1x_osc2_detune - 0.5) * 48.0; // -24..+24 semitones
        let osc2_octave_shift = p.an1x_osc2_octave as f32 * 12.0;
        let osc2_freq = base_freq * 2.0_f32.powf((osc2_detune_st + osc2_octave_shift) / 12.0);

        self.osc1_phase += base_freq / sr;
        let osc1_wrapped = self.osc1_phase >= 1.0;
        if osc1_wrapped {
            self.osc1_phase -= 1.0;
        }
        self.osc2_phase += osc2_freq / sr;
        // Hard sync: reset OSC2 on every OSC1 cycle boundary
        if p.an1x_hard_sync && osc1_wrapped {
            self.osc2_phase = 0.0;
        }
        if self.osc2_phase >= 1.0 {
            self.osc2_phase -= 1.0;
        }
        let sub_freq = base_freq * 0.5;
        self.sub_phase += sub_freq / sr;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= 1.0;
        }

        let s1 = osc_sample(p.an1x_osc1_wave, self.osc1_phase, &mut self.noise_state);
        let s2 = osc_sample(p.an1x_osc2_wave, self.osc2_phase, &mut self.noise_state);
        let sub = if self.sub_phase < 0.5 {
            1.0_f32
        } else {
            -1.0_f32
        }; // sub square

        let ring = s1 * s2;
        let mut osc_mix = s1 * p.an1x_osc1_level
            + s2 * p.an1x_osc2_level
            + sub * p.an1x_sub_level
            + if p.an1x_ring_mod { ring * 0.5 } else { 0.0 };
        osc_mix *= 0.5; // normalise against potential loud mix

        // ── Filter ADSR ──────────────────────────────────────────────────────
        adsr_tick(
            &mut self.filt_phase,
            &mut self.filt_val,
            self.gate,
            p.an1x_filter_attack,
            p.an1x_filter_decay,
            p.an1x_filter_sustain,
            p.an1x_filter_release,
            sr,
        );

        // Filter cutoff modulation
        let cutoff_lfo = if p.an1x_lfo_target == 1 {
            lfo * 0.3
        } else {
            0.0
        };
        let env_amount = (p.an1x_filter_env_amount - 0.5) * 2.0; // -1..+1
        let key_track = (self.current_pitch - 60.0) / 12.0 * p.an1x_filter_key_track;
        let cutoff_norm = (p.an1x_filter_cutoff
            + env_amount * self.filt_val * 0.4
            + key_track * 0.1
            + cutoff_lfo)
            .clamp(0.0, 1.0);
        let cutoff_hz = (80.0_f32 * 225.0_f32.powf(cutoff_norm)).min(sr * 0.45);
        let f_coeff = (std::f32::consts::PI * cutoff_hz / sr).clamp(0.001, 0.49);
        // Allow q to reach near-zero for self-oscillation at resonance ≥ ~0.95
        let q = (1.0 - p.an1x_filter_resonance * 0.995).max(0.005);
        // Reduce input at high resonance to prevent blow-up when self-oscillating
        let input_gain = 1.0 - p.an1x_filter_resonance * 0.65;

        // Chamberlin SVF (LP / HP / BP selector)
        let high = osc_mix * input_gain - self.svf_low - q * self.svf_band;
        let band_new = f_coeff * high + self.svf_band;
        let low_new = f_coeff * band_new + self.svf_low;
        // Soft-clip the band to prevent blow-up; produces a sine at self-oscillation
        self.svf_band = band_new.tanh();
        self.svf_low = low_new.clamp(-1.5, 1.5);
        let filtered = match p.an1x_filter_mode {
            0 => self.svf_low,
            1 => high,
            _ => self.svf_band,
        };

        // ── Amplitude ADSR ───────────────────────────────────────────────────
        let amp_lfo = if p.an1x_lfo_target == 2 {
            lfo * 0.25
        } else {
            0.0
        };
        adsr_tick(
            &mut self.amp_phase,
            &mut self.amp_val,
            self.gate,
            p.an1x_amp_attack,
            p.an1x_amp_decay,
            p.an1x_amp_sustain,
            p.an1x_amp_release,
            sr,
        );
        let amp = (self.amp_val + amp_lfo).clamp(0.0, 1.0);

        if self.amp_phase == AdsrPhase::Idle {
            self.active = false;
        }

        filtered * amp * p.an1x_volume
    }
}

/// Map DrumVoice to its index in the drum_velocity array (matches DrumVoice::ALL order).
pub(super) fn drum_voice_idx(voice: &crate::state::DrumVoice) -> usize {
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
    }
}

// Amen/WAV and Granular voices are in samplers.rs (split for line-count limit).
