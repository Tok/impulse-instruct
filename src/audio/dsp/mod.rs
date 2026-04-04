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
mod voices;
use fx::*;
use voices::*;

use crate::sequencer::TriggerEvent;
use crate::state::{
    An1xLfoTarget, An1xWave, AppState, DrumVoice, FilterMode, LfoTarget, LfoWaveform, Waveform,
};

// ─── AudioParams snapshot (copied from AppState for audio thread) ──────────────

/// Per-slot LFO configuration passed to the audio thread (Copy-safe).
#[derive(Clone, Copy, Debug)]
pub struct LfoParamsCopy {
    pub enabled: bool,
    pub waveform: u8,      // 0=Sine 1=Triangle 2=Saw 3=InvSaw 4=Square 5=S&H
    pub rate: f32,         // 0–1
    pub depth: f32,        // 0–1
    pub phase_offset: f32, // 0–1
    pub target: u8,        // 0=None 1=BassCutoff 2=BassResonance 3=BassPitch 4=BassVolume
                           // 5=ReverbMix 6=DelayTime 7=DelayFeedback 8=ChorusMix 9=ChorusRate 10=Kick808Pitch
}

#[derive(Clone, Copy, Debug)]
pub struct AudioParams {
    // 303
    pub cutoff: f32,
    pub resonance: f32,
    pub env_mod: f32,
    pub decay_303: f32,
    pub accent_level: f32,
    pub waveform_saw: bool,       // true = saw, false = square
    pub waveform_supersaw: bool,  // true = supersaw (overrides waveform_saw)
    pub supersaw_detune: f32,     // 0–1 → 0–1 semitone spread
    pub supersaw_voices: u8,      // 2–7
    pub sub_osc_level: f32,       // 0–1 sub-oscillator mix level
    pub portamento_time_303: f32, // 0–1 → 10ms–500ms
    pub noise_mix_303: f32,       // 0–1 white noise into osc before filter
    pub osc_detune_303: f32,      // semitone offset -1..+1
    pub fm_ratio_303: f32,        // 0–1 → modulator/carrier ratio 0.5–8.0
    pub fm_depth_303: f32,        // 0–1 FM depth; 0 = off
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
    // Chorus
    pub chorus_rate: f32,
    pub chorus_depth: f32,
    pub chorus_mix: f32,
    // Phaser
    pub phaser_rate: f32,
    pub phaser_depth: f32,
    pub phaser_mix: f32,
    // Waveshaper
    pub waveshaper_drive: f32,
    pub waveshaper_mix: f32,
    // Ring modulator
    pub ring_mod_freq: f32,
    pub ring_mod_mix: f32,
    // 3-band EQ
    pub eq_low_gain: f32,
    pub eq_mid_gain: f32,
    pub eq_hi_gain: f32,
    // Compressor
    pub compressor_threshold: f32,
    pub compressor_ratio: f32,
    pub compressor_mix: f32,
    // Tape saturation
    pub tape_drive: f32,
    pub tape_mix: f32,
    pub tape_flutter: f32,
    // Filter mode (0=LP, 1=HP, 2=BP)
    pub filter_mode: u8,
    // Sample rate
    pub sample_rate: f32,
    // LFO
    pub lfo: [LfoParamsCopy; 4],
    pub sequencer_running: bool,
    pub lfo_pitch_mod_st: f32,
    // Noise voice
    pub noise_voice_enabled: bool,
    pub noise_voice_volume: f32,
    pub noise_voice_color: f32,
    pub noise_voice_cutoff: f32,
    // Hoover lead
    pub hoover_enabled: bool,
    pub hoover_filter_start: f32,
    pub hoover_sweep_time: f32,
    pub hoover_resonance: f32,
    pub hoover_detune: f32,
    pub hoover_voices: u8,
    pub hoover_pitch_lfo_rate: f32,
    pub hoover_pitch_lfo_depth: f32,
    pub hoover_volume: f32,
    // AN1X voice
    pub an1x_enabled: bool,
    pub an1x_volume: f32,
    pub an1x_osc1_wave: u8, // 0=Saw 1=Square 2=Triangle 3=Sine 4=Noise
    pub an1x_osc1_level: f32,
    pub an1x_osc2_wave: u8,
    pub an1x_osc2_level: f32,
    pub an1x_osc2_detune: f32, // 0–1 (0.5 = unison)
    pub an1x_osc2_octave: i8,  // -2..+2
    pub an1x_sub_level: f32,
    pub an1x_ring_mod: bool,
    pub an1x_filter_cutoff: f32,
    pub an1x_filter_resonance: f32,
    pub an1x_filter_mode: u8, // 0=LP 1=HP 2=BP
    pub an1x_filter_key_track: f32,
    pub an1x_filter_env_amount: f32,
    pub an1x_filter_attack: f32,
    pub an1x_filter_decay: f32,
    pub an1x_filter_sustain: f32,
    pub an1x_filter_release: f32,
    pub an1x_amp_attack: f32,
    pub an1x_amp_decay: f32,
    pub an1x_amp_sustain: f32,
    pub an1x_amp_release: f32,
    pub an1x_lfo_rate_hz: f32, // Hz (0.01–20)
    pub an1x_lfo_depth: f32,   // 0–1
    pub an1x_lfo_target: u8,   // 0=Pitch 1=FilterCutoff 2=Amplitude
    pub an1x_lfo_delay: f32,   // 0–1 → 0–4s fade-in
    pub an1x_drift: f32,       // 0–1
    pub an1x_glide_time: f32,  // 0–1 → 0–500ms
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
            portamento_time_303: s.bass.portamento_time,
            noise_mix_303: s.bass.noise_mix,
            osc_detune_303: s.bass.osc_detune,
            fm_ratio_303: s.bass.fm_ratio,
            fm_depth_303: s.bass.fm_depth,
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
            chorus_rate: s.fx.chorus_rate,
            chorus_depth: s.fx.chorus_depth,
            chorus_mix: s.fx.chorus_mix,
            phaser_rate: s.fx.phaser_rate,
            phaser_depth: s.fx.phaser_depth,
            phaser_mix: s.fx.phaser_mix,
            waveshaper_drive: s.fx.waveshaper_drive,
            waveshaper_mix: s.fx.waveshaper_mix,
            ring_mod_freq: s.fx.ring_mod_freq,
            ring_mod_mix: s.fx.ring_mod_mix,
            eq_low_gain: s.fx.eq_low_gain,
            eq_mid_gain: s.fx.eq_mid_gain,
            eq_hi_gain: s.fx.eq_hi_gain,
            compressor_threshold: s.fx.compressor_threshold,
            compressor_ratio: s.fx.compressor_ratio,
            compressor_mix: s.fx.compressor_mix,
            tape_drive: s.fx.tape_drive,
            tape_mix: s.fx.tape_mix,
            tape_flutter: s.fx.tape_flutter,
            filter_mode: match s.bass.filter_mode {
                FilterMode::Lowpass => 0,
                FilterMode::Highpass => 1,
                FilterMode::Bandpass => 2,
            },
            sample_rate: 44100.0,
            lfo: {
                let mut arr = [LfoParamsCopy {
                    enabled: false,
                    waveform: 0,
                    rate: 0.2,
                    depth: 0.3,
                    phase_offset: 0.0,
                    target: 0,
                }; 4];
                for (i, slot) in s.lfo.iter().enumerate() {
                    arr[i] = LfoParamsCopy {
                        enabled: slot.enabled,
                        waveform: match slot.waveform {
                            LfoWaveform::Sine => 0,
                            LfoWaveform::Triangle => 1,
                            LfoWaveform::Saw => 2,
                            LfoWaveform::InvSaw => 3,
                            LfoWaveform::Square => 4,
                            LfoWaveform::SampleAndHold => 5,
                        },
                        rate: slot.rate,
                        depth: slot.depth,
                        phase_offset: slot.phase_offset,
                        target: match slot.target {
                            LfoTarget::None => 0,
                            LfoTarget::BassCutoff => 1,
                            LfoTarget::BassResonance => 2,
                            LfoTarget::BassPitch => 3,
                            LfoTarget::BassVolume => 4,
                            LfoTarget::ReverbMix => 5,
                            LfoTarget::DelayTime => 6,
                            LfoTarget::DelayFeedback => 7,
                            LfoTarget::ChorusMix => 8,
                            LfoTarget::ChorusRate => 9,
                            LfoTarget::Kick808Pitch => 10,
                        },
                    };
                }
                arr
            },
            sequencer_running: s.sequencer.running,
            lfo_pitch_mod_st: 0.0,
            noise_voice_enabled: s.noise_voice.enabled,
            noise_voice_volume: s.noise_voice.volume,
            noise_voice_color: s.noise_voice.color,
            noise_voice_cutoff: s.noise_voice.cutoff,
            hoover_enabled: s.hoover.enabled,
            hoover_filter_start: s.hoover.filter_start,
            hoover_sweep_time: s.hoover.sweep_time.clamp(0.1, 4.0),
            hoover_resonance: s.hoover.resonance,
            hoover_detune: s.hoover.detune,
            hoover_voices: s.hoover.voices,
            hoover_pitch_lfo_rate: s.hoover.pitch_lfo_rate,
            hoover_pitch_lfo_depth: s.hoover.pitch_lfo_depth,
            hoover_volume: s.hoover.volume,
            an1x_enabled: s.an1x.enabled,
            an1x_volume: s.an1x.volume,
            an1x_osc1_wave: match s.an1x.osc1_wave {
                An1xWave::Saw => 0,
                An1xWave::Square => 1,
                An1xWave::Triangle => 2,
                An1xWave::Sine => 3,
                An1xWave::Noise => 4,
            },
            an1x_osc1_level: s.an1x.osc1_level,
            an1x_osc2_wave: match s.an1x.osc2_wave {
                An1xWave::Saw => 0,
                An1xWave::Square => 1,
                An1xWave::Triangle => 2,
                An1xWave::Sine => 3,
                An1xWave::Noise => 4,
            },
            an1x_osc2_level: s.an1x.osc2_level,
            an1x_osc2_detune: s.an1x.osc2_detune,
            an1x_osc2_octave: s.an1x.osc2_octave,
            an1x_sub_level: s.an1x.sub_level,
            an1x_ring_mod: s.an1x.ring_mod,
            an1x_filter_cutoff: s.an1x.filter_cutoff,
            an1x_filter_resonance: s.an1x.filter_resonance,
            an1x_filter_mode: match s.an1x.filter_mode {
                FilterMode::Lowpass => 0,
                FilterMode::Highpass => 1,
                FilterMode::Bandpass => 2,
            },
            an1x_filter_key_track: s.an1x.filter_key_track,
            an1x_filter_env_amount: s.an1x.filter_env_amount,
            an1x_filter_attack: s.an1x.filter_attack,
            an1x_filter_decay: s.an1x.filter_decay,
            an1x_filter_sustain: s.an1x.filter_sustain,
            an1x_filter_release: s.an1x.filter_release,
            an1x_amp_attack: s.an1x.amp_attack,
            an1x_amp_decay: s.an1x.amp_decay,
            an1x_amp_sustain: s.an1x.amp_sustain,
            an1x_amp_release: s.an1x.amp_release,
            an1x_lfo_rate_hz: 0.01 + s.an1x.lfo_rate * s.an1x.lfo_rate * 19.99,
            an1x_lfo_depth: s.an1x.lfo_depth,
            an1x_lfo_target: match s.an1x.lfo_target {
                An1xLfoTarget::Pitch => 0,
                An1xLfoTarget::FilterCutoff => 1,
                An1xLfoTarget::Amplitude => 2,
            },
            an1x_lfo_delay: s.an1x.lfo_delay,
            an1x_drift: s.an1x.drift,
            an1x_glide_time: s.an1x.glide_time,
        }
    }
}

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
    // LFO state
    lfo_phases: [f32; 4],
    lfo_sh_held: [f32; 4],
    lfo_noise: NoiseGen,
    prev_running: bool,
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
            lfo_phases: [0.0; 4],
            lfo_sh_held: [0.0; 4],
            lfo_noise: NoiseGen::new(0xCAFE_BABE),
            prev_running: false,
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
            HooverTrigger { note } => self.hoover.trigger(*note),
            HooverGateOff => self.hoover.gate_off(),
            An1xTrigger { note } => self.an1x.trigger(*note, self.sample_rate, &self.params),
            An1xGateOff => self.an1x.gate_off(),
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

            // Scale mix to prevent clipping — summing voices without gain staging
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
                + rim
                + noise_out
                + hoover_out
                + an1x_out)
                * 0.60;

            // Waveshaper — pre-FX insert, adds harmonic saturation before time-based FX
            let dry = if p.waveshaper_mix > 0.001 {
                let drive = p.waveshaper_drive * 8.0 + 1.0;
                let shaped = tanh(dry * drive) / tanh(drive);
                dry * (1.0 - p.waveshaper_mix) + shaped * p.waveshaper_mix
            } else {
                dry
            };

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

            // Chorus / ensemble
            let delayed =
                self.chorus
                    .process(delayed, p.chorus_rate, p.chorus_depth, p.chorus_mix, sr);

            // Phaser
            let delayed =
                self.phaser
                    .process(delayed, p.phaser_rate, p.phaser_depth, p.phaser_mix, sr);

            // Ring modulator
            let delayed = if p.ring_mod_mix > 0.001 {
                let freq_hz = 50.0 + p.ring_mod_freq * 450.0;
                self.ring_mod_phase += freq_hz / sr;
                if self.ring_mod_phase >= 1.0 {
                    self.ring_mod_phase -= 1.0;
                }
                let carrier = (self.ring_mod_phase * std::f32::consts::TAU).sin();
                let ring = delayed * carrier;
                delayed * (1.0 - p.ring_mod_mix) + ring * p.ring_mod_mix
            } else {
                delayed
            };

            // 3-band EQ
            let delayed = self
                .eq
                .process(delayed, p.eq_low_gain, p.eq_mid_gain, p.eq_hi_gain);

            // Compressor
            let delayed = self.compressor.process(
                delayed,
                p.compressor_threshold,
                p.compressor_ratio,
                p.compressor_mix,
                sr,
            );

            // Tape saturation
            let delayed =
                self.tape_sat
                    .process(delayed, p.tape_drive, p.tape_mix, p.tape_flutter, sr);

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
