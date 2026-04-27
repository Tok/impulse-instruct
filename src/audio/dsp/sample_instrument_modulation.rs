// ─── audio/dsp/sample_instrument_modulation.rs ──────────────────────────────
// SF2 modulation surface for the SampleInstrument voice — both the
// dual-LFO block (mod LFO + vib LFO with their pitch / filter / volume
// targets) and the five-stage modulation envelope (Delay → Attack →
// Hold → Decay → Sustain → Release driving modEnvToPitch +
// modEnvToFilterFc).  Lifted out of `sample_instrument.rs` once the
// SF2 mod-env addition pushed it past the 1000-line cap; sibling-style
// split keeps the audio-thread voice file under cap and co-locates
// every SF2 generator-driven modulator.
//
// Pure functional state machines: `step()` advances one sample and
// returns the per-sample modulator values the voice consumes.
// Allocation-free — config + state are POD-shaped with stack
// lifetimes; the audio thread holds them inline on each slot.
//
// Stage curves: Attack / Decay / Release on the mod env use the same
// one-pole exponential the volume envelope uses
// (`coef = exp(-1 / (t · sr))`); Hold / Sustain are flat holds.  SF2
// spec text describes a "convex" attack but FluidSynth + most real-
// world implementations use exponential, which matches our existing
// voice envelope so the two stay in lock-step audibly.

// ─── Mod LFO + vib LFO ───────────────────────────────────────────────────────

/// Dual-LFO config block — copied per-trigger when the region
/// declares any non-zero LFO depth.  Mod LFO drives three targets
/// (pitch / filter cutoff / volume); vib LFO drives pitch only.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RegionLfos {
    pub(crate) mod_freq_hz: f32,
    pub(crate) mod_delay_s: f32,
    pub(crate) mod_to_pitch_cents: f32,
    /// Filter-cutoff swing in cents at LFO peak (`sin = ±1`).  Added
    /// to the active filter's cutoff in cents — only audible when the
    /// region also declares an `initialFilterFc` (otherwise the
    /// 13500-cent default reads as "no filter" and the region_filter
    /// path never engages).
    pub(crate) mod_to_filter_cents: f32,
    /// Volume swing in centibels at LFO peak.  100 cB = 10 dB swing
    /// peak-to-peak per SF2 spec 8.1.3; applied as a multiplicative
    /// gain factor `10^(lfo · depth / 200)` so positive depth boosts
    /// at the LFO peak (matches FluidSynth's polarity convention).
    pub(crate) mod_to_volume_cb: f32,
    pub(crate) vib_freq_hz: f32,
    pub(crate) vib_delay_s: f32,
    pub(crate) vib_to_pitch_cents: f32,
}

/// Running per-slot LFO state.  Each LFO has a phase (0..1) and a
/// delay countdown — the LFO contributes 0 modulation until its delay
/// elapses (the SF2 spec's per-LFO delay generator).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LfoSlotState {
    pub(crate) mod_phase: f32,
    pub(crate) vib_phase: f32,
    pub(crate) mod_delay_remain_s: f32,
    pub(crate) vib_delay_remain_s: f32,
}

impl LfoSlotState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Reset on new note — phases back to 0, delay countdowns
    /// rearmed from the region's config.
    pub(crate) fn trigger(&mut self, lfos: &RegionLfos) {
        self.mod_phase = 0.0;
        self.vib_phase = 0.0;
        self.mod_delay_remain_s = lfos.mod_delay_s;
        self.vib_delay_remain_s = lfos.vib_delay_s;
    }

    /// Advance one sample.  Returns `(mod_value, vib_value)` — both
    /// in -1..+1 (sine of the running phase) when the LFO is active,
    /// 0 while the per-LFO delay window is still elapsing.  Caller
    /// scales these by the depth fields on `RegionLfos` for each
    /// target (pitch / filter / volume).
    pub(crate) fn step(&mut self, lfos: &RegionLfos, sr: f32) -> (f32, f32) {
        let dt = 1.0 / sr;
        let mod_active = if self.mod_delay_remain_s > 0.0 {
            self.mod_delay_remain_s -= dt;
            false
        } else {
            true
        };
        self.mod_phase += lfos.mod_freq_hz * dt;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        let mod_value = if mod_active {
            (self.mod_phase * std::f32::consts::TAU).sin()
        } else {
            0.0
        };
        let vib_active = if self.vib_delay_remain_s > 0.0 {
            self.vib_delay_remain_s -= dt;
            false
        } else {
            true
        };
        self.vib_phase += lfos.vib_freq_hz * dt;
        if self.vib_phase >= 1.0 {
            self.vib_phase -= 1.0;
        }
        let vib_value = if vib_active {
            (self.vib_phase * std::f32::consts::TAU).sin()
        } else {
            0.0
        };
        (mod_value, vib_value)
    }
}

// ─── Modulation envelope ─────────────────────────────────────────────────────

/// SF2 modulation-envelope configuration — copied per-trigger when
/// the region declares a non-zero target depth (`to_pitch_cents` or
/// `to_filter_cents`).  Times are seconds (already converted from
/// SF2 timecents at load time); sustain is the linear 0..1 level the
/// envelope decays to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RegionModEnv {
    pub(crate) delay_s: f32,
    pub(crate) attack_s: f32,
    pub(crate) hold_s: f32,
    pub(crate) decay_s: f32,
    /// Linear 0..1.  Pre-converted from SF2 `sustainModEnv` which
    /// stores 0.1 % units of attenuation from peak (1000 - cb/10 = %
    /// of full level).  0.0 = silent sustain, 1.0 = full hold.
    pub(crate) sustain_level: f32,
    pub(crate) release_s: f32,
    /// Cents shift on the read-rate at full envelope value (env = 1.0).
    /// Multiplied by `env_value` per sample then converted to a rate
    /// factor via the `1 + cents · ln(2)/1200` small-angle approximation
    /// (matches the LFO pitch path).
    pub(crate) to_pitch_cents: f32,
    /// Cents shift on the filter cutoff knob at full envelope value.
    /// The voice converts cents → knob delta via the same closed-form
    /// `8.491e-5` constant the mod-LFO filter target uses.  No-op when
    /// the region carries no `region_filter` (the cutoff knob never
    /// reaches the SVF in that case).
    pub(crate) to_filter_cents: f32,
}

/// AHDSR stage tracked per-slot.  Distinct from the volume-envelope
/// stages since the mod env can be in `Delay` / `Hold` (no analogue on
/// the volume side) and the two envelopes are released independently
/// (gate-off triggers Release on both, but their decay tails are
/// separate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModEnvStage {
    Off,
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

/// Per-slot envelope state.  Holds the current stage, the envelope
/// value (0..1), and two countdowns (Delay / Hold).  Reset to `Off`
/// on construction; `trigger()` enters the first non-zero stage.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ModEnvState {
    pub(crate) stage: ModEnvStage,
    pub(crate) value: f32,
    pub(crate) delay_remain_s: f32,
    pub(crate) hold_remain_s: f32,
}

impl ModEnvState {
    pub(crate) fn new() -> Self {
        Self {
            stage: ModEnvStage::Off,
            value: 0.0,
            delay_remain_s: 0.0,
            hold_remain_s: 0.0,
        }
    }

    /// Reset on new note — enter the first stage that has non-zero
    /// duration so a 0-second `delay` collapses to Attack immediately
    /// (matches SF2 spec behaviour where -12000 timecents = "instant"
    /// is identical to "generator absent").
    pub(crate) fn trigger(&mut self, env: &RegionModEnv) {
        self.value = 0.0;
        self.delay_remain_s = env.delay_s;
        self.hold_remain_s = env.hold_s;
        self.stage = if env.delay_s > 1e-4 {
            ModEnvStage::Delay
        } else {
            ModEnvStage::Attack
        };
    }

    /// Gate-off — flips any non-Off stage into Release so the env
    /// decays from its current value to 0 over `release_s`.  No-op
    /// when already Off (slot already silent).
    pub(crate) fn release(&mut self) {
        if self.stage != ModEnvStage::Off {
            self.stage = ModEnvStage::Release;
        }
    }

    /// Advance one sample.  Returns the unipolar 0..1 env value the
    /// voice multiplies into the per-sample modulation depths.
    pub(crate) fn step(&mut self, env: &RegionModEnv, sr: f32) -> f32 {
        let dt = 1.0 / sr;
        match self.stage {
            ModEnvStage::Off => {
                self.value = 0.0;
            }
            ModEnvStage::Delay => {
                self.delay_remain_s -= dt;
                if self.delay_remain_s <= 0.0 {
                    self.stage = ModEnvStage::Attack;
                }
                self.value = 0.0;
            }
            ModEnvStage::Attack => {
                let coef = (-1.0_f32 / (env.attack_s.max(0.0005) * sr)).exp();
                self.value = 1.0 - (1.0 - self.value) * coef;
                if self.value >= 0.999 {
                    self.value = 1.0;
                    self.stage = if env.hold_s > 1e-4 {
                        ModEnvStage::Hold
                    } else {
                        ModEnvStage::Decay
                    };
                }
            }
            ModEnvStage::Hold => {
                self.value = 1.0;
                self.hold_remain_s -= dt;
                if self.hold_remain_s <= 0.0 {
                    self.stage = ModEnvStage::Decay;
                }
            }
            ModEnvStage::Decay => {
                let coef = (-1.0_f32 / (env.decay_s.max(0.0005) * sr)).exp();
                self.value = env.sustain_level + (self.value - env.sustain_level) * coef;
                if (self.value - env.sustain_level).abs() < 1e-3 {
                    self.value = env.sustain_level;
                    self.stage = ModEnvStage::Sustain;
                }
            }
            ModEnvStage::Sustain => {
                self.value = env.sustain_level;
            }
            ModEnvStage::Release => {
                let coef = (-1.0_f32 / (env.release_s.max(0.0005) * sr)).exp();
                self.value *= coef;
                if self.value < 1e-5 {
                    self.value = 0.0;
                    self.stage = ModEnvStage::Off;
                }
            }
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(attack: f32, decay: f32, sustain: f32, release: f32) -> RegionModEnv {
        RegionModEnv {
            delay_s: 0.0,
            attack_s: attack,
            hold_s: 0.0,
            decay_s: decay,
            sustain_level: sustain,
            release_s: release,
            to_pitch_cents: 0.0,
            to_filter_cents: 0.0,
        }
    }

    #[test]
    fn new_state_is_off() {
        let s = ModEnvState::new();
        assert_eq!(s.stage, ModEnvStage::Off);
        assert_eq!(s.value, 0.0);
    }

    /// Attack with no Delay / Hold ramps from 0 → ~1 over the attack
    /// time, then enters Decay heading towards the sustain level.
    /// We let the env run long enough to hit Sustain and assert the
    /// value lands at the configured sustain.
    #[test]
    fn ahdsr_reaches_sustain_level() {
        let e = env(0.005, 0.010, 0.4, 0.020);
        let mut s = ModEnvState::new();
        s.trigger(&e);
        let sr = 48_000.0;
        // 200 ms is plenty for a 5 ms / 10 ms attack/decay to settle.
        for _ in 0..(0.2 * sr) as i32 {
            s.step(&e, sr);
        }
        assert_eq!(s.stage, ModEnvStage::Sustain);
        assert!(
            (s.value - 0.4).abs() < 1e-3,
            "sustain value {} should match 0.4",
            s.value
        );
    }

    /// Delay stage holds the env at 0 until the delay window elapses.
    /// 30 ms delay → first 1000 samples (at 48 kHz) should keep the
    /// env at 0; samples past the delay should rise above 0.
    #[test]
    fn delay_stage_holds_value_at_zero() {
        let mut e = env(0.005, 0.010, 1.0, 0.020);
        e.delay_s = 0.030;
        let mut s = ModEnvState::new();
        s.trigger(&e);
        assert_eq!(s.stage, ModEnvStage::Delay);
        let sr = 48_000.0;
        // Within the delay window — env value must be exactly 0.
        for _ in 0..1_000 {
            assert_eq!(s.step(&e, sr), 0.0);
        }
        // Run well past the delay; env should now be climbing.
        for _ in 0..3_000 {
            s.step(&e, sr);
        }
        assert!(
            s.value > 0.5,
            "env should be climbing past delay window (value {})",
            s.value
        );
    }

    /// Hold stage keeps the env at 1.0 for `hold_s` after Attack
    /// completes, then enters Decay.  We trigger with a slow decay
    /// + medium hold so the test can observe the plateau.
    #[test]
    fn hold_stage_keeps_value_at_peak() {
        let mut e = env(0.005, 1.0, 0.0, 0.020);
        e.hold_s = 0.050;
        let mut s = ModEnvState::new();
        s.trigger(&e);
        let sr = 48_000.0;
        // Run through Attack into Hold.
        for _ in 0..1_000 {
            s.step(&e, sr);
        }
        assert!(
            matches!(s.stage, ModEnvStage::Hold | ModEnvStage::Attack),
            "should be in attack or hold (stage {:?})",
            s.stage
        );
        // Skip to mid-Hold and assert the plateau.
        for _ in 0..1_000 {
            s.step(&e, sr);
        }
        assert_eq!(s.stage, ModEnvStage::Hold);
        assert!(
            (s.value - 1.0).abs() < 1e-3,
            "hold value {} should be ~1.0",
            s.value
        );
    }

    /// Release flips the stage from any active phase down to Release
    /// and the value decays to 0 within ~release_s.
    #[test]
    fn release_decays_to_zero() {
        let e = env(0.001, 0.005, 0.7, 0.010);
        let mut s = ModEnvState::new();
        s.trigger(&e);
        let sr = 48_000.0;
        for _ in 0..1_000 {
            s.step(&e, sr);
        }
        s.release();
        assert_eq!(s.stage, ModEnvStage::Release);
        for _ in 0..(0.2 * sr) as i32 {
            s.step(&e, sr);
        }
        assert_eq!(s.stage, ModEnvStage::Off);
        assert_eq!(s.value, 0.0);
    }
}
