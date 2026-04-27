// ─── audio/dsp/vocal.rs ───────────────────────────────────────────────────────
// Vocal formant synth — saw source through 3 parallel resonant
// bandpass biquads tuned to the formant frequencies of the
// played vowel.  Distinct from the `NeuTts` voice in shape: that
// one runs a neural phoneme model; this one is pure DSP — the
// vowel character comes entirely from the formant resonance
// pattern.
//
// Per-frame cost: 1 saw eval + 3 biquad taps + 1 tilt LP for
// brightness + 1 ADSR step.  Coefficients are recomputed lazily
// when the user moves the vowel / morph / formant_shift / sample-
// rate knobs (rare relative to the audio rate).  Allocation-free.

use super::AudioParams;
use super::dsp_util::{
    ATTACK_HANDOVER_VALUE, RELEASE_OFF_VALUE, SUSTAIN_REACH_THRESHOLD, nyquist_guard,
    one_pole_coef, one_pole_lp_alpha,
};

/// Vowel formant table — F1, F2, F3 frequencies (Hz) and per-
/// formant Q values.  Numbers from Peterson & Barney 1952 male
/// averages, the canonical reference for vowel-formant
/// synthesis.  Q values picked to give clear vowel articulation
/// without ringing into the next formant.
const VOWEL_FORMANTS: [[(f32, f32); 3]; 5] = [
    // Index 0 — A (as in "father")
    [(700.0, 8.0), (1220.0, 10.0), (2600.0, 12.0)],
    // Index 1 — E (as in "bee")
    [(270.0, 8.0), (2290.0, 10.0), (3010.0, 12.0)],
    // Index 2 — I (as in "sit")
    [(390.0, 8.0), (1990.0, 10.0), (2550.0, 12.0)],
    // Index 3 — O (as in "bought")
    [(570.0, 8.0), (840.0, 10.0), (2410.0, 12.0)],
    // Index 4 — U (as in "boot")
    [(440.0, 8.0), (1020.0, 10.0), (2240.0, 12.0)],
];

const FORMANT_COUNT: usize = 3;

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
        let knob_to_secs =
            |k: f32, lo: f32, hi: f32| -> f32 { (lo + (hi - lo) * k.clamp(0.0, 1.0)).max(0.0005) };
        match self.stage {
            AdsrStage::Off => self.value = 0.0,
            AdsrStage::Attack => {
                let t = knob_to_secs(attack, 0.0005, 1.5);
                let coef = one_pole_coef(t, sr);
                self.value = 1.0 - (1.0 - self.value) * coef;
                if self.value >= ATTACK_HANDOVER_VALUE {
                    self.value = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                let t = knob_to_secs(decay, 0.005, 2.0);
                let coef = one_pole_coef(t, sr);
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
                let coef = one_pole_coef(t, sr);
                self.value *= coef;
                if self.value < RELEASE_OFF_VALUE {
                    self.value = 0.0;
                    self.stage = AdsrStage::Off;
                }
            }
        }
    }
}

/// Two-pole resonant bandpass biquad.  Standard RBJ cookbook
/// formulation — kept inline rather than reusing the existing
/// `Svf` because each formant needs its own center frequency
/// and Q rather than the SVF's mode-flag dispatch.
#[derive(Clone, Copy, Debug)]
struct FormantBiquad {
    /// Feedforward coefficients.
    b0: f32,
    /// Feedforward coefficient for x[n-2] (b2 = -b0 in BPF
    /// constant-skirt-gain form so we don't store it).
    /// b1 = 0 for the BPF, omitted.
    /// Feedback coefficients.
    a1: f32,
    a2: f32,
    /// State.
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl FormantBiquad {
    const fn new() -> Self {
        Self {
            b0: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// RBJ "constant skirt gain, peak gain = Q" BPF — the standard
    /// vocal-formant biquad.  Formula:
    ///   b0 =  sin(ω)/2,  b1 = 0,  b2 = -b0
    ///   a0 =  1 + α,     a1 = -2 cos(ω),  a2 = 1 - α
    /// where α = sin(ω) / (2Q).
    fn set(&mut self, freq_hz: f32, q: f32, sr: f32) {
        let f = freq_hz.clamp(20.0, nyquist_guard(sr));
        let omega = std::f32::consts::TAU * f / sr;
        let q = q.max(0.5);
        let sin_w = omega.sin();
        let cos_w = omega.cos();
        let alpha = sin_w / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = (sin_w * 0.5) / a0;
        self.a1 = (-2.0 * cos_w) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn process(&mut self, x: f32) -> f32 {
        // y = b0·(x - x[n-2]) - a1·y[n-1] - a2·y[n-2]
        // (b1 = 0 and b2 = -b0 baked into the (x - x[n-2]) term.)
        let y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub struct VocalVoice {
    /// Saw oscillator phase 0..1.  Saw is the canonical vocal-
    /// formant source — broad harmonic content for the formant
    /// filters to carve into vowel resonances.
    phase: f32,
    /// 3 formant biquads — one per (F1, F2, F3) of the played
    /// vowel.
    formants: [FormantBiquad; FORMANT_COUNT],
    /// One-pole LP state for the source brightness filter.
    /// Smooths the saw's high harmonics down toward a hummed-
    /// vowel character at low brightness.
    bright_lp: f32,
    /// Cached parameters — the formant biquads only get
    /// recomputed when these change (rare relative to the audio
    /// rate; the inner loop just runs the cached coefficients).
    cached_vowel: u8,
    cached_morph: f32,
    cached_shift: f32,
    cached_sr: f32,
    /// Played fundamental (Hz).
    base_freq: f32,
    velocity: f32,
    /// Voice envelope.
    env: AdsrState,
}

impl VocalVoice {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            formants: [FormantBiquad::new(); FORMANT_COUNT],
            bright_lp: 0.0,
            cached_vowel: u8::MAX,
            cached_morph: -1.0,
            cached_shift: -1.0,
            cached_sr: 0.0,
            base_freq: 261.625_56,
            velocity: 1.0,
            env: AdsrState::new(),
        }
    }

    pub fn trigger(&mut self, freq_hz: f32, velocity: f32) {
        self.base_freq = freq_hz.clamp(20.0, 4_000.0);
        self.velocity = velocity.clamp(0.0, 1.5);
        self.env.trigger();
    }

    pub fn gate_off(&mut self) {
        self.env.release();
    }

    /// Recompute the 3 formant biquads.  Called lazily — only
    /// when one of the inputs that governs them (vowel / morph /
    /// formant_shift / sr) actually moves.
    fn refresh_coefficients(&mut self, p: &AudioParams, sr: f32) {
        let v = (p.vocal_vowel as usize).min(VOWEL_FORMANTS.len() - 1);
        let next = (v + 1) % VOWEL_FORMANTS.len();
        let m = p.vocal_morph.clamp(0.0, 1.0);
        // formant_shift 0..1 → 0.66..1.51 multiplier so that
        // 0.5 (the natural-male centre) is exactly 1.0×.
        // Range chosen to span ~ male → female / child
        // territory without veering into helium-or-ogre extremes
        // by default (the user can still push hard if they want).
        let shift = 0.66_f32.powf(1.0 - 2.0 * p.vocal_formant_shift.clamp(0.0, 1.0));
        let preset_a = &VOWEL_FORMANTS[v];
        let preset_b = &VOWEL_FORMANTS[next];
        for (i, ((f_a, q_a), (f_b, q_b))) in preset_a
            .iter()
            .zip(preset_b.iter())
            .enumerate()
            .take(FORMANT_COUNT)
        {
            let f = (f_a + (f_b - f_a) * m) * shift;
            let q = q_a + (q_b - q_a) * m;
            self.formants[i].set(f, q, sr);
        }
        self.cached_vowel = p.vocal_vowel;
        self.cached_morph = p.vocal_morph;
        self.cached_shift = p.vocal_formant_shift;
        self.cached_sr = sr;
    }

    /// One-sample process.  Returns mono — process_block applies
    /// pan + voice volume at the master mix stage.
    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.vocal_enabled {
            return 0.0;
        }
        // Lazy-refresh the formant coefficients when any of the
        // vowel-side inputs move or the sample rate changes.
        if p.vocal_vowel != self.cached_vowel
            || (p.vocal_morph - self.cached_morph).abs() > 1e-4
            || (p.vocal_formant_shift - self.cached_shift).abs() > 1e-4
            || (sr - self.cached_sr).abs() > 0.5
        {
            self.refresh_coefficients(p, sr);
        }

        self.env.step(
            p.vocal_attack,
            p.vocal_decay,
            p.vocal_sustain,
            p.vocal_release,
            sr,
        );
        let env_gain = self.env.value;
        if env_gain < 1e-6 {
            return 0.0;
        }

        // Saw oscillator — phase advances every sample regardless
        // of envelope state so the phase stays musically aligned
        // across retriggers.
        self.phase = (self.phase + self.base_freq / sr).fract();
        let saw = 2.0 * self.phase - 1.0;

        // Source brightness — 1-pole LP smooths the saw's high
        // harmonics down toward a hummed-vowel character at low
        // brightness.  sr-aware so the cutoff stays musical
        // across rates.
        let bright = p.vocal_brightness.clamp(0.0, 1.0);
        let fc = 200.0 * 60.0_f32.powf(bright); // 200 Hz → 12 kHz log
        let alpha = one_pole_lp_alpha(fc, sr);
        self.bright_lp += alpha * (saw - self.bright_lp);
        let source = self.bright_lp;

        // Pass through the 3 formant biquads in parallel and sum.
        // Equal weights — the per-formant Q already shapes the
        // relative emphasis, and adding per-formant levels just
        // duplicates the brightness control.  Sum scaled by 1/3
        // so a fully-driven bank stays near unity.
        let mut acc = 0.0_f32;
        for f in &mut self.formants {
            acc += f.process(source);
        }
        acc *= 1.0 / FORMANT_COUNT as f32;

        acc * env_gain * self.velocity * p.vocal_volume.clamp(0.0, 1.5)
    }

    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.env.stage != AdsrStage::Off
    }
}

impl Default for VocalVoice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, VOCAL_VOWEL_PRESETS};

    fn enabled_params() -> AudioParams {
        let mut s = AppState::default();
        s.vocal.enabled = true;
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn silent_before_trigger() {
        let mut v = VocalVoice::new();
        let p = enabled_params();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn silent_when_disabled() {
        let mut v = VocalVoice::new();
        let s = AppState::default();
        let p = AudioParams::from_app_state(&s);
        v.trigger(220.0, 1.0);
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn trigger_produces_audible_output() {
        let mut v = VocalVoice::new();
        let p = enabled_params();
        v.trigger(220.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak > 0.05, "audible output (peak {peak})");
    }

    #[test]
    fn every_vowel_preset_produces_output() {
        // Sanity-check every vowel can drive the formant bank
        // without numerical issues.  Catches a regression where,
        // e.g., a future preset with too-high F3 hits the
        // Nyquist clamp (which would silence that band).
        for v_idx in 0..VOCAL_VOWEL_PRESETS {
            let mut s = AppState::default();
            s.vocal.enabled = true;
            s.vocal.vowel = v_idx;
            let p = AudioParams::from_app_state(&s);
            let mut v = VocalVoice::new();
            v.trigger(220.0, 1.0);
            let mut peak = 0.0_f32;
            for _ in 0..4_000 {
                let out = v.process(48_000.0, &p);
                assert!(out.is_finite());
                peak = peak.max(out.abs());
            }
            assert!(
                peak > 0.05,
                "vowel preset {v_idx} produces audible output (peak {peak})"
            );
        }
    }

    #[test]
    fn output_bounded_under_full_drive() {
        let mut s = AppState::default();
        s.vocal.enabled = true;
        s.vocal.volume = 1.5;
        s.vocal.brightness = 1.0;
        s.vocal.formant_shift = 1.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = VocalVoice::new();
        v.trigger(110.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..16_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 3.0, "vocal voice stays bounded (peak {peak})");
    }

    #[test]
    fn release_eventually_silences() {
        let mut s = AppState::default();
        s.vocal.enabled = true;
        s.vocal.attack = 0.0;
        s.vocal.decay = 0.0;
        s.vocal.sustain = 0.5;
        s.vocal.release = 0.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = VocalVoice::new();
        v.trigger(220.0, 1.0);
        for _ in 0..1_000 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        for _ in 0..192_000 {
            let _ = v.process(48_000.0, &p);
        }
        assert!(!v.is_active(), "envelope reaches Off after release");
    }
}
