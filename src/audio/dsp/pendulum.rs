// ─── audio/dsp/pendulum.rs ────────────────────────────────────────────────────
// Pendulum voice — two near-tuned sine oscillators that beat
// acoustically.  Drone voice; no sequencer trigger, knob-driven only.
//
// Algorithm (per sample):
//   1. Map base_pitch → log frequency 30–800 Hz.
//   2. Compute osc1 freq = base, osc2 freq = base + detune_hz.
//   3. Generate sin(phase1) + sin(phase2) with mix balance.
//   4. The constructive / destructive interference between the two
//      oscillators *is* the beat — no separate amplitude modulator;
//      the sum is the effect.
//
// `beat_rate_hz()` returns the live beat rate so the panel can show
// a readout — same f32 the user sees scaled directly to Hz.

use super::AudioParams;

pub(super) struct PendulumVoice {
    /// Phase 0..1 of oscillator 1 — rolls over each cycle.
    phase1: f32,
    /// Phase of oscillator 2 — kept independent of phase1 so the
    /// detune-driven interference is sample-accurate.
    phase2: f32,
}

impl PendulumVoice {
    pub(super) fn new() -> Self {
        Self {
            phase1: 0.0,
            // Slight phase offset on osc2 so the very first sample
            // already shows partial interference (the user hears
            // the beat from t=0 instead of waiting one period).
            phase2: 0.25,
        }
    }

    pub(super) fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.pendulum_enabled {
            return 0.0;
        }

        // Map base_pitch ∈ [0, 1] → log frequency 30–800 Hz.
        // log2(800/30) ≈ 4.74 octaves, comfortable for the
        // beating effect (very-low and mid-range fundamentals).
        let base_hz = 30.0 * (800.0_f32 / 30.0).powf(p.pendulum_base_pitch.clamp(0.0, 1.0));

        // Detune knob 0..1 → 0..60 Hz separation.  Above ~10 Hz
        // the difference becomes its own subaudible tone — that's
        // the inharmonic-drone end of the range.
        let detune = p.pendulum_detune_hz.clamp(0.0, 1.0) * 60.0;
        let f1 = base_hz;
        let f2 = (base_hz + detune).max(0.0);

        // Advance phases independently.
        self.phase1 += f1 / sr;
        if self.phase1 >= 1.0 {
            self.phase1 -= 1.0;
        }
        self.phase2 += f2 / sr;
        if self.phase2 >= 1.0 {
            self.phase2 -= 1.0;
        }

        let tau = std::f32::consts::TAU;
        let osc1 = (self.phase1 * tau).sin();
        let osc2 = (self.phase2 * tau).sin();

        // Mix knob 0..1 → osc1 / osc2 balance.  0 = osc1 only,
        // 1 = osc2 only, 0.5 = equal (deepest beat amplitude).
        // Linear crossfade — phase relationships matter more
        // than equal-power for this effect.
        let mix = p.pendulum_mix.clamp(0.0, 1.0);
        let blended = osc1 * (1.0 - mix) + osc2 * mix;
        // Doubled mix at 0.5 gives near-unity gain (sin + sin
        // can sum to 2 when in phase, but the average power is
        // half).  Scaling by 0.5 keeps peaks well under ±1.
        blended * p.pendulum_volume.clamp(0.0, 1.5) * 0.5
    }
}

impl Default for PendulumVoice {
    fn default() -> Self {
        Self::new()
    }
}

/// Live beat rate (Hz) for the panel's readout.  Pure mapping —
/// `detune_hz_knob ∈ [0, 1]` → `0..60 Hz` — the same scale the DSP
/// uses, exposed so the UI can render the readout without poking at
/// internal voice state.
pub fn beat_rate_hz(detune_knob: f32) -> f32 {
    detune_knob.clamp(0.0, 1.0) * 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> AudioParams {
        let s = crate::state::AppState::default();
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn pendulum_silent_when_disabled() {
        let mut v = PendulumVoice::new();
        let mut p = default_params();
        p.pendulum_enabled = false;
        for _ in 0..256 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn pendulum_produces_audio_when_enabled() {
        let mut v = PendulumVoice::new();
        let mut p = default_params();
        p.pendulum_enabled = true;
        p.pendulum_volume = 1.0;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            peak = peak.max(v.process(48_000.0, &p).abs());
        }
        assert!(peak > 0.05, "should be audible (peak {peak})");
        assert!(peak <= 1.0, "should not run away (peak {peak})");
    }

    #[test]
    fn pendulum_detune_zero_collapses_to_single_oscillator() {
        // With detune = 0 both oscillators tune to base_hz.  The
        // peak of osc1+osc2 (each ±1) is bounded by ±2; with
        // mix=0.5 and the *0.5 final scale, the peak settles at
        // ±0.5.  Verify the path doesn't overflow.
        let mut v = PendulumVoice::new();
        let mut p = default_params();
        p.pendulum_enabled = true;
        p.pendulum_detune_hz = 0.0;
        p.pendulum_mix = 0.5;
        p.pendulum_volume = 1.0;
        let mut peak = 0.0_f32;
        for _ in 0..1024 {
            peak = peak.max(v.process(48_000.0, &p).abs());
        }
        assert!(peak <= 0.6, "no-detune peak should stay near 0.5 (got {peak})");
    }

    #[test]
    fn beat_rate_hz_matches_dsp_scale() {
        // The readout helper must use the same 0..60 Hz scale
        // the DSP does — otherwise the panel display lies about
        // the actual beat rate.
        assert!((beat_rate_hz(0.0) - 0.0).abs() < 1e-5);
        assert!((beat_rate_hz(0.5) - 30.0).abs() < 1e-5);
        assert!((beat_rate_hz(1.0) - 60.0).abs() < 1e-5);
        // Defensive clamping on out-of-range inputs.
        assert!((beat_rate_hz(-1.0) - 0.0).abs() < 1e-5);
        assert!((beat_rate_hz(2.0) - 60.0).abs() < 1e-5);
    }
}
