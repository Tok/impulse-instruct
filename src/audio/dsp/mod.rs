// ─── audio/dsp/mod.rs ─────────────────────────────────────────────────────────
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

mod fx;
mod params;
mod voices;
use fx::*;
pub use params::AudioParams;
use voices::*;

use crate::sequencer::TriggerEvent;
use crate::state::{DrumVoice, FxPlan, FxStep, ModuleKind};

// ─── Fast tanh approximation (used by LadderFilter and Bass303) ───────────────

pub(crate) fn tanh(x: f32) -> f32 {
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

// ─── TB-303 voice ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Bass303 {
    phase: f32,              // oscillator phase 0-1 (voice 0)
    sub_phase: f32,          // sub-oscillator phase (one octave below)
    fm_phase: f32,           // FM modulator phase
    unison_phases: [f32; 6], // phases for voices 1–6 (supersaw)
    freq: f32,               // current freq Hz
    target_freq: f32,        // slide target
    amp_env: f32,            // VCA envelope
    filt_env: f32,           // filter envelope
    gate: bool,
    accent: bool,
    slide: bool,
    filter: LadderFilter,
    svf_low: f32,  // Chamberlin SVF low-pass state
    svf_band: f32, // Chamberlin SVF band-pass state
    noise_gen: NoiseGen,
}

impl Default for Bass303 {
    fn default() -> Self {
        Self {
            phase: 0.0,
            sub_phase: 0.0,
            fm_phase: 0.0,
            unison_phases: [0.0, 0.142, 0.285, 0.428, 0.571, 0.714], // spread across cycle
            freq: 110.0,
            target_freq: 110.0,
            amp_env: 0.0,
            filt_env: 0.0,
            gate: false,
            accent: false,
            slide: false,
            filter: LadderFilter::default(),
            svf_low: 0.0,
            svf_band: 0.0,
            noise_gen: NoiseGen::new(0xBAD_C0DE),
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

        // Slide: portamento toward target; time 0–1 → 10ms–500ms
        if self.slide {
            let slide_time = 0.01 + p.portamento_time_303 * 0.49;
            let slide_coeff = (-1.0 / (sr * slide_time)).exp();
            self.freq = self.freq + (self.target_freq - self.freq) * (1.0 - slide_coeff);
        }

        // Pitch modulation: LFO + oscillator detune (both in semitones)
        let freq_mod = 2.0f32.powf((p.lfo_pitch_mod_st + p.osc_detune_303) / 12.0);

        // FM modulator: sine wave at carrier × ratio; adds to carrier phase increment
        let fm_mod = if p.fm_depth_303 > 0.001 {
            let mod_ratio = 0.5 + p.fm_ratio_303 * 7.5; // 0.5–8.0
            self.fm_phase += self.freq * freq_mod * mod_ratio / sr;
            if self.fm_phase >= 1.0 {
                self.fm_phase -= 1.0;
            }
            (self.fm_phase * std::f32::consts::TAU).sin() * p.fm_depth_303
        } else {
            0.0
        };

        // Oscillator — FM shifts the phase increment each sample
        self.phase += self.freq * freq_mod * (1.0 + fm_mod) / sr;
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
                self.unison_phases[i] += self.freq * freq_mod * ratio / sr;
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
        self.sub_phase += self.freq * freq_mod * 0.5 / sr;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= 1.0;
        }
        let sub = (self.sub_phase * std::f32::consts::TAU).sin();
        let noise = self.noise_gen.next();
        let osc = osc + sub * p.sub_osc_level + noise * p.noise_mix_303;

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

        // Filter — LP uses Moog ladder, HP/BP use Chamberlin SVF
        let filtered = if p.filter_mode == 0 {
            self.filter.process(osc, g, p.resonance * 0.97)
        } else {
            // Chamberlin State Variable Filter
            // f = 2*sin(pi*fc/sr), clamped to avoid instability
            let f = (std::f32::consts::PI * cutoff_hz / sr).sin().min(0.95);
            let q = 1.0 - p.resonance * 0.95; // q=1 = no resonance, q≈0 = self-oscillation
            self.svf_low += f * self.svf_band;
            let high = osc - self.svf_low - q * self.svf_band;
            self.svf_band += f * high;
            if p.filter_mode == 1 {
                high
            } else {
                self.svf_band
            } // 1=HP, 2=BP
        };

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
    chorus: Chorus,
    phaser: Phaser,
    bitcrush_held: f32,
    bitcrush_counter: u32,
    // FX state
    compressor: Compressor,
    tape_sat: TapeSat,
    ring_mod_phase: f32,
    eq: EqBands,
    noise_voice: NoiseVoice,
    hoover: HooverVoice,
    an1x: An1xVoice,
    amen: AmenVoice,
    // LFO state
    lfo_phases: [f32; 4],
    lfo_sh_held: [f32; 4],
    lfo_noise: NoiseGen,
    free_eg_phase: f32, // 0..1 through the 8-step EG period
    free_eg_done: bool, // true after one-shot completes
    prev_running: bool,
    // Per-voice velocity (set on trigger, applied to voice output)
    drum_velocity: [f32; 14],
    // Current params
    params: AudioParams,
    sample_rate: f32,
    // Compiled FX routing plan (updated via AudioCommand::SetFxPlan)
    fx_plan: FxPlan,
    // Gated reverb envelope: 1.0 = open, 0.0 = closed. Tracks transient
    // detection; decays to 0 at a rate set by p.reverb_gate_time.
    reverb_gate_env: f32,
    // TTS ring buffer consumer (lock-free; popped one sample per frame).
    tts_consumer: rtrb::Consumer<f32>,
    // Duck envelope: smoothly attenuates synth when TTS is active.
    tts_duck: f32,
    duck_attack: f32,
    duck_release: f32,
}

impl DspState {
    pub fn new(
        sample_rate: f32,
        params: AudioParams,
        fx_plan: FxPlan,
        tts_consumer: rtrb::Consumer<f32>,
    ) -> Self {
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
            chorus: Chorus::new(),
            phaser: Phaser::new(),
            compressor: Compressor::new(),
            tape_sat: TapeSat::new(),
            ring_mod_phase: 0.0,
            eq: EqBands::new(sample_rate),
            bitcrush_held: 0.0,
            bitcrush_counter: 0,
            noise_voice: NoiseVoice::new(0x4015_EB3D),
            hoover: HooverVoice::new(),
            an1x: An1xVoice::new(),
            amen: AmenVoice::new(),
            lfo_phases: [0.0; 4],
            lfo_sh_held: [0.0; 4],
            lfo_noise: NoiseGen::new(0xCAFE_BABE),
            free_eg_phase: 0.0,
            free_eg_done: false,
            drum_velocity: [1.0; 14],
            prev_running: false,
            params: p,
            sample_rate,
            fx_plan,
            reverb_gate_env: 1.0,
            tts_consumer,
            tts_duck: 1.0,
            duck_attack: 1.0 - (-8.0_f32 / sample_rate).exp(),
            duck_release: 1.0 - (-2.0_f32 / sample_rate).exp(),
        }
    }

    pub fn set_fx_plan(&mut self, plan: FxPlan) {
        self.fx_plan = plan;
    }

    /// Apply one FX step to `sig` and return the result.
    /// Must not hold any borrow on `self.fx_plan` when called.
    fn apply_fx_step(
        &mut self,
        step: FxStep,
        sig: f32,
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        match step {
            FxStep::Waveshaper => {
                if p.waveshaper_mix > 0.001 {
                    let drive = p.waveshaper_drive * 8.0 + 1.0;
                    let shaped = tanh(sig * drive) / tanh(drive);
                    sig * (1.0 - p.waveshaper_mix) + shaped * p.waveshaper_mix
                } else {
                    sig
                }
            }
            FxStep::Reverb => {
                if p.reverb_mix > 0.001 {
                    let wet = self.reverb.process(sig, p.reverb_size, p.reverb_damp);
                    sig * (1.0 - p.reverb_mix) + wet * p.reverb_mix * gate_env
                } else {
                    sig
                }
            }
            FxStep::Delay => {
                let wet = self.delay.process(sig, delay_samples, p.delay_feedback);
                sig * (1.0 - p.delay_mix) + wet * p.delay_mix
            }
            FxStep::Bitcrush => {
                if p.bitcrush_mix > 0.01 {
                    let hold_frames = (1.0 + p.bitcrush_rate * 15.0) as u32;
                    if self.bitcrush_counter == 0 {
                        let bits = (1.0 + p.bitcrush_bits * 15.0).round().max(1.0);
                        let scale = (1u32 << (bits as u32 - 1)) as f32;
                        self.bitcrush_held = (sig * scale).round() / scale;
                        self.bitcrush_counter = hold_frames;
                    } else {
                        self.bitcrush_counter -= 1;
                    }
                    sig * (1.0 - p.bitcrush_mix) + self.bitcrush_held * p.bitcrush_mix
                } else {
                    sig
                }
            }
            FxStep::Chorus => {
                self.chorus
                    .process(sig, p.chorus_rate, p.chorus_depth, p.chorus_mix, sr)
            }
            FxStep::Phaser => {
                self.phaser
                    .process(sig, p.phaser_rate, p.phaser_depth, p.phaser_mix, sr)
            }
            FxStep::RingMod => {
                if p.ring_mod_mix > 0.001 {
                    let freq_hz = 50.0 + p.ring_mod_freq * 450.0;
                    self.ring_mod_phase += freq_hz / sr;
                    if self.ring_mod_phase >= 1.0 {
                        self.ring_mod_phase -= 1.0;
                    }
                    let carrier = (self.ring_mod_phase * std::f32::consts::TAU).sin();
                    let ring = sig * carrier;
                    sig * (1.0 - p.ring_mod_mix) + ring * p.ring_mod_mix
                } else {
                    sig
                }
            }
            FxStep::Eq => self
                .eq
                .process(sig, p.eq_low_gain, p.eq_mid_gain, p.eq_hi_gain),
            FxStep::Compressor => self.compressor.process(
                sig,
                p.compressor_threshold,
                p.compressor_ratio,
                p.compressor_mix,
                sr,
            ),
            FxStep::TapeSat => {
                self.tape_sat
                    .process(sig, p.tape_drive, p.tape_mix, p.tape_flutter, sr)
            }
            FxStep::Drive => {
                if p.distortion_drive > 0.01 {
                    let drive = p.distortion_drive * 6.0 + 1.0;
                    let dist = tanh(sig * drive);
                    sig * (1.0 - p.distortion_mix) + dist * p.distortion_mix
                } else {
                    sig
                }
            }
        }
    }

    fn apply_fx_chain(
        &mut self,
        mut sig: f32,
        chain: &[FxStep],
        p: &AudioParams,
        delay_samples: usize,
        sr: f32,
        gate_env: f32,
    ) -> f32 {
        for &step in chain {
            sig = self.apply_fx_step(step, sig, p, delay_samples, sr, gate_env);
        }
        sig
    }

    pub fn update_params(&mut self, p: AudioParams) {
        let mut p = p;
        p.sample_rate = self.sample_rate;
        self.params = p;
    }

    /// Load new sample data into the amen voice. Called from the audio command handler
    /// (outside process_block) when the user picks a new WAV file.
    pub fn load_amen(&mut self, data: std::sync::Arc<Vec<f32>>) {
        self.amen.load(data);
    }

    pub fn handle_trigger(&mut self, event: &TriggerEvent) {
        use crate::sequencer::TriggerEvent::*;
        match event {
            DrumTrigger { voice, velocity } => {
                let in_rack = match voice {
                    DrumVoice::Kick808
                    | DrumVoice::Snare808
                    | DrumVoice::HihatClosed808
                    | DrumVoice::HihatOpen808
                    | DrumVoice::TomHi808
                    | DrumVoice::TomMid808
                    | DrumVoice::TomLo808 => self.params.rack_drums808,
                    DrumVoice::Kick909
                    | DrumVoice::Snare909
                    | DrumVoice::HihatClosed909
                    | DrumVoice::HihatOpen909
                    | DrumVoice::Clap909
                    | DrumVoice::Rim909 => self.params.rack_drums909,
                    DrumVoice::Amen => self.params.rack_amen,
                };
                if !in_rack {
                    return;
                }
                self.drum_velocity[voices::drum_voice_idx(voice)] = velocity.clamp(0.0, 1.0);
                match voice {
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
                    DrumVoice::Amen => self.amen.trigger(),
                }
            }
            BassTrigger {
                note,
                accent,
                slide,
                gate_samples: _,
            } => {
                if self.params.rack_bass {
                    self.bass.trigger(*note, *accent, *slide);
                }
            }
            BassGateOff => {
                if self.params.rack_bass {
                    self.bass.gate_off();
                }
            }
            HooverTrigger { note } => {
                if self.params.rack_hoover {
                    self.hoover.trigger(*note);
                }
            }
            HooverGateOff => {
                if self.params.rack_hoover {
                    self.hoover.gate_off();
                }
            }
            An1xTrigger { note } => {
                if self.params.rack_an1x {
                    self.an1x.trigger(*note, self.sample_rate, &self.params);
                }
            }
            An1xGateOff => {
                if self.params.rack_an1x {
                    self.an1x.gate_off();
                }
            }
        }
    }

    /// Process one buffer — pure computation, no I/O, no allocation.
    pub fn process_block(&mut self, output: &mut [f32], channels: usize) {
        let p_base = self.params;
        let sr = self.sample_rate;
        let block_size = output.len() / channels.max(1);

        // Phase reset: when sequencer transitions from stopped to running, reset all LFO phases
        if p_base.sequencer_running && !self.prev_running {
            self.lfo_phases = [0.0; 4];
            self.free_eg_phase = 0.0;
            self.free_eg_done = false;
        }
        self.prev_running = p_base.sequencer_running;

        // Advance LFO phases once per block and apply modulation to a working params copy
        let mut p = p_base;
        for i in 0..4 {
            let lp = p_base.lfo[i];
            if !lp.enabled {
                continue;
            }
            let rate_hz = 0.01 + lp.rate * 19.99;
            let phase_inc = rate_hz * (block_size as f32 / sr);
            let old_phase = self.lfo_phases[i];
            self.lfo_phases[i] = (old_phase + phase_inc) % 1.0;
            let wrapped = self.lfo_phases[i] < old_phase; // true when phase wrapped

            let phase = (self.lfo_phases[i] + lp.phase_offset) % 1.0;
            let lfo_val = match lp.waveform {
                0 => (phase * std::f32::consts::TAU).sin(),
                1 => 1.0 - 4.0 * (phase - 0.5).abs(),
                2 => phase * 2.0 - 1.0,
                3 => 1.0 - phase * 2.0,
                4 => {
                    if phase < 0.5 {
                        1.0
                    } else {
                        -1.0
                    }
                }
                _ => {
                    // S&H: update held value on phase wrap
                    if wrapped {
                        self.lfo_sh_held[i] = self.lfo_noise.next();
                    }
                    self.lfo_sh_held[i]
                }
            };

            let mod_val = lfo_val * lp.depth;
            match lp.target {
                1 => p.cutoff = (p.cutoff + mod_val).clamp(0.0, 1.0),
                2 => p.resonance = (p.resonance + mod_val).clamp(0.0, 1.0),
                3 => p.lfo_pitch_mod_st += mod_val * 12.0, // ±12 semitones at depth=1
                4 => p.volume_303 = (p.volume_303 + mod_val).clamp(0.0, 1.5),
                5 => p.reverb_mix = (p.reverb_mix + mod_val).clamp(0.0, 1.0),
                6 => p.delay_time = (p.delay_time + mod_val * 0.5).clamp(0.0, 1.0),
                7 => p.delay_feedback = (p.delay_feedback + mod_val * 0.5).clamp(0.0, 0.99),
                8 => p.chorus_mix = (p.chorus_mix + mod_val).clamp(0.0, 1.0),
                9 => p.chorus_rate = (p.chorus_rate + mod_val).clamp(0.0, 1.0),
                10 => p.kick808_pitch = (p.kick808_pitch + mod_val * 0.5).clamp(0.0, 1.0),
                _ => {}
            }
        }

        // ── Free EG ───────────────────────────────────────────────────────────
        if p_base.free_eg_enabled && p_base.free_eg_target != 0 && !self.free_eg_done {
            // Advance phase by one block
            let phase_inc = (block_size as f32 / sr) / p_base.free_eg_period.max(0.001);
            self.free_eg_phase += phase_inc;
            if self.free_eg_phase >= 1.0 {
                if p_base.free_eg_loop {
                    self.free_eg_phase -= 1.0;
                } else {
                    self.free_eg_phase = 1.0;
                    self.free_eg_done = true;
                }
            }
            // Interpolate between the 8 steps
            let pos = self.free_eg_phase * 7.0; // 0..7
            let idx = pos.floor() as usize;
            let frac = pos.fract();
            let v0 = p_base.free_eg_values[idx.min(7)];
            let v1 = p_base.free_eg_values[(idx + 1).min(7)];
            let level = v0 + (v1 - v0) * frac; // 0..1
            // Convert to bipolar mod: depth=0.5 → no mod; depth=1 → full positive; depth=0 → full negative
            let bipolar_depth = (p_base.free_eg_depth - 0.5) * 2.0; // -1..+1
            let mod_val = level * bipolar_depth;
            match p_base.free_eg_target {
                1 => p.cutoff = (p.cutoff + mod_val).clamp(0.0, 1.0),
                2 => p.resonance = (p.resonance + mod_val).clamp(0.0, 1.0),
                3 => p.lfo_pitch_mod_st += mod_val * 12.0,
                4 => p.volume_303 = (p.volume_303 + mod_val).clamp(0.0, 1.5),
                5 => p.reverb_mix = (p.reverb_mix + mod_val).clamp(0.0, 1.0),
                6 => p.delay_time = (p.delay_time + mod_val * 0.5).clamp(0.0, 1.0),
                7 => p.delay_feedback = (p.delay_feedback + mod_val * 0.5).clamp(0.0, 0.99),
                8 => p.chorus_mix = (p.chorus_mix + mod_val).clamp(0.0, 1.0),
                9 => p.chorus_rate = (p.chorus_rate + mod_val).clamp(0.0, 1.0),
                10 => p.kick808_pitch = (p.kick808_pitch + mod_val * 0.5).clamp(0.0, 1.0),
                _ => {}
            }
        }

        // Apply master pitch offset to melodic voices via lfo_pitch_mod_st accumulator.
        if p.master_pitch_st.abs() > 0.001 {
            p.lfo_pitch_mod_st += p.master_pitch_st;
        }

        let delay_samples =
            (p.delay_time * sr * 1.0).clamp(1.0, MAX_DELAY_SAMPLES as f32 - 1.0) as usize;

        // Snapshot FX chains into stack arrays before the per-frame loop.
        // This releases the immutable borrow on self.fx_plan so that apply_fx_step
        // (&mut self) can be called without a borrow conflict.
        const MAX_CHAIN: usize = 16;
        let mut global_chain = [FxStep::Waveshaper; MAX_CHAIN];
        let mut global_len = 0usize;
        for (i, &s) in self.fx_plan.steps.iter().enumerate().take(MAX_CHAIN) {
            global_chain[i] = s;
            global_len += 1;
        }

        // Per-voice route snapshots — copy from HashMap (all Copy, no allocation).
        let snap_route = |kind: ModuleKind| -> ([FxStep; MAX_CHAIN], usize) {
            let mut arr = [FxStep::Waveshaper; MAX_CHAIN];
            let mut len = 0usize;
            if let Some(steps) = self.fx_plan.voice_routes.get(&kind) {
                for (i, &s) in steps.iter().enumerate().take(MAX_CHAIN) {
                    arr[i] = s;
                    len += 1;
                }
            }
            (arr, len)
        };
        let (chain_bass, bass_len) = snap_route(ModuleKind::AcidBass);
        let (chain_808, d808_len) = snap_route(ModuleKind::DrumKit808);
        let (chain_909, d909_len) = snap_route(ModuleKind::DrumKit909);
        let (chain_hoover, hov_len) = snap_route(ModuleKind::HooverLead);
        let (chain_an1x, an1x_len) = snap_route(ModuleKind::An1xVoice);
        let (chain_amen, amen_len) = snap_route(ModuleKind::AmenSampler);
        let (chain_noise, noise_len) = snap_route(ModuleKind::NoiseVoice);
        let (chain_tts, tts_len) = snap_route(ModuleKind::EspeakNgTts);
        let have_voice_routes = !self.fx_plan.voice_routes.is_empty();
        // Release the immutable borrow on self (via fx_plan) before the mutable frame loop.
        let _ = snap_route;

        for frame in output.chunks_mut(channels) {
            // Mix all voices to mono
            let bass_out = self.bass.process(&p);

            let dv = &self.drum_velocity;
            let k808 = self.kick808.process(
                p.kick808_pitch,
                p.kick808_decay,
                p.kick808_punch,
                p.kick808_volume,
                p.kick808_pitch_env_depth,
                p.kick808_pitch_env_time,
                p.kick808_clip,
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
            let th808 = self
                .tom_hi808
                .process(0.7, 0.4, 0.6, 0.7, 0.5, 0.2, 0.0, sr);
            let tm808 = self
                .tom_mid808
                .process(0.5, 0.45, 0.6, 0.7, 0.5, 0.2, 0.0, sr);
            let tl808 = self
                .tom_lo808
                .process(0.3, 0.5, 0.6, 0.7, 0.5, 0.2, 0.0, sr);

            let k909 = self.kick909.process(
                p.kick909_pitch,
                p.kick909_decay,
                p.kick909_punch,
                p.kick909_volume,
                p.kick909_pitch_env_depth,
                p.kick909_pitch_env_time,
                p.kick909_clip,
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
            let noise_out = if p.noise_voice_enabled {
                self.noise_voice.process(
                    p.noise_voice_volume,
                    p.noise_voice_color,
                    p.noise_voice_cutoff,
                    sr,
                )
            } else {
                0.0
            };
            let hoover_out = if p.hoover_enabled {
                self.hoover.process(sr, &p)
            } else {
                0.0
            };
            let an1x_out = if p.an1x_enabled {
                self.an1x.process(sr, &p)
            } else {
                0.0
            };
            let amen_out = self.amen.process(p.amen_pitch, p.amen_volume, p.amen_loop);

            // Per-voice bus sums (scaled to prevent clipping)
            let bus_bass = bass_out;
            let bus_808 = k808 * dv[0]
                + s808 * dv[1]
                + hh808c * dv[2]
                + hh808o * dv[3]
                + th808 * dv[4]
                + tm808 * dv[5]
                + tl808 * dv[6];
            let bus_909 = k909 * dv[7]
                + s909 * dv[8]
                + hh909c * dv[9]
                + hh909o * dv[10]
                + clap * dv[11]
                + rim * dv[12];
            let bus_hoover = hoover_out;
            let bus_an1x = an1x_out;
            let bus_amen = amen_out * dv[13];
            let bus_noise = noise_out;

            // Gated reverb: detect transient from pre-FX dry signal.
            // When gate_time > 0, re-open gate on transients; close exponentially.
            let detection_sum =
                (bus_bass + bus_808 + bus_909 + bus_hoover + bus_an1x + bus_amen + bus_noise)
                    * 0.60;
            if p.reverb_gate_time > 0.001 {
                if detection_sum.abs() > 0.08 {
                    self.reverb_gate_env = 1.0;
                } else {
                    let decay = (-1.0 / (p.reverb_gate_time * sr)).exp();
                    self.reverb_gate_env *= decay;
                }
            } else {
                self.reverb_gate_env = 1.0;
            }
            let gate_env = self.reverb_gate_env;

            // Route voices through FX chains and sum to output.
            // Fast path: no voice routes → apply global chain to full dry mix (unchanged behaviour).
            // Per-voice path: each bus through its own chain, then sum, then global chain.
            let synth_out = if !have_voice_routes {
                let dry = detection_sum; // already scaled 0.60 above
                self.apply_fx_chain(
                    dry,
                    &global_chain[..global_len],
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            } else {
                let routed_bass = if bass_len > 0 {
                    self.apply_fx_chain(
                        bus_bass,
                        &chain_bass[..bass_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_bass
                };
                let routed_808 = if d808_len > 0 {
                    self.apply_fx_chain(
                        bus_808,
                        &chain_808[..d808_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_808
                };
                let routed_909 = if d909_len > 0 {
                    self.apply_fx_chain(
                        bus_909,
                        &chain_909[..d909_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_909
                };
                let routed_hoover = if hov_len > 0 {
                    self.apply_fx_chain(
                        bus_hoover,
                        &chain_hoover[..hov_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_hoover
                };
                let routed_an1x = if an1x_len > 0 {
                    self.apply_fx_chain(
                        bus_an1x,
                        &chain_an1x[..an1x_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_an1x
                };
                let routed_amen = if amen_len > 0 {
                    self.apply_fx_chain(
                        bus_amen,
                        &chain_amen[..amen_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_amen
                };
                let routed_noise = if noise_len > 0 {
                    self.apply_fx_chain(
                        bus_noise,
                        &chain_noise[..noise_len],
                        &p,
                        delay_samples,
                        sr,
                        gate_env,
                    )
                } else {
                    bus_noise
                };
                let mixed = (routed_bass
                    + routed_808
                    + routed_909
                    + routed_hoover
                    + routed_an1x
                    + routed_amen
                    + routed_noise)
                    * 0.60;
                // Global chain after per-voice mixing
                self.apply_fx_chain(
                    mixed,
                    &global_chain[..global_len],
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            };

            // TTS bus: pop one sample, apply TTS voice chain, duck synth and mix.
            let tts_raw = self.tts_consumer.pop().unwrap_or(0.0);
            let tts_sig = if tts_raw != 0.0 && tts_len > 0 {
                self.apply_fx_chain(
                    tts_raw,
                    &chain_tts[..tts_len],
                    &p,
                    delay_samples,
                    sr,
                    gate_env,
                )
            } else {
                tts_raw
            };
            // Smooth duck envelope: attenuate synth when TTS active
            let tts_active = tts_raw != 0.0 || self.tts_consumer.slots() > 0;
            let duck_target = if tts_active { 0.35_f32 } else { 1.0 };
            let coeff = if self.tts_duck > duck_target {
                self.duck_attack
            } else {
                self.duck_release
            };
            self.tts_duck += (duck_target - self.tts_duck) * coeff;

            let out = ((synth_out * self.tts_duck + tts_sig) * p.master_volume).clamp(-1.0, 1.0);

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
