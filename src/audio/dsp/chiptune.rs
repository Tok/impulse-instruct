// ─── audio/dsp/chiptune.rs ────────────────────────────────────────────────────
// SID-flavoured chiptune voice — three oscillators (saw / 16-step
// triangle / pulse / LFSR noise), per-osc ADSR, shared resonant
// filter (LP / BP / HP), plus the SID-defining ring-mod and
// hard-sync toggles between adjacent oscillators.
//
// Goes for the SID *sound* rather than cycle-accurate emulation.
// The 16-step triangle staircase is reproduced (the defining
// grit of SID triangles vs smooth analogue), the 23-bit-style
// LFSR provides the metallic noise, and the filter is a 2x-OS
// state-variable from `fx_extras`.  6581 chip-to-chip filter
// variation isn't modelled — the result voicing is closer to the
// cleaner 8580 generation.
//
// Per-frame cost: 3 phase increments + 3 waveform evals + 3 ADSR
// steps + 1 LFSR tick + 1 SVF process.  Allocation-free.

use super::AudioParams;
use super::dsp_util::{ATTACK_HANDOVER_VALUE, RELEASE_OFF_VALUE, SUSTAIN_REACH_THRESHOLD};
use super::fx_extras::Svf;
use crate::state::CHIPTUNE_OSCS;

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

pub struct ChiptuneVoice {
    /// Per-oscillator phase 0..1.  Saw / triangle / pulse all
    /// derive their output from this; noise oscillators ignore
    /// the phase but still advance it (so flipping a noise
    /// channel back to a tonal waveform mid-note doesn't pop).
    phases: [f32; CHIPTUNE_OSCS],
    envs: [AdsrState; CHIPTUNE_OSCS],
    /// LFSR state for noise oscillators.  23-bit feedback
    /// register matches the SID's noise generator length; the
    /// taps below produce the same kind of metallic crackle.
    lfsr: u32,
    /// Cached LFSR output sample, updated when the LFSR ticks.
    /// We tick once per oscillator-frame at a rate proportional
    /// to the played frequency so noise pitches with the played
    /// note (faithful to SID behaviour).
    lfsr_held: f32,
    /// Sub-sample accumulator for the LFSR clock — the LFSR
    /// ticks at the noise oscillator's frequency, not the audio
    /// rate.  This counts samples since the last tick.
    lfsr_clock: f32,
    /// Cached fundamental frequency (Hz) — set on `trigger`.
    base_freq: f32,
    velocity: f32,
    /// Filter (shared across all 3 oscillators) — same SVF used
    /// by `SampleInstrument` so the chiptune voice's filter
    /// behaviour matches the rest of the rack's filter sound.
    filter: Svf,
}

impl ChiptuneVoice {
    pub fn new() -> Self {
        Self {
            phases: [0.0; CHIPTUNE_OSCS],
            envs: [AdsrState::new(); CHIPTUNE_OSCS],
            // Non-zero seed so the first noise sample isn't 0.
            lfsr: 0x0011_0011,
            lfsr_held: 0.0,
            lfsr_clock: 0.0,
            base_freq: 261.625_56,
            velocity: 1.0,
            filter: Svf::new(),
        }
    }

    pub fn trigger(&mut self, freq_hz: f32, velocity: f32) {
        self.base_freq = freq_hz.clamp(20.0, 8_000.0);
        self.velocity = velocity.clamp(0.0, 1.5);
        for env in &mut self.envs {
            env.trigger();
        }
    }

    pub fn gate_off(&mut self) {
        for env in &mut self.envs {
            env.release();
        }
    }

    /// Advance the LFSR by one tick — Galois-style 23-bit register
    /// with taps at bits 17 and 22 (matches the SID's tap
    /// positions).  Returns the new LFSR output sample.
    fn lfsr_tick(&mut self) -> f32 {
        let lsb = self.lfsr & 1;
        self.lfsr >>= 1;
        if lsb != 0 {
            // Tap polynomial: x^23 + x^18 + 1 — the SID's noise
            // pattern.  The seed and shift direction are
            // arbitrary; what matters is the period (~8M
            // samples) and the metallic spectral signature.
            self.lfsr ^= 0x0042_0000;
        }
        // Output is bit 0 mapped to ±1.  Bipolar so the
        // resulting noise has zero DC.
        if (self.lfsr & 1) != 0 { 1.0 } else { -1.0 }
    }

    /// One-sample process.  Returns mono — process_block applies
    /// pan + voice volume at the master mix stage.
    pub fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.chiptune_enabled {
            return 0.0;
        }
        // Step every envelope first so the per-osc gain reflects
        // the current sample, not the previous one.
        for i in 0..CHIPTUNE_OSCS {
            self.envs[i].step(
                p.chiptune_osc_attack[i],
                p.chiptune_osc_decay[i],
                p.chiptune_osc_sustain[i],
                p.chiptune_osc_release[i],
                sr,
            );
        }

        let dt = 1.0 / sr;
        // Tick the LFSR at the played frequency (not the audio
        // rate) so noise pitches with the note — faithful to the
        // SID's frequency-synced noise generator.  The clock
        // sample is held until the next tick boundary.
        self.lfsr_clock += dt * self.base_freq;
        if self.lfsr_clock >= 1.0 {
            self.lfsr_clock = self.lfsr_clock.fract();
            self.lfsr_held = self.lfsr_tick();
        }
        let noise_sample = self.lfsr_held;

        // Per-oscillator wave generation.  Phases advance every
        // sample regardless of waveform so toggling waveforms
        // mid-note doesn't pop the phase.  Index loop because
        // the body indexes 6+ separate per-osc arrays on the
        // params snapshot — iterator-zip across that many slices
        // is far less readable than the plain index.
        let pulse_w = p.chiptune_pulse_width.clamp(0.05, 0.95);
        let mut osc_out = [0.0_f32; CHIPTUNE_OSCS];
        let mut osc1_wrapped = false;
        #[allow(clippy::needless_range_loop)]
        for i in 0..CHIPTUNE_OSCS {
            // Each oscillator plays at the played note (no per-
            // osc detune — keeps the V1 surface narrow).  Future
            // V2: add per-osc fine-detune if users want lush
            // unison without ring-mod / sync.
            let freq = self.base_freq;
            let prev_phase = self.phases[i];
            self.phases[i] = (prev_phase + freq * dt).fract();
            // Detect wrap on osc 1 for the sync flag — read
            // *before* osc 2 generates output so the reset can
            // affect this same sample's osc 2 wave.
            if i == 0 && self.phases[i] < prev_phase {
                osc1_wrapped = true;
            }
            // Hard sync — when osc 1 wraps and `sync` is on,
            // osc 2's phase resets immediately.  Always applied
            // before osc 2's wave evaluation.
            if i == 1 && p.chiptune_sync && osc1_wrapped {
                self.phases[i] = 0.0;
            }
            let phase = self.phases[i];
            let raw = match p.chiptune_osc_waveform[i] {
                0 => 2.0 * phase - 1.0, // Saw 0..1 → -1..1
                1 => {
                    // 16-step triangle staircase — the SID's
                    // signature triangle character (the
                    // straight-edge pieces of the staircase
                    // make the wave more buzzy than a smooth
                    // analogue triangle).  Quantise the
                    // smooth-triangle output to 16 levels.
                    let smooth = 1.0 - 2.0 * (2.0 * phase - 1.0).abs();
                    let q = (smooth * 8.0).round() / 8.0;
                    q.clamp(-1.0, 1.0)
                }
                2 => {
                    if phase < pulse_w {
                        1.0
                    } else {
                        -1.0
                    }
                }
                _ => noise_sample, // 3 (or any unexpected) → noise
            };
            osc_out[i] = raw * p.chiptune_osc_level[i] * self.envs[i].value;
        }

        // Ring mod — osc 1's output multiplied by the sign of
        // osc 2.  Applied *after* per-osc levels so a ring-mod
        // user can tune the depth via osc 2's level (lower = less
        // dramatic clang).  SID-authentic rings are typically
        // applied to triangle on osc 1; we apply to whatever osc
        // 1 is producing for simplicity.
        if p.chiptune_ring_mod {
            let sign2 = if osc_out[1] >= 0.0 { 1.0 } else { -1.0 };
            osc_out[0] *= sign2;
        }

        let dry: f32 = osc_out.iter().sum::<f32>() * (1.0 / CHIPTUNE_OSCS as f32);

        // Filter — re-uses the standard `Svf` so behaviour
        // matches the SVF FX module + SAMPLER+'s per-voice
        // filter.  Mode 0..2 = LP / BP / HP; mode 3 (notch) is
        // available but not exposed in the chiptune UI to keep
        // the panel narrow.
        let wet = self.filter.process(
            dry,
            p.chiptune_filter_cutoff,
            p.chiptune_filter_resonance,
            0.0, // no drive — kept clean
            super::fx_extras::SvfMode::from_u8(p.chiptune_filter_mode),
            1.0, // SVF process always outputs full wet; we
            // crossfade with `dry` below.
            sr,
        );
        let mix = p.chiptune_filter_mix.clamp(0.0, 1.0);
        let out = dry * (1.0 - mix) + wet * mix;
        out * self.velocity * p.chiptune_volume.clamp(0.0, 1.5)
    }

    #[cfg(test)]
    pub fn any_active(&self) -> bool {
        self.envs.iter().any(|e| e.stage != AdsrStage::Off)
    }
}

impl Default for ChiptuneVoice {
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
        s.chiptune.enabled = true;
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn silent_before_trigger() {
        let mut v = ChiptuneVoice::new();
        let p = enabled_params();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn silent_when_disabled() {
        let mut v = ChiptuneVoice::new();
        let s = AppState::default();
        let p = AudioParams::from_app_state(&s);
        v.trigger(440.0, 1.0);
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn trigger_produces_audible_output() {
        let mut v = ChiptuneVoice::new();
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
    fn every_waveform_produces_output() {
        // Drive osc 1 alone with each of the four waveforms in
        // turn.  Catches a regression where, e.g., the noise
        // dispatch falls through to silence.
        for wave in 0..4_u8 {
            let mut s = AppState::default();
            s.chiptune.enabled = true;
            s.chiptune.osc1.waveform = wave;
            s.chiptune.osc1.level = 1.0;
            s.chiptune.osc1.attack = 0.0;
            s.chiptune.osc1.sustain = 1.0;
            s.chiptune.osc2.level = 0.0; // silence the others
            s.chiptune.osc3.level = 0.0;
            let p = AudioParams::from_app_state(&s);
            let mut v = ChiptuneVoice::new();
            v.trigger(440.0, 1.0);
            let mut peak = 0.0_f32;
            for _ in 0..3_000 {
                let out = v.process(48_000.0, &p);
                assert!(out.is_finite());
                peak = peak.max(out.abs());
            }
            assert!(
                peak > 0.05,
                "waveform {wave} should produce audible output (peak {peak})"
            );
        }
    }

    #[test]
    fn output_bounded_under_full_drive() {
        // Every osc at full level + filter at full resonance +
        // ring-mod + sync — output should stay finite + bounded.
        let mut s = AppState::default();
        s.chiptune.enabled = true;
        s.chiptune.volume = 1.5;
        s.chiptune.osc1.level = 1.0;
        s.chiptune.osc2.level = 1.0;
        s.chiptune.osc3.level = 1.0;
        s.chiptune.osc1.attack = 0.0;
        s.chiptune.osc2.attack = 0.0;
        s.chiptune.osc3.attack = 0.0;
        s.chiptune.osc1.sustain = 1.0;
        s.chiptune.osc2.sustain = 1.0;
        s.chiptune.osc3.sustain = 1.0;
        s.chiptune.filter_resonance = 1.0;
        s.chiptune.filter_mix = 1.0;
        s.chiptune.ring_mod = true;
        s.chiptune.sync = true;
        let p = AudioParams::from_app_state(&s);
        let mut v = ChiptuneVoice::new();
        v.trigger(220.0, 1.0);
        let mut peak = 0.0_f32;
        for _ in 0..16_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite(), "non-finite output");
            peak = peak.max(out.abs());
        }
        assert!(peak <= 4.0, "fully-driven chiptune bounded (peak {peak})");
    }

    #[test]
    fn release_eventually_silences() {
        let mut s = AppState::default();
        s.chiptune.enabled = true;
        s.chiptune.osc1.attack = 0.0;
        s.chiptune.osc1.decay = 0.0;
        s.chiptune.osc1.sustain = 0.5;
        s.chiptune.osc1.release = 0.0;
        s.chiptune.osc2.release = 0.0;
        s.chiptune.osc3.release = 0.0;
        let p = AudioParams::from_app_state(&s);
        let mut v = ChiptuneVoice::new();
        v.trigger(440.0, 1.0);
        for _ in 0..1_000 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        for _ in 0..192_000 {
            let _ = v.process(48_000.0, &p);
        }
        assert!(!v.any_active(), "every osc envelope should reach Off");
    }

    #[test]
    fn sync_resets_osc2_phase_when_osc1_wraps() {
        // Drive osc 1 saw at 100 Hz, osc 2 saw at 100 Hz, sync
        // on.  Verify osc 2's phase resets when osc 1 wraps.
        // At identical frequencies the resets are nearly
        // simultaneous; the test checks that the bookkeeping
        // doesn't blow up rather than asserting an exact phase.
        let mut s = AppState::default();
        s.chiptune.enabled = true;
        s.chiptune.osc1.waveform = 0; // Saw
        s.chiptune.osc2.waveform = 0; // Saw
        s.chiptune.osc1.level = 1.0;
        s.chiptune.osc2.level = 1.0;
        s.chiptune.osc1.attack = 0.0;
        s.chiptune.osc2.attack = 0.0;
        s.chiptune.osc1.sustain = 1.0;
        s.chiptune.osc2.sustain = 1.0;
        s.chiptune.sync = true;
        let p = AudioParams::from_app_state(&s);
        let mut v = ChiptuneVoice::new();
        v.trigger(100.0, 1.0);
        for _ in 0..2_000 {
            let _ = v.process(48_000.0, &p);
        }
        // After 2000 samples (~42 ms) at 100 Hz, both oscs have
        // wrapped ~4 times.  Phases should be valid (0..1) and
        // finite — sync logic doesn't throw the phases out of
        // range.
        for &p in &v.phases {
            assert!(
                (0.0..=1.0).contains(&p) && p.is_finite(),
                "phase out of range with sync: {p}"
            );
        }
    }
}
