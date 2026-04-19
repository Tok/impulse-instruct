// ─── audio/dsp/an1x.rs ───────────────────────────────────────────────────────
// AN1X-style analog-modelled synth voice: dual-osc + sub + ring-mod,
// Chamberlin SVF with LP / HP / BP modes and self-oscillation at
// high resonance, three ADSRs (amp / filter / pitch), LFO with delayed
// fade-in and per-target routing, per-voice pitch drift.
//
// Lifted out of voices.rs so that file stays under the 1000-line cap.
// The ADSR primitives + `osc_sample` still live in `voices`; this file
// just consumes them.

use super::voices::{AdsrPhase, adsr_tick, osc_sample};

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
        let pitch_st =
            self.current_pitch + drift_st + pitch_lfo_st + pitch_env_st + p.an1x_pitch_mod_st;
        let base_freq = super::dsp_util::midi_to_hz_tuned(pitch_st.round() as u8, p.tuning)
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
