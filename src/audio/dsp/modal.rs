// ─── audio/dsp/modal.rs ───────────────────────────────────────────────────────
// Modal / struck physical-model voice — 8 parallel two-pole
// resonant biquads excited by a short LP-filtered noise burst on
// each trigger.  Cheap, idiomatic modal synthesis: each mode is a
// damped sinusoid `A·exp(-t/τ)·sin(2πfₙt)`; the resonator
// difference equation
//
//   y[n] = 2·r·cos(ω)·y[n-1] − r²·y[n-2] + x[n]
//
// produces exactly that response when fed a unit impulse, where
// `r = exp(-1/(τ·sr))` and `ω = 2π·f/sr`.
//
// Per-frame cost: 8 biquad taps + 1 excitation pop + 1-pole LP on
// the noise burst.  Allocation-free in `process()`.  Output
// normalised by the sum of the per-mode levels so a fully-pegged
// bank stays bounded.

use super::AudioParams;
use super::dsp_util::{AUDIBLE_HZ_MIN, nyquist_guard, one_pole_coef, one_pole_lp_alpha};
use crate::state::{MODAL_MODES, MODAL_RATIO_PRESETS};

/// Idealised mode-frequency ratios per preset.  Values relative to
/// the played fundamental: index 0 is always 1.0 (= played note).
/// Numbers from acoustics references for an idealised church
/// bell, tubular chime, and metal bar; the harmonic preset is
/// just integer multiples for string-/pluck-like timbres.
const RATIO_PRESETS: [[f32; MODAL_MODES]; MODAL_RATIO_PRESETS as usize] = [
    // 0 — Harmonic: integer multiples
    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
    // 1 — Bell: idealised church-bell partials
    [1.0, 2.0, 2.4, 3.0, 4.5, 5.33, 6.66, 8.0],
    // 2 — Tubular chime — narrower inharmonic spread than the bell
    [1.0, 2.756, 5.404, 8.933, 13.345, 18.638, 24.814, 31.871],
    // 3 — Metal / marimba — strong odd-mode emphasis with glassy
    //     overtones; numbers from a uniform-thickness metal bar
    [1.0, 3.984, 9.933, 11.32, 21.83, 27.04, 35.0, 41.0],
];

/// Length of the excitation noise burst in samples.  ~5.3 ms at
/// 48 kHz — short enough to read as "struck" rather than
/// "bowed", long enough to seed every mode in the bank with
/// audible energy.
const EXCITATION_LEN: usize = 256;

#[derive(Clone, Copy, Debug)]
struct Resonator {
    /// Per-sample biquad coefficient `2·r·cos(ω)`.
    a1: f32,
    /// Per-sample biquad coefficient `-r²`.  Stored negated as
    /// `neg_a2 = -a2 = r²` so the inner loop can subtract it
    /// without a sign flip.
    neg_a2: f32,
    /// Input scale `(1 - r²)` — the standard energy-normalising
    /// gain for a resonant biquad.  Without it, long-decay modes
    /// (r → 1) have unbounded resonance gain because their
    /// bandwidth shrinks; with it, the steady-state amplitude
    /// stays consistent across the decay-time range so a fully-
    /// pegged bank with `decay_scale = 1` doesn't clip.
    in_scale: f32,
    /// y[n-1].
    y1: f32,
    /// y[n-2].
    y2: f32,
}

impl Resonator {
    const fn new() -> Self {
        Self {
            a1: 0.0,
            neg_a2: 0.0,
            in_scale: 1.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn set_freq_decay(&mut self, freq_hz: f32, decay_s: f32, sr: f32) {
        // Clamp freq away from Nyquist + DC so the cos doesn't
        // explode and r stays in (0, 1).
        let f = freq_hz.clamp(AUDIBLE_HZ_MIN, nyquist_guard(sr));
        let omega = std::f32::consts::TAU * f / sr;
        let r = one_pole_coef(decay_s.max(0.001), sr);
        self.a1 = 2.0 * r * omega.cos();
        self.neg_a2 = r * r;
        // Energy-preserving input scale: `sqrt(1 - r²)` instead of
        // the `(1 - r²)` constant-output form.  The full
        // normalisation chokes the steady-state amplitude when r
        // is close to 1 (long decay → tiny output); the sqrt
        // form is the standard compromise — bounded steady-state
        // response under continuous excitation, audible decaying
        // tail under impulse / short-burst excitation, which is
        // what we actually drive these resonators with.
        self.in_scale = (1.0 - r * r).sqrt();
    }

    fn process(&mut self, x: f32) -> f32 {
        let y = self.a1 * self.y1 - self.neg_a2 * self.y2 + x * self.in_scale;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub struct ModalVoice {
    bank: [Resonator; MODAL_MODES],
    /// LCG state for the per-trigger noise burst.  Same trick as
    /// `rack_random` / NoiseGen — no `rand` crate dep, no
    /// allocation, deterministic.
    rng_state: u64,
    /// Number of excitation samples remaining to inject.  When >
    /// 0, the next `process` calls pop a noise sample.  Set to
    /// `EXCITATION_LEN` on `trigger`.
    excitation_remaining: usize,
    /// One-pole LP state for the excitation brightness filter.
    /// Smooths the noise burst before it hits the resonators.
    excitation_lp: f32,
    /// Last per-mode level snapshot used for the normalisation
    /// divisor.  Computed on the audio thread from
    /// `p.modal_levels` each sample.
    velocity: f32,
}

impl ModalVoice {
    pub fn new() -> Self {
        Self {
            bank: [Resonator::new(); MODAL_MODES],
            rng_state: 0xCAFE_BABE_DEAD_BEEF,
            excitation_remaining: 0,
            excitation_lp: 0.0,
            velocity: 1.0,
        }
    }

    /// Sequencer trigger.  Loads each resonator's frequency +
    /// decay from `p`, then primes a noise burst that will feed
    /// every mode for ~5 ms.  Phase / state of the resonators is
    /// preserved across triggers so the user can layer hits on a
    /// still-ringing bell and the tails sum naturally.
    pub fn trigger(&mut self, freq_hz: f32, velocity: f32, p: &AudioParams, sr: f32) {
        self.velocity = velocity.clamp(0.0, 1.5);
        let preset = (p.modal_ratio_preset as usize).min(RATIO_PRESETS.len() - 1);
        let ratios = &RATIO_PRESETS[preset];
        // decay_scale 0..1 → 0.005..5 s on the fundamental.  Each
        // higher mode dies ~30% faster per index step so the
        // bright "ping" attack settles into the warmer
        // fundamental.
        let base_tau = 0.005 + p.modal_decay_scale.clamp(0.0, 1.0) * 4.995;
        for (i, mode) in self.bank.iter_mut().enumerate() {
            let f = freq_hz * ratios[i];
            let tau = base_tau / (1.0 + i as f32 * 0.3);
            mode.set_freq_decay(f, tau, sr);
        }
        self.excitation_remaining = EXCITATION_LEN;
        self.excitation_lp = 0.0;
    }

    /// Sequencer gate-off — modal voices aren't really
    /// "gateable" (the resonators ring out naturally), but
    /// providing the hook keeps the Voice trait surface
    /// consistent with every other voice.  We dampen the bank's
    /// state slightly to suggest a hand on the bell, more
    /// gracefully than abruptly silencing it.
    pub fn gate_off(&mut self) {
        for mode in &mut self.bank {
            mode.y1 *= 0.5;
            mode.y2 *= 0.5;
        }
    }

    /// xorshift-style step on the LCG state.  Returns a value in
    /// [-1, 1].  Inline so the inner-loop call is free.
    #[inline]
    fn next_noise(&mut self) -> f32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.rng_state >> 33) as i32) as f32 / (i32::MAX as f32)
    }

    /// One-sample process.  Returns mono — process_block applies
    /// pan + voice volume at the master mix stage like every
    /// other voice.
    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.modal_enabled {
            return 0.0;
        }
        // Pop one excitation sample (or 0 when the burst's
        // exhausted).  Brightness drives a 1-pole LP applied
        // *to the noise* before it hits the resonators — high
        // brightness lets sharp transients excite high modes,
        // low brightness gives a softer mallet hit.
        let raw = if self.excitation_remaining > 0 {
            self.excitation_remaining -= 1;
            self.next_noise()
        } else {
            0.0
        };
        // 1-pole LP coefficient — sr-aware so the cutoff stays
        // musical across sample rates.  brightness 0 → fc ≈ 200 Hz
        // (very dark, fundamental-only excitation); brightness 1
        // → fc ≈ 12 kHz (almost unfiltered).  Cheap one-pole smoother.
        let bright = p.modal_brightness.clamp(0.0, 1.0);
        let fc = 200.0 * 60.0_f32.powf(bright); // log sweep 200 Hz → 12 kHz
        let alpha = one_pole_lp_alpha(fc, sr);
        self.excitation_lp += alpha * (raw - self.excitation_lp);

        // Sum of levels for normalisation — keeps a fully-pegged
        // spectrum bounded.  Add 1e-3 so an all-zero spectrum
        // doesn't divide by zero (output is silent anyway).
        let level_sum: f32 = p.modal_levels.iter().sum::<f32>().max(1e-3);

        let mut acc = 0.0_f32;
        // Excitation is scaled by velocity at the bank input so a
        // soft hit drives soft, a hard hit drives loud — the
        // resonators do the rest.
        let drive = self.excitation_lp * self.velocity;
        for (i, mode) in self.bank.iter_mut().enumerate() {
            let y = mode.process(drive);
            acc += y * p.modal_levels[i];
        }
        acc *= p.modal_volume.clamp(0.0, 1.5) / level_sum;
        acc
    }

    /// True when *any* resonator's state is above a numerical
    /// floor — used by tests to verify the bank actually rings
    /// out after a gate-off.  Cheap inner-product check.
    #[cfg(test)]
    pub fn any_ringing(&self) -> bool {
        self.bank
            .iter()
            .any(|m| m.y1.abs() > 1e-5 || m.y2.abs() > 1e-5)
    }
}

impl Default for ModalVoice {
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
        s.modal.enabled = true;
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn silent_before_trigger() {
        let mut v = ModalVoice::new();
        let p = enabled_params();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn silent_when_disabled() {
        let mut v = ModalVoice::new();
        let s = AppState::default();
        let p = AudioParams::from_app_state(&s);
        v.trigger(440.0, 1.0, &p, 48_000.0);
        for _ in 0..2_000 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn trigger_produces_audible_output() {
        let mut v = ModalVoice::new();
        let p = enabled_params();
        v.trigger(440.0, 1.0, &p, 48_000.0);
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak > 0.05, "audible output (peak {peak})");
    }

    #[test]
    fn output_bounded_under_full_spectrum() {
        // Every mode at full level — output should stay bounded
        // thanks to the level-sum normalisation.  Bell preset
        // because that's the default.
        let mut s = AppState::default();
        s.modal.enabled = true;
        s.modal.levels = [1.0; MODAL_MODES];
        s.modal.volume = 1.5;
        s.modal.decay_scale = 1.0;
        s.modal.brightness = 1.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = ModalVoice::new();
        v.trigger(220.0, 1.0, &p, 48_000.0);
        let mut peak = 0.0_f32;
        for _ in 0..16_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite(), "non-finite output");
            peak = peak.max(out.abs());
        }
        assert!(peak <= 3.0, "fully-pegged bank stays bounded (peak {peak})");
    }

    #[test]
    fn modes_eventually_stop_ringing() {
        // With short decay, the bank should dampen out before
        // the test budget elapses.  Property of the cheap-N-mode
        // resonator design: r < 1 so each mode's energy strictly
        // decays absent re-excitation.
        let mut s = AppState::default();
        s.modal.enabled = true;
        s.modal.decay_scale = 0.0; // ~5 ms tau on fundamental
        let p = AudioParams::from_app_state(&s);
        let mut v = ModalVoice::new();
        v.trigger(440.0, 1.0, &p, 48_000.0);
        // Run 200 ms — well past 40× the longest tau in the bank.
        for _ in 0..(48_000 / 5) {
            let _ = v.process(48_000.0, &p);
        }
        assert!(
            !v.any_ringing(),
            "short-decay bank should be silent within 200 ms"
        );
    }

    #[test]
    fn each_preset_produces_audible_output() {
        // Sanity-check every ratio preset can excite the bank
        // without numerical issues.  Catches the case where a
        // future preset addition has a too-high ratio that hits
        // the Nyquist clamp (which would silence that mode).
        for preset in 0..MODAL_RATIO_PRESETS {
            let mut s = AppState::default();
            s.modal.enabled = true;
            s.modal.ratio_preset = preset;
            let p = AudioParams::from_app_state(&s);
            let mut v = ModalVoice::new();
            v.trigger(220.0, 1.0, &p, 48_000.0);
            let mut peak = 0.0_f32;
            for _ in 0..2_000 {
                let out = v.process(48_000.0, &p);
                assert!(out.is_finite());
                peak = peak.max(out.abs());
            }
            assert!(
                peak > 0.05,
                "preset {preset} produces audible output (peak {peak})"
            );
        }
    }
}
