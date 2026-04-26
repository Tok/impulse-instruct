// ─── audio/dsp/fm_ops.rs ──────────────────────────────────────────────────────
// FM operator synth — 4-op DX7-flavoured voice DSP.  Four sine
// oscillators with per-op ADSR envelopes routed by an algorithm
// selector.  See `state::fm_ops` for the parameter shape.
//
// Per-frame cost: 4 phase increments + 4 sine evaluations + 4 ADSR
// steps + algorithm dispatch.  Allocation-free in `process()` —
// every buffer lives on the struct.

use super::AudioParams;

/// Modulation-index scaling.  Op `level` 0..1 multiplies into this
/// constant when the op is acting as a modulator.  4× the standard
/// DX7 unit (which is ≈ 2π) — plenty of headroom for FM-bass /
/// bell territory without wandering into broken spectra.
const FM_INDEX_MAX: f32 = 8.0;

/// ADSR stages — same pattern as `sample_instrument::AdsrStage`,
/// duplicated here so the FM voice doesn't have to share state with
/// the sample instrument's per-slot envelope.
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
        // Don't reset value — overlapping triggers should glide
        // from the current envelope position into Attack rather
        // than clicking back to 0.
    }

    fn release(&mut self) {
        if self.stage != AdsrStage::Off {
            self.stage = AdsrStage::Release;
        }
    }

    fn step(&mut self, attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) {
        // Same knob → seconds map as the SampleInstrument ADSR so
        // the two voices feel consistent across the rack.
        let knob_to_secs =
            |k: f32, lo: f32, hi: f32| -> f32 { (lo + (hi - lo) * k.clamp(0.0, 1.0)).max(0.0005) };
        match self.stage {
            AdsrStage::Off => {
                self.value = 0.0;
            }
            AdsrStage::Attack => {
                let t = knob_to_secs(attack, 0.0005, 1.5);
                let coef = (-1.0_f32 / (t * sr)).exp();
                self.value = 1.0 - (1.0 - self.value) * coef;
                if self.value >= 0.999 {
                    self.value = 1.0;
                    self.stage = AdsrStage::Decay;
                }
            }
            AdsrStage::Decay => {
                let t = knob_to_secs(decay, 0.005, 2.0);
                let coef = (-1.0_f32 / (t * sr)).exp();
                let target = sustain.clamp(0.0, 1.0);
                self.value = target + (self.value - target) * coef;
                if (self.value - target).abs() < 1e-3 {
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
                if self.value < 1e-5 {
                    self.value = 0.0;
                    self.stage = AdsrStage::Off;
                }
            }
        }
    }
}

pub struct FmOpsVoice {
    /// Per-op phase accumulator (radians 0..2π).
    phases: [f32; 4],
    /// Per-op ADSR.
    envs: [AdsrState; 4],
    /// Last-sample output of op 4 — used by the feedback path on
    /// chain algorithms (op 4 sits at the top of the modulator
    /// stack on algos 0/1/2 so it's the natural feedback target,
    /// matching how DX7 algorithms expose their feedback knob).
    /// Stored across `process()` calls so the back-edge has the
    /// usual one-sample delay.
    op4_prev: f32,
    /// Carrier base frequency (Hz) — set on `trigger`, scaled per
    /// op by its `ratio` knob.
    base_freq: f32,
    /// Last triggered velocity (0..1) — multiplied into the final
    /// output gain so accent hits sound louder than soft hits.
    velocity: f32,
}

impl FmOpsVoice {
    pub fn new() -> Self {
        Self {
            phases: [0.0; 4],
            envs: [AdsrState::new(); 4],
            op4_prev: 0.0,
            base_freq: 261.625_56, // C4
            velocity: 1.0,
        }
    }

    /// Sequencer trigger — sets the carrier frequency from the
    /// played MIDI note and pushes every op's ADSR into Attack.
    /// Phases are kept (no zero-reset) so retriggers don't click;
    /// the envelope handles attack shaping.
    pub fn trigger(&mut self, freq_hz: f32, velocity: f32) {
        self.base_freq = freq_hz.clamp(20.0, 8_000.0);
        self.velocity = velocity.clamp(0.0, 1.5);
        for env in &mut self.envs {
            env.trigger();
        }
    }

    /// Sequencer gate-off — every op moves to Release.
    pub fn gate_off(&mut self) {
        for env in &mut self.envs {
            env.release();
        }
    }

    /// One-sample process.  Returns mono — the parent `process_block`
    /// applies pan + voice volume at the master mix stage like every
    /// other voice.
    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.fm_ops_enabled {
            return 0.0;
        }
        // Step every envelope first so the per-op gain reflects the
        // current sample, not the previous one.  Per-op ADSR knobs
        // are bundled per-op for cache locality.
        let attacks = [
            p.fm_ops_op1_attack,
            p.fm_ops_op2_attack,
            p.fm_ops_op3_attack,
            p.fm_ops_op4_attack,
        ];
        let decays = [
            p.fm_ops_op1_decay,
            p.fm_ops_op2_decay,
            p.fm_ops_op3_decay,
            p.fm_ops_op4_decay,
        ];
        let sustains = [
            p.fm_ops_op1_sustain,
            p.fm_ops_op2_sustain,
            p.fm_ops_op3_sustain,
            p.fm_ops_op4_sustain,
        ];
        let releases = [
            p.fm_ops_op1_release,
            p.fm_ops_op2_release,
            p.fm_ops_op3_release,
            p.fm_ops_op4_release,
        ];
        let levels = [
            p.fm_ops_op1_level,
            p.fm_ops_op2_level,
            p.fm_ops_op3_level,
            p.fm_ops_op4_level,
        ];
        let ratios = [
            p.fm_ops_op1_ratio,
            p.fm_ops_op2_ratio,
            p.fm_ops_op3_ratio,
            p.fm_ops_op4_ratio,
        ];
        for i in 0..4 {
            self.envs[i].step(attacks[i], decays[i], sustains[i], releases[i], sr);
        }

        // Effective per-op gain = level * envelope value.  This is
        // what gives FM patches their evolving spectrum — modulator
        // envelopes shape brightness over time, carrier envelopes
        // shape amplitude.
        let env_gains = [
            self.envs[0].value,
            self.envs[1].value,
            self.envs[2].value,
            self.envs[3].value,
        ];

        // Map ratio knobs (0..1) to frequency multipliers (0.5..8x)
        // log-symmetrically so the unison detent sits at knob=0.5.
        let ratio_mult = |knob: f32| -> f32 {
            let k = knob.clamp(0.0, 1.0);
            // 0 → 0.5, 0.5 → 1.0, 1 → 8.0 — log-symmetric piecewise.
            // Single power-of-16 sweep keeps the math cheap; for V1
            // we don't need integer-snap behaviour (DX7-faithful
            // ratio coarse stepping).
            16.0_f32.powf(k - 0.5)
        };

        // Phase increments per sample for each op (radians).
        let two_pi = std::f32::consts::TAU;
        let dt = 1.0 / sr;
        let phase_inc = |freq: f32| freq * two_pi * dt;
        let mut incs = [0.0_f32; 4];
        for (i, slot) in incs.iter_mut().enumerate() {
            let freq = (self.base_freq * ratio_mult(ratios[i])).clamp(0.05, sr * 0.45);
            *slot = phase_inc(freq);
        }

        // Compute each op's modulation phase (with FM offset from
        // upstream ops) according to the algorithm topology.  The
        // algorithm clamp is defensive — `apply_llm_update` already
        // clamps, but the audio thread shouldn't trust upstream.
        let alg = p.fm_ops_algorithm.min(crate::state::FM_ALGORITHM_COUNT - 1);
        let feedback = p.fm_ops_feedback.clamp(0.0, 1.0);

        // Step phases first; we'll evaluate the sines below.  Phase
        // wrap into 0..2π keeps numerical precision tight.
        for (phase, inc) in self.phases.iter_mut().zip(incs.iter()) {
            *phase = (*phase + *inc) % two_pi;
        }

        // Modulation chain.  Op level used as a *modulator* gets
        // scaled by FM_INDEX_MAX (so a level=1 modulator pushes a
        // full ±FM_INDEX_MAX rad swing onto the next op's phase).
        // Op level used as a *carrier* is plain audio gain.
        let fm_in =
            |op_idx: usize, mod_signal: f32| -> f32 { (self.phases[op_idx] + mod_signal).sin() };
        let mod_gain = |level: f32, env: f32| level * env * FM_INDEX_MAX;
        let carrier_gain = |level: f32, env: f32| level * env;

        // Op 4 always evaluated first since it sits at the top of
        // the chain in algos 0..2 and provides the feedback path.
        // Feedback adds the previous sample's op-4 output (× the
        // feedback knob, scaled by the same FM_INDEX_MAX so the
        // knob feels symmetric with op-as-modulator).
        let op4_phase_mod = self.op4_prev * feedback * FM_INDEX_MAX;
        let op4_out = fm_in(3, op4_phase_mod);
        // Cache for next sample — shaped by the env so the
        // feedback ringing decays with the envelope rather than
        // hanging at full amplitude after release.
        self.op4_prev = op4_out * env_gains[3];

        let out = match alg {
            // Stack: 4→3→2→1.  Op 1 is the only carrier.
            0 => {
                let m4 = op4_out * mod_gain(levels[3], env_gains[3]);
                let op3_out = fm_in(2, m4);
                let m3 = op3_out * mod_gain(levels[2], env_gains[2]);
                let op2_out = fm_in(1, m3);
                let m2 = op2_out * mod_gain(levels[1], env_gains[1]);
                let op1_out = fm_in(0, m2);
                op1_out * carrier_gain(levels[0], env_gains[0])
            }
            // Multimod: 4→1, 3→1, 2→1.  Op 1 is the carrier with
            // three parallel modulators.  Sum the modulator phase
            // contributions before evaluating op 1's sine — that's
            // what gives multimod its bell / mallet character.
            1 => {
                let m4 = op4_out * mod_gain(levels[3], env_gains[3]);
                let op3_out = self.phases[2].sin();
                let m3 = op3_out * mod_gain(levels[2], env_gains[2]);
                let op2_out = self.phases[1].sin();
                let m2 = op2_out * mod_gain(levels[1], env_gains[1]);
                let op1_out = fm_in(0, m4 + m3 + m2);
                op1_out * carrier_gain(levels[0], env_gains[0])
            }
            // Parallel pairs: 4→3, 2→1.  Two stacks summed.
            2 => {
                let m4 = op4_out * mod_gain(levels[3], env_gains[3]);
                let op3_out = fm_in(2, m4);
                let op3_carrier = op3_out * carrier_gain(levels[2], env_gains[2]);
                let op2_out = self.phases[1].sin();
                let m2 = op2_out * mod_gain(levels[1], env_gains[1]);
                let op1_out = fm_in(0, m2);
                let op1_carrier = op1_out * carrier_gain(levels[0], env_gains[0]);
                // Average of the two stacks so total carrier headroom
                // stays roughly equal to single-stack algorithms.
                (op1_carrier + op3_carrier) * 0.5
            }
            // Additive: every op is a carrier, no FM.  Sum and scale
            // by 1/4 so a fully-driven additive patch isn't 4× the
            // amplitude of a single-carrier patch.
            _ => {
                let s1 = self.phases[0].sin() * carrier_gain(levels[0], env_gains[0]);
                let s2 = self.phases[1].sin() * carrier_gain(levels[1], env_gains[1]);
                let s3 = self.phases[2].sin() * carrier_gain(levels[2], env_gains[2]);
                let s4 = self.phases[3].sin() * carrier_gain(levels[3], env_gains[3]);
                (s1 + s2 + s3 + s4) * 0.25
            }
        };

        out * self.velocity * p.fm_ops_volume.clamp(0.0, 1.5)
    }

    /// True when any op envelope is past Off — used by tests + the
    /// (future) panel meter.
    #[cfg(test)]
    pub fn any_active(&self) -> bool {
        self.envs.iter().any(|e| e.stage != AdsrStage::Off)
    }
}

impl Default for FmOpsVoice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn make_params_enabled() -> AudioParams {
        let mut s = AppState::default();
        s.fm_ops.enabled = true;
        // Default 2-op stack — op 1 carrier at level 1, op 2
        // modulator at level 0.5.
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn silent_before_trigger() {
        let mut v = FmOpsVoice::new();
        let p = make_params_enabled();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn silent_when_disabled() {
        let mut v = FmOpsVoice::new();
        let s = AppState::default(); // enabled = false
        let p = AudioParams::from_app_state(&s);
        v.trigger(440.0, 1.0);
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn trigger_produces_audible_output() {
        let mut v = FmOpsVoice::new();
        let p = make_params_enabled();
        v.trigger(440.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..2_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak > 0.05, "audible output after trigger (peak {peak})");
    }

    #[test]
    fn release_eventually_silences() {
        // Use fast release per-op so the test budget doesn't have
        // to scale with the (musically slow) default knob.  At
        // release=0 the time constant is 5 ms; 4 s is ~800 τ so
        // every op clears the 1e-5 cutoff comfortably.
        let mut s = AppState::default();
        s.fm_ops.enabled = true;
        for op in [
            &mut s.fm_ops.op1,
            &mut s.fm_ops.op2,
            &mut s.fm_ops.op3,
            &mut s.fm_ops.op4,
        ] {
            op.level = 1.0;
            op.attack = 0.0;
            op.decay = 0.0;
            op.sustain = 0.5;
            op.release = 0.0;
        }
        let p = AudioParams::from_app_state(&s);
        let mut v = FmOpsVoice::new();
        v.trigger(440.0, 1.0);
        for _ in 0..1_000 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        for _ in 0..192_000 {
            let _ = v.process(48_000.0, &p);
        }
        assert!(!v.any_active(), "every op should be silenced after release");
    }

    #[test]
    fn additive_algorithm_sums_four_carriers() {
        // Algo 3 (additive) — set every op's level to 1 and confirm
        // the output stays bounded.  No FM cross-modulation in this
        // mode; the sum of four sines averaged by 1/4 should never
        // exceed 1 in magnitude.
        let mut s = AppState::default();
        s.fm_ops.enabled = true;
        s.fm_ops.algorithm = 3;
        for op in [
            &mut s.fm_ops.op1,
            &mut s.fm_ops.op2,
            &mut s.fm_ops.op3,
            &mut s.fm_ops.op4,
        ] {
            op.level = 1.0;
            op.attack = 0.0;
            op.decay = 0.0;
            op.sustain = 1.0;
        }
        s.fm_ops.volume = 1.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = FmOpsVoice::new();
        v.trigger(440.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 1.5, "additive sum stays bounded (peak {peak})");
        assert!(peak > 0.1, "audible output (peak {peak})");
    }

    #[test]
    fn output_bounded_under_full_modulation() {
        // Stack algo with every op level + feedback maxed — should
        // produce a wild but bounded waveform (no NaN / runaway).
        let mut s = AppState::default();
        s.fm_ops.enabled = true;
        s.fm_ops.algorithm = 0;
        s.fm_ops.feedback = 1.0;
        for op in [
            &mut s.fm_ops.op1,
            &mut s.fm_ops.op2,
            &mut s.fm_ops.op3,
            &mut s.fm_ops.op4,
        ] {
            op.level = 1.0;
            op.attack = 0.0;
            op.decay = 0.0;
            op.sustain = 1.0;
        }
        let p = AudioParams::from_app_state(&s);
        let mut v = FmOpsVoice::new();
        v.trigger(220.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..16_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite(), "non-finite output");
            peak = peak.max(out.abs());
        }
        assert!(
            peak < 8.0,
            "FM stack stays bounded under stress (peak {peak})"
        );
    }
}
