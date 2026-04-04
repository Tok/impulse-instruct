// ─── audio/dsp.rs ─────────────────────────────────────────────────────────────
// Pure DSP — all synthesis happens here.
// No allocations inside process_block(). State machines, not globals.
//
// Synthesis approach:
//   TB-303: saw/square osc → Moog-style ladder filter → VCA
//   808 Kick: sine w/ exponential pitch drop + punch transient
//   808 Snare: tuned tone + filtered noise
//   808 HiHat: high-passed noise
//   909 variants: same architecture, different tuning/character
//   FX: simple reverb (Schroeder), echo delay, waveshaper drive

use crate::sequencer::TriggerEvent;
use crate::state::{AppState, DrumVoice, Waveform};

const MAX_DELAY_SAMPLES: usize = 96_000; // 2s @ 48kHz
const REVERB_COMBS: usize = 8;
const REVERB_ALLPASS: usize = 4;

// ─── AudioParams snapshot (copied from AppState for audio thread) ──────────────

#[derive(Clone, Copy, Debug)]
pub struct AudioParams {
    // 303
    pub cutoff: f32,
    pub resonance: f32,
    pub env_mod: f32,
    pub decay_303: f32,
    pub accent_level: f32,
    pub waveform_saw: bool,      // true = saw, false = square
    pub waveform_supersaw: bool, // true = supersaw (overrides waveform_saw)
    pub supersaw_detune: f32,    // 0–1 → 0–1 semitone spread
    pub supersaw_voices: u8,     // 2–7
    pub sub_osc_level: f32,      // 0–1 sub-oscillator mix level
    pub distortion_303: f32,
    pub volume_303: f32,
    // 808 kick
    pub kick808_pitch: f32,
    pub kick808_decay: f32,
    pub kick808_punch: f32,
    pub kick808_volume: f32,
    pub kick808_pitch_env_depth: f32,
    pub kick808_pitch_env_time: f32,
    // 808 snare
    pub snare808_tone: f32,
    pub snare808_snappy: f32,
    pub snare808_decay: f32,
    pub snare808_volume: f32,
    // 808 hihats
    pub hihat_closed808_decay: f32,
    pub hihat_open808_decay: f32,
    pub hihat808_volume: f32,
    // 909 kick
    pub kick909_pitch: f32,
    pub kick909_decay: f32,
    pub kick909_punch: f32,
    pub kick909_volume: f32,
    pub kick909_pitch_env_depth: f32,
    pub kick909_pitch_env_time: f32,
    // 909 snare
    pub snare909_tone: f32,
    pub snare909_snappy: f32,
    pub snare909_decay: f32,
    pub snare909_volume: f32,
    // 909 hihat/clap
    pub hihat_closed909_decay: f32,
    pub hihat_open909_decay: f32,
    pub hihat909_volume: f32,
    pub clap909_decay: f32,
    pub clap909_volume: f32,
    // FX
    pub reverb_size: f32,
    pub reverb_damp: f32,
    pub reverb_mix: f32,
    pub delay_time: f32,
    pub delay_feedback: f32,
    pub delay_mix: f32,
    pub distortion_drive: f32,
    pub distortion_mix: f32,
    pub master_volume: f32,
    // Bitcrush
    pub bitcrush_bits: f32,
    pub bitcrush_rate: f32,
    pub bitcrush_mix: f32,
    // Sample rate
    pub sample_rate: f32,
}

impl AudioParams {
    pub fn from_app_state(s: &AppState) -> Self {
        Self {
            cutoff: s.bass.cutoff,
            resonance: s.bass.resonance,
            env_mod: s.bass.env_mod,
            decay_303: s.bass.decay,
            accent_level: s.bass.accent_level,
            waveform_saw: s.bass.waveform == Waveform::Saw,
            waveform_supersaw: s.bass.waveform == Waveform::Supersaw,
            supersaw_detune: s.bass.supersaw_detune,
            supersaw_voices: s.bass.supersaw_voices,
            sub_osc_level: s.bass.sub_osc_level,
            distortion_303: s.bass.distortion,
            volume_303: s.bass.volume,
            kick808_pitch: s.kit_a.kick.pitch,
            kick808_decay: s.kit_a.kick.decay,
            kick808_punch: s.kit_a.kick.punch,
            kick808_volume: s.kit_a.kick.volume,
            kick808_pitch_env_depth: s.kit_a.kick.pitch_env_depth,
            kick808_pitch_env_time: s.kit_a.kick.pitch_env_time,
            snare808_tone: s.kit_a.snare.tone,
            snare808_snappy: s.kit_a.snare.snappy,
            snare808_decay: s.kit_a.snare.decay,
            snare808_volume: s.kit_a.snare.volume,
            hihat_closed808_decay: s.kit_a.hihat_closed.decay,
            hihat_open808_decay: s.kit_a.hihat_open.decay,
            hihat808_volume: s.kit_a.hihat_closed.volume,
            kick909_pitch: s.kit_b.kick.pitch,
            kick909_decay: s.kit_b.kick.decay,
            kick909_punch: s.kit_b.kick.punch,
            kick909_volume: s.kit_b.kick.volume,
            kick909_pitch_env_depth: s.kit_b.kick.pitch_env_depth,
            kick909_pitch_env_time: s.kit_b.kick.pitch_env_time,
            snare909_tone: s.kit_b.snare.tone,
            snare909_snappy: s.kit_b.snare.snappy,
            snare909_decay: s.kit_b.snare.decay,
            snare909_volume: s.kit_b.snare.volume,
            hihat_closed909_decay: s.kit_b.hihat_closed.decay,
            hihat_open909_decay: s.kit_b.hihat_open.decay,
            hihat909_volume: s.kit_b.hihat_closed.volume,
            clap909_decay: s.kit_b.clap.decay,
            clap909_volume: s.kit_b.clap.volume,
            reverb_size: s.fx.reverb_size,
            reverb_damp: s.fx.reverb_damp,
            reverb_mix: s.fx.reverb_mix,
            delay_time: s.fx.delay_time,
            delay_feedback: s.fx.delay_feedback,
            delay_mix: s.fx.delay_mix,
            distortion_drive: s.fx.distortion_drive,
            distortion_mix: s.fx.distortion_mix,
            master_volume: s.fx.master_volume,
            bitcrush_bits: s.fx.bitcrush_bits,
            bitcrush_rate: s.fx.bitcrush_rate,
            bitcrush_mix: s.fx.bitcrush_mix,
            sample_rate: 44100.0,
        }
    }
}

// ─── Low-level voice state machines ──────────────────────────────────────────

/// Moog-style 4-pole ladder filter state.
#[derive(Clone, Copy, Default)]
struct LadderFilter {
    s: [f32; 4],
}

impl LadderFilter {
    /// `g` is the per-stage filter coefficient (0–~0.99).
    /// Callers must map cutoff to a proper g before calling.
    fn process(&mut self, input: f32, g: f32, resonance: f32) -> f32 {
        let f = g.clamp(0.001, 0.99);
        let k = resonance * 4.0; // self-oscillation at k=4.0

        // Feedback
        let fb = k * self.s[3];
        let x = input - fb;

        // 4 one-pole stages
        self.s[0] = self.s[0] + f * (tanh(x) - tanh(self.s[0]));
        self.s[1] = self.s[1] + f * (tanh(self.s[0]) - tanh(self.s[1]));
        self.s[2] = self.s[2] + f * (tanh(self.s[1]) - tanh(self.s[2]));
        self.s[3] = self.s[3] + f * (tanh(self.s[2]) - tanh(self.s[3]));
        self.s[3]
    }
}

fn tanh(x: f32) -> f32 {
    // Fast tanh approximation
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Simple one-pole smoothing filter.
#[derive(Clone, Copy, Default)]
struct OnePole {
    state: f32,
}

impl OnePole {
    fn process(&mut self, input: f32, coeff: f32) -> f32 {
        self.state = self.state * coeff + input * (1.0 - coeff);
        self.state
    }
}

/// PRNG for noise generation (xorshift32 — no stdlib, no heap).
#[derive(Clone, Copy)]
struct NoiseGen {
    state: u32,
}

impl NoiseGen {
    fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }
    fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

// ─── TB-303 voice ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Bass303 {
    phase: f32,              // oscillator phase 0-1 (voice 0)
    sub_phase: f32,          // sub-oscillator phase (one octave below)
    unison_phases: [f32; 6], // phases for voices 1–6 (supersaw)
    freq: f32,               // current freq Hz
    target_freq: f32,        // slide target
    amp_env: f32,            // VCA envelope
    filt_env: f32,           // filter envelope
    gate: bool,
    accent: bool,
    slide: bool,
    filter: LadderFilter,
}

impl Default for Bass303 {
    fn default() -> Self {
        Self {
            phase: 0.0,
            sub_phase: 0.0,
            unison_phases: [0.0, 0.142, 0.285, 0.428, 0.571, 0.714], // spread across cycle
            freq: 110.0,
            target_freq: 110.0,
            amp_env: 0.0,
            filt_env: 0.0,
            gate: false,
            accent: false,
            slide: false,
            filter: LadderFilter::default(),
        }
    }
}

impl Bass303 {
    fn trigger(&mut self, note: u8, accent: bool, slide: bool) {
        let new_freq = midi_to_hz(note);
        if !self.slide {
            self.freq = new_freq;
        }
        self.target_freq = new_freq;
        self.accent = accent;
        self.slide = slide;
        self.gate = true;
        if !slide {
            self.amp_env = if accent { 1.0 } else { 0.8 };
            self.filt_env = 1.0;
        }
    }

    fn gate_off(&mut self) {
        self.gate = false;
    }

    fn process(&mut self, p: &AudioParams) -> f32 {
        let sr = p.sample_rate;

        // Slide: portamento toward target
        if self.slide {
            let slide_coeff = (-1.0 / (sr * 0.06)).exp(); // 60ms slide
            self.freq = self.freq + (self.target_freq - self.freq) * (1.0 - slide_coeff);
        }

        // Oscillator
        self.phase += self.freq / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let osc = if p.waveform_supersaw {
            // Supersaw: N detuned saws mixed and normalised
            let n = p.supersaw_voices.clamp(2, 7) as usize;
            // Detune spread: 0–1 maps to 0–1 semitone total spread
            // Each voice is offset by i / (n-1) semitones from -spread/2 to +spread/2
            let spread_semitones = p.supersaw_detune; // 0–1 semitone range
            let mut sum = self.phase * 2.0 - 1.0; // voice 0 at centre pitch
            for i in 0..(n - 1) {
                let t = if n > 2 {
                    i as f32 / (n as f32 - 2.0)
                } else {
                    0.5
                };
                let detune_st = (t - 0.5) * spread_semitones;
                let ratio = 2.0f32.powf(detune_st / 12.0);
                self.unison_phases[i] += self.freq * ratio / sr;
                if self.unison_phases[i] >= 1.0 {
                    self.unison_phases[i] -= 1.0;
                }
                sum += self.unison_phases[i] * 2.0 - 1.0;
            }
            (sum / n as f32) * 1.4 // slight gain boost to compensate cancellation
        } else if p.waveform_saw {
            self.phase * 2.0 - 1.0
        } else {
            // Square with slight PWM
            if self.phase < 0.5 { 1.0 } else { -1.0 }
        };

        // Sub-oscillator: sine one octave below, mixed before filter
        self.sub_phase += self.freq * 0.5 / sr;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= 1.0;
        }
        let sub = (self.sub_phase * std::f32::consts::TAU).sin();
        let osc = osc + sub * p.sub_osc_level;

        // Envelope decay coefficients
        let accent_mult = if self.accent {
            p.accent_level * 0.4 + 0.8
        } else {
            1.0
        };
        let env_decay_coeff = {
            let t = p.decay_303 * 1.9 + 0.1; // 0.1–2.0 s decay
            (-1.0 / (sr * t)).exp()
        };

        // AMP envelope
        if !self.gate {
            self.amp_env *= 0.9995;
        }

        // Filter envelope
        self.filt_env *= env_decay_coeff;

        // Dynamic cutoff: 0-1 → 200-8000 Hz (exponential) → per-stage coefficient g
        let cutoff_env = (p.cutoff + self.filt_env * p.env_mod * accent_mult).clamp(0.0, 1.0);
        let cutoff_hz = 200.0 * (40.0f32).powf(cutoff_env); // 200 Hz at 0 → 8000 Hz at 1
        let g = {
            let w = cutoff_hz / sr;
            (w / (1.0 + w)).clamp(0.001, 0.99)
        };

        // Ladder filter
        let filtered = self.filter.process(osc, g, p.resonance * 0.97);

        // Soft clip distortion
        let dist = if p.distortion_303 > 0.01 {
            let drive = p.distortion_303 * 8.0 + 1.0;
            tanh(filtered * drive) / tanh(drive)
        } else {
            filtered
        };

        dist * self.amp_env * p.volume_303 * accent_mult
    }
}

// ─── Drum voice generic base ──────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
struct Envelope {
    value: f32,
    active: bool,
}

impl Envelope {
    fn trigger(&mut self) {
        self.value = 1.0;
        self.active = true;
    }
    fn tick(&mut self, decay_coeff: f32) -> f32 {
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
struct Kick {
    phase: f32,
    pitch_env: Envelope,
    amp_env: Envelope,
    noise_gen: NoiseGen,
    punch_env: Envelope,
}

impl Kick {
    fn new(seed: u32) -> Self {
        Self {
            phase: 0.0,
            pitch_env: Envelope::default(),
            amp_env: Envelope::default(),
            noise_gen: NoiseGen::new(seed),
            punch_env: Envelope::default(),
        }
    }

    fn trigger(&mut self) {
        self.pitch_env.trigger();
        self.amp_env.trigger();
        self.punch_env.trigger();
        self.phase = 0.0;
    }

    fn process(
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
struct Snare {
    phase: f32,
    tone_env: Envelope,
    noise_env: Envelope,
    noise_filter: OnePole,
    noise_gen: NoiseGen,
}

impl Snare {
    fn new(seed: u32) -> Self {
        Self {
            phase: 0.0,
            tone_env: Envelope::default(),
            noise_env: Envelope::default(),
            noise_filter: OnePole::default(),
            noise_gen: NoiseGen::new(seed),
        }
    }

    fn trigger(&mut self) {
        self.tone_env.trigger();
        self.noise_env.trigger();
    }

    fn process(&mut self, tone: f32, snappy: f32, decay: f32, volume: f32, sr: f32) -> f32 {
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
struct HiHat {
    amp_env: Envelope,
    noise_filter: OnePole,
    noise_gen: NoiseGen,
}

impl HiHat {
    fn new(seed: u32) -> Self {
        Self {
            amp_env: Envelope::default(),
            noise_filter: OnePole::default(),
            noise_gen: NoiseGen::new(seed),
        }
    }

    fn trigger(&mut self) {
        self.amp_env.trigger();
    }

    fn process(&mut self, decay: f32, tone: f32, volume: f32, sr: f32) -> f32 {
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
struct Clap {
    amp_env: Envelope,
    burst_env: Envelope,
    noise_gen: NoiseGen,
    burst_count: u8,
    burst_timer: u32,
}

impl Clap {
    fn new(seed: u32) -> Self {
        Self {
            amp_env: Envelope::default(),
            burst_env: Envelope::default(),
            noise_gen: NoiseGen::new(seed),
            burst_count: 0,
            burst_timer: 0,
        }
    }

    fn trigger(&mut self) {
        self.amp_env.trigger();
        self.burst_env.trigger();
        self.burst_count = 3;
        self.burst_timer = 0;
    }

    fn process(&mut self, decay: f32, volume: f32, sr: f32) -> f32 {
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

// ─── Simple Schroeder reverb ──────────────────────────────────────────────────

struct Reverb {
    comb_delays: [Vec<f32>; REVERB_COMBS],
    comb_ptrs: [usize; REVERB_COMBS],
    comb_filters: [f32; REVERB_COMBS],
    allpass_delays: [Vec<f32>; REVERB_ALLPASS],
    allpass_ptrs: [usize; REVERB_ALLPASS],
}

// Freeverb-inspired delay lengths (prime-ish, tuned for ~44.1kHz)
const COMB_LENGTHS: [usize; REVERB_COMBS] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_LENGTHS: [usize; REVERB_ALLPASS] = [556, 441, 341, 225];

impl Reverb {
    fn new() -> Self {
        Self {
            comb_delays: std::array::from_fn(|i| vec![0.0f32; COMB_LENGTHS[i]]),
            comb_ptrs: [0; REVERB_COMBS],
            comb_filters: [0.0; REVERB_COMBS],
            allpass_delays: std::array::from_fn(|i| vec![0.0f32; ALLPASS_LENGTHS[i]]),
            allpass_ptrs: [0; REVERB_ALLPASS],
        }
    }

    fn process(&mut self, input: f32, room_size: f32, damp: f32) -> f32 {
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

struct DelayLine {
    buf: Vec<f32>,
    ptr: usize,
}

impl DelayLine {
    fn new() -> Self {
        Self {
            buf: vec![0.0; MAX_DELAY_SAMPLES],
            ptr: 0,
        }
    }

    fn process(&mut self, input: f32, delay_samples: usize, feedback: f32) -> f32 {
        let delay_samples = delay_samples.clamp(1, MAX_DELAY_SAMPLES - 1);
        let read_ptr = (self.ptr + MAX_DELAY_SAMPLES - delay_samples) % MAX_DELAY_SAMPLES;
        let delayed = self.buf[read_ptr];
        self.buf[self.ptr] = input + delayed * feedback;
        self.ptr = (self.ptr + 1) % MAX_DELAY_SAMPLES;
        delayed
    }
}

// ─── Full DSP state ───────────────────────────────────────────────────────────

pub struct DspState {
    // Voices
    bass: Bass303,
    kick808: Kick,
    snare808: Snare,
    hihat_closed808: HiHat,
    hihat_open808: HiHat,
    tom_hi808: Kick,
    tom_mid808: Kick,
    tom_lo808: Kick,
    kick909: Kick,
    snare909: Snare,
    hihat_closed909: HiHat,
    hihat_open909: HiHat,
    clap909: Clap,
    rim909: Snare,
    // FX
    reverb: Reverb,
    delay: DelayLine,
    bitcrush_held: f32,
    bitcrush_counter: u32,
    // Current params
    params: AudioParams,
    sample_rate: f32,
}

impl DspState {
    pub fn new(sample_rate: f32, params: AudioParams) -> Self {
        let mut p = params;
        p.sample_rate = sample_rate;
        Self {
            bass: Bass303::default(),
            kick808: Kick::new(0x1234),
            snare808: Snare::new(0x5678),
            hihat_closed808: HiHat::new(0x9abc),
            hihat_open808: HiHat::new(0xdef0),
            tom_hi808: Kick::new(0x1111),
            tom_mid808: Kick::new(0x2222),
            tom_lo808: Kick::new(0x3333),
            kick909: Kick::new(0xaaaa),
            snare909: Snare::new(0xbbbb),
            hihat_closed909: HiHat::new(0xcccc),
            hihat_open909: HiHat::new(0xdddd),
            clap909: Clap::new(0xeeee),
            rim909: Snare::new(0xffff),
            reverb: Reverb::new(),
            delay: DelayLine::new(),
            bitcrush_held: 0.0,
            bitcrush_counter: 0,
            params: p,
            sample_rate,
        }
    }

    pub fn update_params(&mut self, p: AudioParams) {
        let mut p = p;
        p.sample_rate = self.sample_rate;
        self.params = p;
    }

    pub fn handle_trigger(&mut self, event: &TriggerEvent) {
        use crate::sequencer::TriggerEvent::*;
        match event {
            DrumTrigger { voice, velocity: _ } => match voice {
                DrumVoice::Kick808 => self.kick808.trigger(),
                DrumVoice::Snare808 => self.snare808.trigger(),
                DrumVoice::HihatClosed808 => self.hihat_closed808.trigger(),
                DrumVoice::HihatOpen808 => self.hihat_open808.trigger(),
                DrumVoice::TomHi808 => self.tom_hi808.trigger(),
                DrumVoice::TomMid808 => self.tom_mid808.trigger(),
                DrumVoice::TomLo808 => self.tom_lo808.trigger(),
                DrumVoice::Kick909 => self.kick909.trigger(),
                DrumVoice::Snare909 => self.snare909.trigger(),
                DrumVoice::HihatClosed909 => self.hihat_closed909.trigger(),
                DrumVoice::HihatOpen909 => self.hihat_open909.trigger(),
                DrumVoice::Clap909 => self.clap909.trigger(),
                DrumVoice::Rim909 => self.rim909.trigger(),
            },
            BassTrigger {
                note,
                accent,
                slide,
                gate_samples: _,
            } => {
                self.bass.trigger(*note, *accent, *slide);
            }
            BassGateOff => self.bass.gate_off(),
        }
    }

    /// Process one buffer — pure computation, no I/O, no allocation.
    pub fn process_block(&mut self, output: &mut [f32], channels: usize) {
        let p = self.params;
        let sr = self.sample_rate;

        let delay_samples =
            (p.delay_time * sr * 1.0).clamp(1.0, MAX_DELAY_SAMPLES as f32 - 1.0) as usize;

        for frame in output.chunks_mut(channels) {
            // Mix all voices to mono
            let bass_out = self.bass.process(&p);

            let k808 = self.kick808.process(
                p.kick808_pitch,
                p.kick808_decay,
                p.kick808_punch,
                p.kick808_volume,
                p.kick808_pitch_env_depth,
                p.kick808_pitch_env_time,
                sr,
            );
            let s808 = self.snare808.process(
                p.snare808_tone,
                p.snare808_snappy,
                p.snare808_decay,
                p.snare808_volume,
                sr,
            );
            let hh808c =
                self.hihat_closed808
                    .process(p.hihat_closed808_decay, 0.8, p.hihat808_volume, sr);
            let hh808o =
                self.hihat_open808
                    .process(p.hihat_open808_decay, 0.75, p.hihat808_volume, sr);
            let th808 = self.tom_hi808.process(0.7, 0.4, 0.6, 0.7, 0.5, 0.2, sr);
            let tm808 = self.tom_mid808.process(0.5, 0.45, 0.6, 0.7, 0.5, 0.2, sr);
            let tl808 = self.tom_lo808.process(0.3, 0.5, 0.6, 0.7, 0.5, 0.2, sr);

            let k909 = self.kick909.process(
                p.kick909_pitch,
                p.kick909_decay,
                p.kick909_punch,
                p.kick909_volume,
                p.kick909_pitch_env_depth,
                p.kick909_pitch_env_time,
                sr,
            );
            let s909 = self.snare909.process(
                p.snare909_tone,
                p.snare909_snappy,
                p.snare909_decay,
                p.snare909_volume,
                sr,
            );
            let hh909c =
                self.hihat_closed909
                    .process(p.hihat_closed909_decay, 0.85, p.hihat909_volume, sr);
            let hh909o =
                self.hihat_open909
                    .process(p.hihat_open909_decay, 0.8, p.hihat909_volume, sr);
            let clap = self.clap909.process(p.clap909_decay, p.clap909_volume, sr);
            let rim = self.rim909.process(0.7, 0.3, 0.15, 0.75, sr);

            // Scale mix to prevent clipping — summing 14 voices without gain staging
            // causes hard clipping even with moderate individual volumes
            let dry = (bass_out
                + k808
                + s808
                + hh808c
                + hh808o
                + th808
                + tm808
                + tl808
                + k909
                + s909
                + hh909c
                + hh909o
                + clap
                + rim)
                * 0.65;

            // FX chain
            let reverb_wet = self.reverb.process(dry, p.reverb_size, p.reverb_damp);
            let reverbed = dry * (1.0 - p.reverb_mix) + reverb_wet * p.reverb_mix;

            let delay_wet = self
                .delay
                .process(reverbed, delay_samples, p.delay_feedback);
            let delayed = reverbed * (1.0 - p.delay_mix) + delay_wet * p.delay_mix;

            // Bitcrush (bit depth reduction + sample rate decimation)
            let delayed = if p.bitcrush_mix > 0.01 {
                // Sample rate decimation: hold sample for N frames
                let hold_frames = (1.0 + p.bitcrush_rate * 15.0) as u32;
                if self.bitcrush_counter == 0 {
                    // Bit depth reduction
                    let bits = (1.0 + p.bitcrush_bits * 15.0).round().max(1.0);
                    let scale = (1u32 << (bits as u32 - 1)) as f32;
                    self.bitcrush_held = (delayed * scale).round() / scale;
                    self.bitcrush_counter = hold_frames;
                } else {
                    self.bitcrush_counter -= 1;
                }
                let crushed = self.bitcrush_held;
                delayed * (1.0 - p.bitcrush_mix) + crushed * p.bitcrush_mix
            } else {
                delayed
            };

            // Master drive (soft clip)
            let driven = if p.distortion_drive > 0.01 {
                let drive = p.distortion_drive * 6.0 + 1.0;
                let dist = tanh(delayed * drive);
                delayed * (1.0 - p.distortion_mix) + dist * p.distortion_mix
            } else {
                delayed
            };

            let out = (driven * p.master_volume).clamp(-1.0, 1.0);

            for s in frame.iter_mut() {
                *s = out;
            }
        }
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

pub fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}
