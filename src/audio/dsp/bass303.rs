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
    // Per-voice LFO state — phase advances every sample, fade counts up
    // from 0 on retrigger to honor lfo_delay.
    lfo_phase: f32,
    lfo_fade: f32, // 0..1 — 1 = full depth reached
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
            lfo_phase: 0.0,
            lfo_fade: 0.0,
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

/// Evaluate a unit LFO waveform at phase `p` (0..1).  Returns a value
/// in the range -1..+1.  `waveform` uses the numeric mirror from
/// BassVoiceParams (1=Sine, 2=Tri, 3=Saw, 4=InvSaw, 5=Square).
/// LFO rate in Hz, either tempo-synced or free.  Pulled out of `process`
/// so it's unit-testable in isolation.
pub(super) fn lfo_rate_hz(
    rate_norm: f32,
    bpm_sync: bool,
    sequencer_bpm: f32,
    sync_beats: f32,
) -> f32 {
    if bpm_sync {
        ((sequencer_bpm / 60.0) / sync_beats.max(0.03125)).clamp(0.01, 40.0)
    } else {
        (0.01 + rate_norm * 19.99).clamp(0.01, 40.0)
    }
}

/// One sample's worth of LFO fade-in ramp.  `lfo_delay` is the 0..1 knob
/// value (mapped to 0..4 seconds inside).  Delays under ~1 ms snap to
/// fully open instantly to avoid divide-by-near-zero noise.
pub(super) fn lfo_fade_step(prev_fade: f32, sample_rate: f32, lfo_delay: f32) -> f32 {
    let delay_s = lfo_delay * 4.0;
    if delay_s > 0.001 {
        (prev_fade + 1.0 / (sample_rate * delay_s)).min(1.0)
    } else {
        1.0
    }
}

pub(super) fn lfo_wave(p: f32, waveform: u8) -> f32 {
    let p = p.rem_euclid(1.0);
    match waveform {
        2 => 4.0 * (p - 0.5).abs() - 1.0, // triangle
        3 => 2.0 * p - 1.0,               // saw (up)
        4 => 1.0 - 2.0 * p,               // inv saw (down)
        5 => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        } // square
        _ => (p * std::f32::consts::TAU).sin(), // sine (default)
    }
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
            // Reset LFO fade-in so each new note honors the delay setting.
            self.lfo_fade = 0.0;
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

        // ── Per-voice LFO (SH-101 style) ────────────────────────────────────
        // Compute once per sample, route to the selected target further
        // down.  Rate is either bpm-synced (Hz = (host_bpm / 60) /
        // sync_beats) or free (linear 0.01..20 Hz mapping).  The fade-in
        // honors lfo_delay so each new note ramps up to full depth over
        // delay seconds (0 = instant).  lfo_target == 0 = Off = no-op.
        let lfo_value = if vp.lfo_target == 0 || vp.lfo_depth <= 0.0001 {
            0.0
        } else {
            let rate_hz = lfo_rate_hz(
                vp.lfo_rate,
                vp.lfo_bpm_sync,
                p.sequencer_bpm,
                vp.lfo_sync_beats,
            );
            self.lfo_phase = (self.lfo_phase + rate_hz / sr).rem_euclid(1.0);
            let raw = lfo_wave(self.lfo_phase, vp.lfo_waveform);
            self.lfo_fade = lfo_fade_step(self.lfo_fade, sr, vp.lfo_delay);
            raw * vp.lfo_depth * self.lfo_fade
        };
        // Compose each modulation contribution based on target.  Values
        // are chosen so full depth = audibly musical but not destructive.
        let lfo_pitch_st = if vp.lfo_target == 1 {
            lfo_value * 2.0
        } else {
            0.0
        };
        let lfo_pwm = if vp.lfo_target == 2 {
            lfo_value * 0.45
        } else {
            0.0
        };
        let lfo_cutoff = if vp.lfo_target == 3 {
            lfo_value * 0.5
        } else {
            0.0
        };
        let lfo_amp_mult = if vp.lfo_target == 4 {
            1.0 + lfo_value * 0.5
        } else {
            1.0
        };

        let freq_mod = 2.0f32.powf((p.lfo_pitch_mod_st + vp.osc_detune + lfo_pitch_st) / 12.0);

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
            // LFO PWM modulation (if routed) adds/subtracts around the
            // static pulse_width value.
            let pw = (vp.pulse_width + lfo_pwm).clamp(0.05, 0.95);
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

        let cutoff_env =
            (vp.cutoff + self.filt_env * vp.env_mod * accent_mult + lfo_cutoff).clamp(0.0, 1.0);
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

        dist * self.amp_env * vp.volume * accent_mult * lfo_amp_mult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lfo_rate_free_maps_normalised_to_hz() {
        // rate_norm == 0 → ~0.01 Hz floor; 1.0 → ~20 Hz top.
        assert!((lfo_rate_hz(0.0, false, 120.0, 1.0) - 0.01).abs() < 1e-3);
        assert!((lfo_rate_hz(1.0, false, 120.0, 1.0) - 20.0).abs() < 1e-3);
    }

    #[test]
    fn lfo_rate_bpm_sync_matches_expected_hz() {
        // 120 BPM, 1 beat sync → 2 Hz; 0.25 beats (16th) → 8 Hz; 4 beats → 0.5 Hz.
        assert!((lfo_rate_hz(0.0, true, 120.0, 1.0) - 2.0).abs() < 1e-4);
        assert!((lfo_rate_hz(0.0, true, 120.0, 0.25) - 8.0).abs() < 1e-4);
        assert!((lfo_rate_hz(0.0, true, 120.0, 4.0) - 0.5).abs() < 1e-4);
        // 174 BPM jungle, 1/16 sync.
        assert!((lfo_rate_hz(0.0, true, 174.0, 0.25) - (174.0 / 60.0 / 0.25)).abs() < 1e-3);
    }

    #[test]
    fn lfo_rate_clamped_to_safe_range() {
        // Pathological huge BPM with small sync: still clamped at 40 Hz.
        assert!(lfo_rate_hz(0.0, true, 9999.0, 0.03125) <= 40.0);
        // Tiny BPM: clamped at 0.01 Hz floor.
        assert!(lfo_rate_hz(0.0, true, 0.001, 1000.0) >= 0.01);
    }

    #[test]
    fn lfo_fade_zero_delay_snaps_full_open() {
        assert_eq!(lfo_fade_step(0.0, 44100.0, 0.0), 1.0);
        assert_eq!(lfo_fade_step(0.5, 44100.0, 0.0001), 1.0);
    }

    #[test]
    fn lfo_fade_with_delay_ramps_over_expected_duration() {
        // delay=0.25 → 1 second total fade.  After SR samples, fade ≈ 1.
        let sr = 44100.0;
        let delay = 0.25; // mapped to 1 s inside lfo_fade_step
        let mut fade = 0.0;
        for _ in 0..(sr as usize) {
            fade = lfo_fade_step(fade, sr, delay);
        }
        assert!(
            (fade - 1.0).abs() < 1e-3,
            "after 1 s expected fade≈1, got {fade}"
        );
        // Halfway through (SR/2 samples) we should be ~0.5.
        let mut fade = 0.0;
        for _ in 0..(sr as usize / 2) {
            fade = lfo_fade_step(fade, sr, delay);
        }
        assert!(
            (fade - 0.5).abs() < 1e-2,
            "at half-time expected ~0.5, got {fade}"
        );
    }

    #[test]
    fn lfo_wave_shapes_have_expected_endpoints() {
        // Sine: 0 at p=0, 0 at p=0.5; positive peak at 0.25.
        assert!((lfo_wave(0.0, 0)).abs() < 1e-5);
        assert!((lfo_wave(0.5, 0)).abs() < 1e-5);
        assert!(lfo_wave(0.25, 0) > 0.99);
        // Saw (3): -1 at p=0, +1 at p=1.
        assert!((lfo_wave(0.0, 3) + 1.0).abs() < 1e-5);
        assert!((lfo_wave(0.999, 3) - 0.998).abs() < 1e-2);
        // Inv saw (4): +1 at p=0.
        assert!((lfo_wave(0.0, 4) - 1.0).abs() < 1e-5);
        // Square (5): +1 first half, -1 second half.
        assert_eq!(lfo_wave(0.25, 5), 1.0);
        assert_eq!(lfo_wave(0.75, 5), -1.0);
        // Triangle (2): +1 at 0 (peak), -1 at 0.5 (valley).
        assert!((lfo_wave(0.0, 2) - 1.0).abs() < 1e-5);
        assert!((lfo_wave(0.5, 2) + 1.0).abs() < 1e-5);
    }
}
