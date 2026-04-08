// ─── TB-303 voice ─────────────────────────────────────────────────────────────
// Extracted from mod.rs to stay under the 1000-line limit.

use super::AudioParams;
use super::dsp_util::{midi_to_hz, tanh};
use super::params;
use super::voices::{LadderFilter, NoiseGen};

#[derive(Clone)]
pub(super) struct Bass303 {
    phase: f32,
    sub_phase: f32,
    fm_phase: f32,
    unison_phases: [f32; 6],
    freq: f32,
    target_freq: f32,
    amp_env: f32,
    filt_env: f32,
    gate: bool,
    accent: bool,
    slide: bool,
    filter: LadderFilter,
    svf_low: f32,
    svf_band: f32,
    noise_gen: NoiseGen,
}

impl Default for Bass303 {
    fn default() -> Self {
        Self {
            phase: 0.0,
            sub_phase: 0.0,
            fm_phase: 0.0,
            unison_phases: [0.0, 0.142, 0.285, 0.428, 0.571, 0.714],
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
    pub(super) fn trigger(&mut self, note: u8, accent: bool, slide: bool) {
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

    pub(super) fn gate_off(&mut self) {
        self.gate = false;
    }

    pub(super) fn process(&mut self, p: &AudioParams, vp: &params::BassVoiceParams) -> f32 {
        let sr = p.sample_rate;

        if self.slide {
            let slide_time = 0.01 + vp.portamento_time * 0.49;
            let slide_coeff = (-1.0 / (sr * slide_time)).exp();
            self.freq = self.freq + (self.target_freq - self.freq) * (1.0 - slide_coeff);
        }

        let freq_mod = 2.0f32.powf((p.lfo_pitch_mod_st + vp.osc_detune) / 12.0);

        let fm_mod = if vp.fm_depth > 0.001 {
            let mod_ratio = 0.5 + vp.fm_ratio * 7.5;
            self.fm_phase += self.freq * freq_mod * mod_ratio / sr;
            if self.fm_phase >= 1.0 {
                self.fm_phase -= 1.0;
            }
            (self.fm_phase * std::f32::consts::TAU).sin() * vp.fm_depth
        } else {
            0.0
        };

        self.phase += self.freq * freq_mod * (1.0 + fm_mod) / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let osc = if vp.waveform_supersaw {
            let n = vp.supersaw_voices.clamp(2, 7) as usize;
            let spread_semitones = vp.supersaw_detune;
            let mut sum = self.phase * 2.0 - 1.0;
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
            (sum / n as f32) * 1.4
        } else if vp.waveform_saw {
            self.phase * 2.0 - 1.0
        } else {
            if self.phase < 0.5 { 1.0 } else { -1.0 }
        };
        self.sub_phase += self.freq * freq_mod * 0.5 / sr;
        if self.sub_phase >= 1.0 {
            self.sub_phase -= 1.0;
        }
        let sub = (self.sub_phase * std::f32::consts::TAU).sin();
        let noise = self.noise_gen.next();
        let osc = osc + sub * vp.sub_osc_level + noise * vp.noise_mix;

        let accent_mult = if self.accent {
            vp.accent_level * 0.4 + 0.8
        } else {
            1.0
        };
        let env_decay_coeff = {
            let t = vp.decay * 4.9 + 0.1;
            (-1.0 / (sr * t)).exp()
        };

        if !self.gate {
            self.amp_env *= 0.9995;
        }
        self.filt_env *= env_decay_coeff;

        let cutoff_env = (vp.cutoff + self.filt_env * vp.env_mod * accent_mult).clamp(0.0, 1.0);
        let cutoff_hz = 200.0 * (40.0f32).powf(cutoff_env);
        let g = {
            let w = cutoff_hz / sr;
            (w / (1.0 + w)).clamp(0.001, 0.99)
        };

        let filtered = if vp.filter_mode == 0 {
            self.filter.process(osc, g, vp.resonance * 0.97)
        } else {
            let f = (std::f32::consts::PI * cutoff_hz / sr).sin().min(0.95);
            let q = 1.0 - vp.resonance * 0.95;
            self.svf_low += f * self.svf_band;
            let high = osc - self.svf_low - q * self.svf_band;
            self.svf_band += f * high;
            if vp.filter_mode == 1 {
                high
            } else {
                self.svf_band
            }
        };

        let dist = if vp.distortion > 0.01 {
            let drive = vp.distortion * 8.0 + 1.0;
            tanh(filtered * drive) / tanh(drive)
        } else {
            filtered
        };

        dist * self.amp_env * vp.volume * accent_mult
    }
}
