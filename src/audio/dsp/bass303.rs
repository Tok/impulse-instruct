// ─── TB-303 voice ─────────────────────────────────────────────────────────────
// Extracted from mod.rs to stay under the 1000-line limit.

use super::AudioParams;
use super::dsp_util::tanh;
use super::params;
use super::voices::{LadderFilter, NoiseGen};

/// ADSR envelope phase.  Attack rises to 1.0, decay falls to sustain,
/// sustain holds while gate is on, release falls to 0 after gate-off.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EnvPhase {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone)]
pub(super) struct Bass303 {
    phase: f32,
    sub_phase: f32,
    fm_phase: f32,
    unison_phases: [f32; 6],
    freq: f32,
    target_freq: f32,
    // Legacy amp/filt env kept for accent hold + legacy `decay` field.
    amp_env: f32,
    filt_env: f32,
    // New ADSR state — runs alongside the legacy values.  amp_env and
    // filt_env become the value; amp_phase/filt_phase advance through
    // Attack → Decay → Sustain → Release based on gate transitions.
    amp_phase: EnvPhase,
    filt_phase: EnvPhase,
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
            amp_phase: EnvPhase::Idle,
            filt_phase: EnvPhase::Idle,
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

/// Exponential smoothing coefficient for a time constant of `t` seconds
/// at sample rate `sr`.  Returns a value in (0, 1) such that
/// `new = old * coeff + target * (1 - coeff)` reaches `1 - 1/e` of the
/// distance to `target` over `t` seconds.  Panics are not possible — sr
/// and t are clamped by the caller.
fn env_coeff(sr: f32, t_seconds: f32) -> f32 {
    if t_seconds <= 0.0001 {
        return 0.0; // instant step to target
    }
    (-1.0 / (sr * t_seconds)).exp()
}

impl Bass303 {
    pub(super) fn trigger(&mut self, note: u8, accent: bool, slide: bool, tuning: u8) {
        let new_freq = super::dsp_util::midi_to_hz_tuned(note, tuning);
        if !self.slide {
            self.freq = new_freq;
        }
        self.target_freq = new_freq;
        self.accent = accent;
        self.slide = slide;
        self.gate = true;
        if !slide {
            // Envelopes restart their attack phase.  Actual starting value
            // comes from wherever they currently sit (release-in-progress
            // steals-from-silence behavior), so legato-ish retriggers
            // don't click.
            self.amp_phase = EnvPhase::Attack;
            self.filt_phase = EnvPhase::Attack;
            // Preserve legacy accent behavior: accent peaks higher.
            if self.amp_env < 0.01 {
                self.amp_env = 0.0;
            }
            if self.filt_env < 0.01 {
                self.filt_env = 0.0;
            }
        }
    }

    pub(super) fn gate_off(&mut self) {
        self.gate = false;
        self.amp_phase = EnvPhase::Release;
        self.filt_phase = EnvPhase::Release;
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
            // Pulse wave with variable width.  pulse_width = 0.5 is a
            // classic square; narrower = reedier, wider = less harmonic.
            let pw = vp.pulse_width.clamp(0.05, 0.95);
            if self.phase < pw { 1.0 } else { -1.0 }
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
        // Legacy filter-env decay time (used when filter_sustain == 0 — the
        // 303 behavior).  For 101 shaping, filter_attack / filter_sustain /
        // filter_release take over via the phase machine.
        let filt_decay_t = vp.decay * 4.9 + 0.1; // 100ms..5s
        let env_decay_coeff = env_coeff(sr, filt_decay_t);

        // ── Amp envelope state machine ───────────────────────────────────────
        let amp_attack_t = vp.amp_attack * 1.0; // 0..1s
        // Amp decay uses the legacy `decay` knob too — they share
        // meaning ("how fast it settles after the hit").
        let amp_decay_t = filt_decay_t;
        let amp_release_t = vp.amp_release * 2.0; // 0..2s
        let amp_attack_c = env_coeff(sr, amp_attack_t.max(0.001));
        let amp_decay_c = env_coeff(sr, amp_decay_t);
        let amp_release_c = env_coeff(sr, amp_release_t.max(0.001));
        // Accent lifts the amp peak target the same way the old code did.
        let amp_peak = if self.accent { 1.0 } else { 0.8 };
        match self.amp_phase {
            EnvPhase::Idle => {
                self.amp_env = 0.0;
            }
            EnvPhase::Attack => {
                self.amp_env = amp_peak - (amp_peak - self.amp_env) * amp_attack_c;
                if amp_attack_c <= 0.001 || self.amp_env >= amp_peak - 0.001 {
                    self.amp_env = amp_peak;
                    self.amp_phase = EnvPhase::Decay;
                }
            }
            EnvPhase::Decay => {
                let target = amp_peak * vp.amp_sustain;
                self.amp_env = target + (self.amp_env - target) * amp_decay_c;
                if amp_decay_c <= 0.001 || (self.amp_env - target).abs() < 0.001 {
                    self.amp_env = target;
                    self.amp_phase = EnvPhase::Sustain;
                }
            }
            EnvPhase::Sustain => {
                self.amp_env = amp_peak * vp.amp_sustain;
            }
            EnvPhase::Release => {
                self.amp_env *= amp_release_c;
                if self.amp_env < 0.001 {
                    self.amp_env = 0.0;
                    self.amp_phase = EnvPhase::Idle;
                }
            }
        }

        // ── Filter envelope state machine ────────────────────────────────────
        let filt_attack_t = vp.filter_attack * 0.5;
        let filt_release_t = vp.filter_release * 2.0;
        let filt_attack_c = env_coeff(sr, filt_attack_t.max(0.001));
        let filt_release_c = env_coeff(sr, filt_release_t.max(0.001));
        match self.filt_phase {
            EnvPhase::Idle => {
                self.filt_env = 0.0;
            }
            EnvPhase::Attack => {
                self.filt_env = 1.0 - (1.0 - self.filt_env) * filt_attack_c;
                if filt_attack_c <= 0.001 || self.filt_env >= 0.999 {
                    self.filt_env = 1.0;
                    self.filt_phase = EnvPhase::Decay;
                }
            }
            EnvPhase::Decay => {
                let target = vp.filter_sustain;
                self.filt_env = target + (self.filt_env - target) * env_decay_coeff;
                if env_decay_coeff <= 0.001 || (self.filt_env - target).abs() < 0.001 {
                    self.filt_env = target;
                    self.filt_phase = EnvPhase::Sustain;
                }
            }
            EnvPhase::Sustain => {
                self.filt_env = vp.filter_sustain;
            }
            EnvPhase::Release => {
                self.filt_env *= filt_release_c;
                if self.filt_env < 0.001 {
                    self.filt_env = 0.0;
                    self.filt_phase = EnvPhase::Idle;
                }
            }
        }

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
