// ─── audio/dsp/additive.rs ────────────────────────────────────────────────────
// Additive synth voice — 16 sine partials at integer multiples of
// the played fundamental, summed with per-harmonic levels and
// shaped by a single voice-wide ADSR.  Distinct from the
// Wavetable voice in shape: wavetable scans pre-baked frames;
// additive lets the user draw the spectrum directly via per-
// partial level sliders on the panel.
//
// Per-frame cost: 16 phase increments + 16 sine evaluations + 1
// ADSR step + 16-element weighted sum.  Allocation-free in
// `process()`.  Output normalised by the sum of the levels so a
// fully-pegged spectrum stays bounded.

use super::AudioParams;
use super::dsp_util::{ATTACK_HANDOVER_VALUE, RELEASE_OFF_VALUE, SUSTAIN_REACH_THRESHOLD};
use crate::state::ADDITIVE_HARMONICS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdsrStage {
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Debug)]
struct AdsrState {
    stage: AdsrStage,
    value: f32,
}

impl AdsrState {
    const fn new() -> Self {
        Self {
            stage: AdsrStage::Off,
            value: 0.0,
        }
    }

    fn trigger(&mut self) {
        self.stage = AdsrStage::Attack;
    }

    fn release(&mut self) {
        if self.stage != AdsrStage::Off {
            self.stage = AdsrStage::Release;
        }
    }

    fn step(&mut self, attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) {
        // Same knob → seconds map as the FM-ops / SAMPLER+ ADSRs
        // so the user gets consistent feel across all voices.
        let knob_to_secs =
            |k: f32, lo: f32, hi: f32| -> f32 { (lo + (hi - lo) * k.clamp(0.0, 1.0)).max(0.0005) };
        match self.stage {
            AdsrStage::Off => self.value = 0.0,
            AdsrStage::Attack => {
                let t = knob_to_secs(attack, 0.0005, 1.5);
                let coef = (-1.0_f32 / (t * sr)).exp();
                self.value = 1.0 - (1.0 - self.value) * coef;
                if self.value >= ATTACK_HANDOVER_VALUE {
                    self.value = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                let t = knob_to_secs(decay, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                let target = sustain.clamp(0.0, 1.0);
                self.value = target + (self.value - target) * coef;
                if (self.value - target).abs() < SUSTAIN_REACH_THRESHOLD {
                    self.value = target;
                    self.stage = AdsrStage::Sustain;
                }
            }
            AdsrStage::Sustain => {
                let target = sustain.clamp(0.0, 1.0);
                self.value += (target - self.value) * 0.001;
            }
            AdsrStage::Release => {
                let t = knob_to_secs(release, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                self.value *= coef;
                if self.value < RELEASE_OFF_VALUE {
                    self.value = 0.0;
                    self.stage = AdsrStage::Off;
                }
            }
        }
    }
}

pub struct AdditiveVoice {
    /// Per-partial phase accumulator (radians 0..2π).  Index 0 =
    /// fundamental, index N = (N+1)th harmonic.
    phases: [f32; ADDITIVE_HARMONICS],
    env: AdsrState,
    /// Fundamental frequency (Hz) — set on `trigger`.  Each
    /// partial's instantaneous frequency is `(i+1) * base_freq`.
    base_freq: f32,
    velocity: f32,
}

impl AdditiveVoice {
    pub fn new() -> Self {
        Self {
            phases: [0.0; ADDITIVE_HARMONICS],
            env: AdsrState::new(),
            base_freq: 261.625_56,
            velocity: 1.0,
        }
    }

    pub fn trigger(&mut self, freq_hz: f32, velocity: f32) {
        self.base_freq = freq_hz.clamp(20.0, 8_000.0);
        self.velocity = velocity.clamp(0.0, 1.5);
        self.env.trigger();
    }

    pub fn gate_off(&mut self) {
        self.env.release();
    }

    /// One-sample process.  Returns mono — process_block applies
    /// pan + voice volume at the master mix stage like every other
    /// voice.
    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.additive_enabled {
            return 0.0;
        }
        self.env.step(
            p.additive_attack,
            p.additive_decay,
            p.additive_sustain,
            p.additive_release,
            sr,
        );
        let env_gain = self.env.value;
        if env_gain < 1e-6 {
            return 0.0;
        }

        let two_pi = std::f32::consts::TAU;
        let dt = 1.0 / sr;
        // Sum of levels for normalisation — keeps a fully-pegged
        // spectrum bounded at ~1.0 amplitude before envelope.
        // Add 1e-3 so an all-zero spectrum doesn't divide by zero.
        let sum: f32 = p.additive_levels.iter().sum::<f32>().max(1e-3);
        let mut acc = 0.0_f32;
        for (i, level) in p.additive_levels.iter().enumerate() {
            if *level < 1e-4 {
                // Still advance the phase — keeps partials in
                // sync if the user fades a slider in mid-note.
                let h = (i + 1) as f32;
                let freq = (self.base_freq * h).clamp(0.05, sr * 0.45);
                self.phases[i] = (self.phases[i] + freq * two_pi * dt) % two_pi;
                continue;
            }
            let h = (i + 1) as f32;
            let freq = (self.base_freq * h).clamp(0.05, sr * 0.45);
            self.phases[i] = (self.phases[i] + freq * two_pi * dt) % two_pi;
            acc += self.phases[i].sin() * level;
        }
        acc *= env_gain * self.velocity * p.additive_volume.clamp(0.0, 1.5) / sum;
        acc
    }

    /// True when the envelope is past Off — used by tests + the
    /// (future) panel meter.
    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.env.stage != AdsrStage::Off
    }
}

impl Default for AdditiveVoice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn enabled_params() -> AudioParams {
        let mut s = AppState::default();
        s.additive.enabled = true;
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn silent_before_trigger() {
        let mut v = AdditiveVoice::new();
        let p = enabled_params();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn silent_when_disabled() {
        let mut v = AdditiveVoice::new();
        let s = AppState::default(); // enabled = false
        let p = AudioParams::from_app_state(&s);
        v.trigger(440.0, 1.0);
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn trigger_produces_audible_output() {
        let mut v = AdditiveVoice::new();
        let p = enabled_params();
        v.trigger(440.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..2_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak > 0.05, "audible output (peak {peak})");
    }

    #[test]
    fn output_bounded_under_pegged_spectrum() {
        // Every harmonic at full level — output should stay
        // bounded thanks to the normalisation by the level sum.
        let mut s = AppState::default();
        s.additive.enabled = true;
        s.additive.levels = [1.0; ADDITIVE_HARMONICS];
        s.additive.volume = 1.5;
        s.additive.attack = 0.0;
        s.additive.decay = 0.0;
        s.additive.sustain = 1.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = AdditiveVoice::new();
        v.trigger(220.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..16_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Normalised sum × envelope × volume(=1.5) — bound at ~2.
        // Catches any regression that drops the normalisation.
        assert!(peak <= 2.5, "fully-pegged spectrum bounded (peak {peak})");
    }

    #[test]
    fn release_eventually_silences() {
        let mut s = AppState::default();
        s.additive.enabled = true;
        s.additive.attack = 0.0;
        s.additive.decay = 0.0;
        s.additive.sustain = 0.5;
        s.additive.release = 0.0; // ~5 ms time constant
        let p = AudioParams::from_app_state(&s);
        let mut v = AdditiveVoice::new();
        v.trigger(440.0, 1.0);
        for _ in 0..1_000 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        for _ in 0..192_000 {
            let _ = v.process(48_000.0, &p);
        }
        assert!(!v.is_active(), "envelope should reach Off after release");
    }

    #[test]
    fn higher_partials_at_correct_frequencies() {
        // Drive only the 4th harmonic (index 3) and confirm the
        // output peaks at 4× the played fundamental.  The peak
        // count over a fixed window is a cheap proxy for frequency
        // since we don't have an FFT here.
        let mut s = AppState::default();
        s.additive.enabled = true;
        s.additive.levels = [0.0; ADDITIVE_HARMONICS];
        s.additive.levels[3] = 1.0; // only the 4th partial
        s.additive.attack = 0.0;
        s.additive.decay = 0.0;
        s.additive.sustain = 1.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = AdditiveVoice::new();
        v.trigger(100.0, 1.0); // expect output at 400 Hz
        let sr = 48_000.0;

        // Warm up.
        for _ in 0..2_000 {
            let _ = v.process(sr, &p);
        }
        // Count zero-crossings across 1 second.  At 400 Hz that's
        // 800 crossings.  Allow ±3 % tolerance for envelope tail.
        let mut last_sign = 0_i32;
        let mut crossings = 0_i32;
        for _ in 0..(sr as usize) {
            let out = v.process(sr, &p);
            let s = if out > 0.0 {
                1
            } else if out < 0.0 {
                -1
            } else {
                last_sign
            };
            if s != last_sign && last_sign != 0 {
                crossings += 1;
            }
            last_sign = s;
        }
        let observed_hz = crossings as f32 / 2.0; // pos+neg pair = 1 cycle
        assert!(
            (observed_hz - 400.0).abs() < 12.0,
            "4th-harmonic-only should oscillate at 400 Hz, observed {observed_hz}"
        );
    }
}
