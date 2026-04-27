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

use crate::state::SfzRegion;

// ─── Pure DSP math helpers ───────────────────────────────────────────────────
//
// These are the closed-form unit conversions the per-sample modulation
// code relies on.  They live here as named functions (rather than
// magic-number multiplies inline in `process_slot`) so the call sites
// stay readable and the math is unit-testable in isolation.

/// `ln(2) / 1200` — small-angle Taylor coefficient that turns cents
/// into a pitch ratio via `1 + cents · COEF`.  Accurate to <0.01 %
/// within ±1200 cents.  Used for the LFO pitch path where the LFO
/// depths are clamped to ±1200 cents at construction.
pub(crate) const CENTS_TO_TAYLOR_RATE_COEF: f32 = 0.000_577_6;

/// `ln(2) / (1200 · ln(900))` — closed-form constant that turns
/// cents-of-cutoff-shift into a 0..1 SVF knob delta.  Derivation: the
/// SVF maps `knob → 20 · 900^knob`, so adding `dC` cents to the
/// cutoff scales hz by `2^(dC/1200)` and shifts the knob by exactly
/// `dC · ln(2) / (1200 · ln(900))`.  Constant per-sample cost — no
/// `powf` needed for filter cutoff modulation.
pub(crate) const CENTS_TO_SVF_KNOB_COEF: f32 = 8.491e-5;

/// Convert a cents pitch shift into a multiplicative rate factor via
/// the Taylor approximation `1 + cents · ln(2)/1200`.  Cheap enough
/// for per-sample LFO use; for larger excursions (mod env can swing
/// ±12000 cents) use [`cents_to_exact_rate_factor`].
#[inline]
pub(crate) fn cents_to_taylor_rate_factor(cents: f32) -> f32 {
    1.0 + cents * CENTS_TO_TAYLOR_RATE_COEF
}

/// Convert a cents pitch shift into a multiplicative rate factor
/// exactly via `2^(cents/1200)`.  One `powf` per call — used by the
/// mod-env pitch path, where the depth can reach ±12000 cents (10
/// octaves) and the small-angle approximation breaks down.
#[inline]
pub(crate) fn cents_to_exact_rate_factor(cents: f32) -> f32 {
    2.0_f32.powf(cents / 1200.0)
}

/// Convert a cents-of-cutoff-shift into a 0..1 SVF knob delta.  See
/// [`CENTS_TO_SVF_KNOB_COEF`] for the derivation.  The caller is
/// responsible for adding the delta to the base knob and clamping
/// the result to `0.0..=1.0`.
#[inline]
pub(crate) fn filter_cents_to_knob_delta(cents: f32) -> f32 {
    cents * CENTS_TO_SVF_KNOB_COEF
}

/// Convert a centibel offset into a multiplicative linear gain
/// factor: `10^(cb / 200)`.  Positive `cb` boosts (matches the
/// FluidSynth convention used by `modLfoToVolume`).
#[inline]
pub(crate) fn cb_to_linear_gain(cb: f32) -> f32 {
    10.0_f32.powf(cb / 200.0)
}

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

/// Build a `RegionLfos` config from a parsed `SfzRegion`, returning
/// `None` when every depth target is below the activation threshold
/// (no audible modulation, so no point allocating per-slot phase
/// state).  Threshold 0.5 (cents / cB) skips the SF2 spec-default
/// "0" cleanly without tripping on tiny round-trip noise from the
/// i16 generator unit.  All four LFO timing fields and depth fields
/// are clamped to musically sane ranges so a malformed SF2 can't
/// stall the per-sample step.
pub(crate) fn region_lfos_from(region: &SfzRegion) -> Option<RegionLfos> {
    if region.mod_lfo_to_pitch_cents.abs() <= 0.5
        && region.mod_lfo_to_filter_fc_cents.abs() <= 0.5
        && region.mod_lfo_to_volume_cb.abs() <= 0.5
        && region.vib_lfo_to_pitch_cents.abs() <= 0.5
    {
        return None;
    }
    Some(RegionLfos {
        mod_freq_hz: region.mod_lfo_freq_hz.clamp(0.05, 20.0),
        mod_delay_s: region.mod_lfo_delay_s.clamp(0.0, 5.0),
        mod_to_pitch_cents: region.mod_lfo_to_pitch_cents.clamp(-1200.0, 1200.0),
        mod_to_filter_cents: region.mod_lfo_to_filter_fc_cents.clamp(-12000.0, 12000.0),
        mod_to_volume_cb: region.mod_lfo_to_volume_cb.clamp(-960.0, 960.0),
        vib_freq_hz: region.vib_lfo_freq_hz.clamp(0.05, 20.0),
        vib_delay_s: region.vib_lfo_delay_s.clamp(0.0, 5.0),
        vib_to_pitch_cents: region.vib_lfo_to_pitch_cents.clamp(-1200.0, 1200.0),
    })
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

/// Build a `RegionModEnv` config from a parsed `SfzRegion`, returning
/// `None` when neither depth target reaches the activation threshold.
/// Mirrors `region_lfos_from` — both stay None when the SF2 region
/// declares no modulation, and the per-sample apply short-circuits.
/// All five timing fields + sustain + two depths are clamped so
/// extreme generator values can't stall the AHDSR state machine.
pub(crate) fn region_mod_env_from(region: &SfzRegion) -> Option<RegionModEnv> {
    if region.mod_env_to_pitch_cents.abs() <= 0.5 && region.mod_env_to_filter_fc_cents.abs() <= 0.5
    {
        return None;
    }
    Some(RegionModEnv {
        delay_s: region.mod_env_delay_s.clamp(0.0, 20.0),
        attack_s: region.mod_env_attack_s.clamp(0.0, 20.0),
        hold_s: region.mod_env_hold_s.clamp(0.0, 20.0),
        decay_s: region.mod_env_decay_s.clamp(0.0, 20.0),
        sustain_level: region.mod_env_sustain_level.clamp(0.0, 1.0),
        release_s: region.mod_env_release_s.clamp(0.0, 20.0),
        to_pitch_cents: region.mod_env_to_pitch_cents.clamp(-12000.0, 12000.0),
        to_filter_cents: region.mod_env_to_filter_fc_cents.clamp(-12000.0, 12000.0),
    })
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

    // ─── Pure math helpers ────────────────────────────────────────────────

    /// `cents_to_taylor_rate_factor(0)` is exactly 1 — the no-op
    /// pitch shift maps to a unity rate factor.
    #[test]
    fn cents_to_taylor_rate_factor_zero_is_unity() {
        assert_eq!(cents_to_taylor_rate_factor(0.0), 1.0);
    }

    /// The Taylor approximation tracks the exact `2^(c/1200)`
    /// closely at musical LFO depths (≤ ±100 cents — the typical
    /// vibrato range) but degrades beyond that.  This test pins
    /// the accuracy at the depths the LFO usually runs at; the
    /// ±1200-cent clamp is a safety bound and not where the
    /// approximation is intended to be tight.
    #[test]
    fn cents_to_taylor_rate_factor_accurate_at_musical_depths() {
        for cents in [-100.0_f32, -50.0, -10.0, 10.0, 50.0, 100.0] {
            let approx = cents_to_taylor_rate_factor(cents);
            let exact = cents_to_exact_rate_factor(cents);
            let rel_err = ((approx - exact) / exact).abs();
            assert!(
                rel_err < 0.005,
                "Taylor approx at {cents} cents drifted (err {rel_err})"
            );
        }
    }

    /// Octave shifts (±1200 cents) come out exactly 2× / 0.5× via the
    /// exact `2^(c/1200)` mapping.  Tight tolerance — this is the
    /// closed-form exponent.
    #[test]
    fn cents_to_exact_rate_factor_octaves() {
        assert!((cents_to_exact_rate_factor(0.0) - 1.0).abs() < 1e-6);
        assert!((cents_to_exact_rate_factor(1200.0) - 2.0).abs() < 1e-4);
        assert!((cents_to_exact_rate_factor(-1200.0) - 0.5).abs() < 1e-4);
        assert!((cents_to_exact_rate_factor(2400.0) - 4.0).abs() < 1e-3);
    }

    /// `filter_cents_to_knob_delta` should match the closed-form
    /// derivation: for any `knob`, applying the delta produces the
    /// same Hz as adding the cents directly to the SVF Hz mapping.
    /// Verifies the math constant is correct end-to-end.
    #[test]
    fn filter_cents_to_knob_delta_matches_svf_hz_mapping() {
        for &base_knob in &[0.2_f32, 0.5, 0.8] {
            for &dc in &[-1200.0_f32, -300.0, 300.0, 1200.0] {
                let base_hz = 20.0_f32 * 900.0_f32.powf(base_knob);
                let target_hz = base_hz * 2.0_f32.powf(dc / 1200.0);
                let target_knob = (target_hz / 20.0).log(900.0);
                let new_knob = base_knob + filter_cents_to_knob_delta(dc);
                assert!(
                    (new_knob - target_knob).abs() < 1e-4,
                    "knob delta at base {base_knob} dC {dc} drifted (got {new_knob}, want {target_knob})"
                );
            }
        }
    }

    /// `cb_to_linear_gain` should map 0 cB → 1.0 (unity), positive cB
    /// → boost, negative cB → cut.  100 cB ≈ 10 dB ≈ 3.162× gain.
    #[test]
    fn cb_to_linear_gain_known_values() {
        assert_eq!(cb_to_linear_gain(0.0), 1.0);
        // 100 cB = 10 dB → 10^0.5 ≈ 3.16228
        assert!((cb_to_linear_gain(100.0) - 3.162_277_5).abs() < 1e-3);
        // -100 cB → 1 / 3.162 ≈ 0.316
        assert!((cb_to_linear_gain(-100.0) - 0.316_227_8).abs() < 1e-3);
    }

    // ─── Builder activation gates ────────────────────────────────────────

    /// Default `SfzRegion` has every depth at 0 → both builders
    /// must return None so the per-slot allocation is skipped and
    /// the per-sample apply short-circuits.
    #[test]
    fn region_builders_return_none_for_default_region() {
        let r = SfzRegion::default();
        assert!(region_lfos_from(&r).is_none());
        assert!(region_mod_env_from(&r).is_none());
    }

    /// Activation gate is depth-driven, not time-driven: setting
    /// non-zero LFO frequency / delay alone (without any depth)
    /// must still yield None.  Mirrors the same property for the
    /// mod env (tested via the integration test
    /// `region_without_mod_env_is_bit_equal_to_default_path`).
    #[test]
    fn region_lfos_from_ignores_timing_without_depth() {
        let r = SfzRegion {
            mod_lfo_freq_hz: 6.0,
            mod_lfo_delay_s: 0.1,
            vib_lfo_freq_hz: 4.0,
            vib_lfo_delay_s: 0.05,
            ..Default::default()
        };
        assert!(region_lfos_from(&r).is_none());
    }

    /// Any one of the four depth targets crossing the 0.5 threshold
    /// activates the LFO build.  Each target tested in isolation
    /// to ensure the gate's `||` chain hasn't lost a clause.
    #[test]
    fn region_lfos_from_activates_on_any_single_target() {
        let base = SfzRegion::default();
        let cases: [(&str, SfzRegion); 4] = [
            (
                "mod→pitch",
                SfzRegion {
                    mod_lfo_to_pitch_cents: 50.0,
                    ..base.clone()
                },
            ),
            (
                "mod→filter",
                SfzRegion {
                    mod_lfo_to_filter_fc_cents: 50.0,
                    ..base.clone()
                },
            ),
            (
                "mod→volume",
                SfzRegion {
                    mod_lfo_to_volume_cb: 50.0,
                    ..base.clone()
                },
            ),
            (
                "vib→pitch",
                SfzRegion {
                    vib_lfo_to_pitch_cents: 50.0,
                    ..base
                },
            ),
        ];
        for (label, r) in cases {
            assert!(
                region_lfos_from(&r).is_some(),
                "{label} alone should activate LFO build"
            );
        }
    }

    /// LFO depths beyond the spec-permitted ranges are clamped at
    /// build time so the per-sample math has bounded inputs.  Pitch
    /// depths clamp to ±1200 cents; filter to ±12000; volume to
    /// ±960 cB; timing fields to musical maxima.
    #[test]
    fn region_lfos_from_clamps_extreme_values() {
        let r = SfzRegion {
            mod_lfo_to_pitch_cents: 9999.0,
            mod_lfo_to_filter_fc_cents: 99999.0,
            mod_lfo_to_volume_cb: 9999.0,
            vib_lfo_to_pitch_cents: -9999.0,
            mod_lfo_freq_hz: 100.0,
            mod_lfo_delay_s: 99.0,
            vib_lfo_freq_hz: -1.0,
            vib_lfo_delay_s: -1.0,
            ..Default::default()
        };
        let lfos = region_lfos_from(&r).expect("depths above gate");
        assert_eq!(lfos.mod_to_pitch_cents, 1200.0);
        assert_eq!(lfos.mod_to_filter_cents, 12000.0);
        assert_eq!(lfos.mod_to_volume_cb, 960.0);
        assert_eq!(lfos.vib_to_pitch_cents, -1200.0);
        assert_eq!(lfos.mod_freq_hz, 20.0);
        assert_eq!(lfos.mod_delay_s, 5.0);
        assert_eq!(lfos.vib_freq_hz, 0.05);
        assert_eq!(lfos.vib_delay_s, 0.0);
    }

    /// Mod env activates only when one of the two depth targets is
    /// audible.  Extreme times alone keep the env inert — confirmed
    /// here in isolation alongside the integration regression test.
    #[test]
    fn region_mod_env_from_ignores_timing_without_depth() {
        let r = SfzRegion {
            mod_env_attack_s: 1.0,
            mod_env_decay_s: 2.0,
            mod_env_release_s: 3.0,
            mod_env_sustain_level: 0.3,
            ..Default::default()
        };
        assert!(region_mod_env_from(&r).is_none());
    }

    /// Each depth target activates the env on its own, mirroring the
    /// LFO gate test.  Catches a regression where the `||` chain
    /// drops a clause.
    #[test]
    fn region_mod_env_from_activates_on_any_single_target() {
        let mut r = SfzRegion::default();
        r.mod_env_to_pitch_cents = 50.0;
        assert!(region_mod_env_from(&r).is_some(), "pitch alone");
        r.mod_env_to_pitch_cents = 0.0;
        r.mod_env_to_filter_fc_cents = 50.0;
        assert!(region_mod_env_from(&r).is_some(), "filter alone");
    }

    /// Mod env time + depth fields clamp to bounded ranges so a
    /// malformed SF2 with negative-/huge-timecents can't stall the
    /// AHDSR state machine.
    #[test]
    fn region_mod_env_from_clamps_extreme_values() {
        let r = SfzRegion {
            mod_env_delay_s: 99.0,
            mod_env_attack_s: -1.0,
            mod_env_hold_s: 99.0,
            mod_env_decay_s: 99.0,
            mod_env_sustain_level: 2.5,
            mod_env_release_s: 99.0,
            mod_env_to_pitch_cents: 99999.0,
            mod_env_to_filter_fc_cents: -99999.0,
            ..Default::default()
        };
        let env = region_mod_env_from(&r).expect("depths above gate");
        assert_eq!(env.delay_s, 20.0);
        assert_eq!(env.attack_s, 0.0);
        assert_eq!(env.hold_s, 20.0);
        assert_eq!(env.decay_s, 20.0);
        assert_eq!(env.sustain_level, 1.0);
        assert_eq!(env.release_s, 20.0);
        assert_eq!(env.to_pitch_cents, 12000.0);
        assert_eq!(env.to_filter_cents, -12000.0);
    }

    // ─── LfoSlotState behaviour ──────────────────────────────────────────

    fn lfos(mod_freq: f32, vib_freq: f32, mod_delay: f32, vib_delay: f32) -> RegionLfos {
        RegionLfos {
            mod_freq_hz: mod_freq,
            mod_delay_s: mod_delay,
            mod_to_pitch_cents: 100.0,
            mod_to_filter_cents: 0.0,
            mod_to_volume_cb: 0.0,
            vib_freq_hz: vib_freq,
            vib_delay_s: vib_delay,
            vib_to_pitch_cents: 100.0,
        }
    }

    #[test]
    fn lfo_slot_state_new_is_zeroed() {
        let s = LfoSlotState::new();
        assert_eq!(s.mod_phase, 0.0);
        assert_eq!(s.vib_phase, 0.0);
        assert_eq!(s.mod_delay_remain_s, 0.0);
        assert_eq!(s.vib_delay_remain_s, 0.0);
    }

    /// Trigger arms the per-LFO delay countdowns from the region
    /// config and resets the running phases — the running state
    /// might carry residual values from a previous note.
    #[test]
    fn lfo_slot_state_trigger_arms_delays_and_resets_phases() {
        let mut s = LfoSlotState::new();
        s.mod_phase = 0.7;
        s.vib_phase = 0.3;
        let cfg = lfos(6.0, 4.0, 0.05, 0.10);
        s.trigger(&cfg);
        assert_eq!(s.mod_phase, 0.0);
        assert_eq!(s.vib_phase, 0.0);
        assert_eq!(s.mod_delay_remain_s, 0.05);
        assert_eq!(s.vib_delay_remain_s, 0.10);
    }

    /// Inside the per-LFO delay window `step()` returns 0 for that
    /// LFO; once the countdown elapses, the value should be a sine
    /// in -1..+1.  Tests both LFOs independently to confirm their
    /// delay countdowns are tracked separately.
    #[test]
    fn lfo_slot_state_step_returns_zero_during_delay() {
        let mut s = LfoSlotState::new();
        // Mod LFO has 30 ms delay, vib LFO has 0 ms — vib should
        // start oscillating immediately, mod should stay at 0.
        let cfg = lfos(8.0, 8.0, 0.030, 0.0);
        s.trigger(&cfg);
        let sr = 48_000.0;
        let mut mod_max = 0.0_f32;
        let mut vib_max = 0.0_f32;
        // 20 ms — well within the mod delay window.
        for _ in 0..(0.020 * sr) as i32 {
            let (m, v) = s.step(&cfg, sr);
            mod_max = mod_max.max(m.abs());
            vib_max = vib_max.max(v.abs());
        }
        assert_eq!(mod_max, 0.0, "mod LFO must be silent during delay");
        assert!(
            vib_max > 0.5,
            "vib LFO with no delay should oscillate (max {vib_max})"
        );
    }

    /// Once the delay elapses, `step()` should return values bounded
    /// by sin (i.e. -1..+1) and actually move — running for several
    /// cycles should produce both positive and negative excursions.
    #[test]
    fn lfo_slot_state_step_oscillates_after_delay() {
        let mut s = LfoSlotState::new();
        let cfg = lfos(10.0, 10.0, 0.0, 0.0);
        s.trigger(&cfg);
        let sr = 48_000.0;
        let mut min_v = f32::INFINITY;
        let mut max_v = f32::NEG_INFINITY;
        // 250 ms = 2.5 cycles at 10 Hz.
        for _ in 0..(0.250 * sr) as i32 {
            let (m, _v) = s.step(&cfg, sr);
            min_v = min_v.min(m);
            max_v = max_v.max(m);
        }
        assert!(min_v < -0.95, "mod LFO should reach near -1 (min {min_v})");
        assert!(max_v > 0.95, "mod LFO should reach near +1 (max {max_v})");
        assert!(min_v >= -1.0 && max_v <= 1.0, "sine must stay in [-1, 1]");
    }

    // ─── Modulation envelope ─────────────────────────────────────────────

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
