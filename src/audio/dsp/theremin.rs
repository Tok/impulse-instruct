// ─── audio/dsp/theremin.rs ────────────────────────────────────────────────────
// Theremin voice — XY-pad-driven sine oscillator with portamento
// glide and odd-harmonic brightness.  Absurd-queue voice; no
// sequencer trigger — the player drags the pad on the panel and the
// audio thread interpolates pitch / gain to match.
//
// Algorithm:
//   1. Map x ∈ [0, 1] → log frequency 50–2000 Hz.
//   2. One-pole-smooth that target into `pitch_smooth_hz` so dragging
//      the pad doesn't click — portamento knob sets the time constant.
//   3. Same smoothing on the gain (y) but with a fixed short tau so
//      the player can articulate quickly without per-knob latency.
//   4. Generate fundamental sine + 3rd + 5th odd harmonics scaled by
//      the brightness knob.  The odd harmonics are what give a real
//      Theremin its "talking" overtone character at high volumes.

use super::AudioParams;

/// Fixed gain-smoother time constant (~10 ms).  Independent of the
/// portamento knob so that abrupt volume changes (a quick pad-flick
/// articulation) don't get smeared by a long glide; pitch keeps the
/// expressive glide, gain stays responsive.
const GAIN_SMOOTH_HZ: f32 = 100.0;

pub(super) struct ThereminVoice {
    /// Oscillator phase 0..1 — rolls over each cycle.
    phase: f32,
    /// Smoothed target frequency (Hz).  Starts at the rest pitch so
    /// the first sample after enabling doesn't click.
    pitch_smooth_hz: f32,
    /// Smoothed output gain.  Starts at 0 so a freshly enabled voice
    /// doesn't snap to whatever the y knob currently reads.
    gain_smooth: f32,
}

impl ThereminVoice {
    pub(super) fn new() -> Self {
        Self {
            phase: 0.0,
            pitch_smooth_hz: 440.0,
            gain_smooth: 0.0,
        }
    }

    pub(super) fn process(&mut self, sr: f32, p: &AudioParams) -> f32 {
        if !p.theremin_enabled {
            // Quietly relax state when disabled so re-enabling
            // doesn't pop on whatever was loaded last.  Cheap: two
            // multiplies per sample.
            self.gain_smooth *= 0.999;
            return 0.0;
        }

        // Map x → log frequency 50–2000 Hz.  log2(2000/50) ≈ 5.32
        // octaves — close to a real Theremin's 5-octave range.
        let target_hz = 50.0 * 40.0_f32.powf(p.theremin_x.clamp(0.0, 1.0));

        // Portamento smoothing.  Knob 0 → ~1 ms (effectively snap);
        // knob 1 → ~500 ms (long glissando).  exp() once per sample
        // is fine — single oscillator, no allocation.
        let porta_t = 0.001 + p.theremin_portamento.clamp(0.0, 1.0) * 0.5;
        let pitch_coef = (-1.0_f32 / (porta_t * sr)).exp();
        self.pitch_smooth_hz = target_hz + (self.pitch_smooth_hz - target_hz) * pitch_coef;

        // Gain smoothing — fixed-tau lowpass so the player can
        // articulate quickly without the portamento knob affecting
        // it.  Same one-pole shape.
        let gain_coef = (-std::f32::consts::TAU * GAIN_SMOOTH_HZ / sr).exp();
        let target_gain = p.theremin_y.clamp(0.0, 1.0);
        self.gain_smooth = target_gain + (self.gain_smooth - target_gain) * gain_coef;

        // Advance phase; wrap.
        self.phase += self.pitch_smooth_hz / sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // Fundamental + odd harmonics.  Brightness scales the
        // harmonics; at 0 the voice is a pure sine.  Coefficients
        // chosen so peak amplitude stays within ±1 even at full
        // brightness (1.0 + 0.3 + 0.15 = 1.45 worst-case sum, but
        // they don't all peak in phase together — the constructive
        // interference falls below ±1.2 in practice; the master
        // mix clamps anyway).
        let tau = std::f32::consts::TAU;
        let bright = p.theremin_brightness.clamp(0.0, 1.0);
        let fundamental = (self.phase * tau).sin();
        let third = (self.phase * 3.0 * tau).sin() * bright * 0.30;
        let fifth = (self.phase * 5.0 * tau).sin() * bright * 0.15;
        let osc = fundamental + third + fifth;

        osc * self.gain_smooth * p.theremin_volume.clamp(0.0, 1.5) * 0.5
    }
}

impl Default for ThereminVoice {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an `AudioParams` with sensible Theremin defaults so
    /// each test only needs to override the field it cares about.
    /// Uses `from_app_state(&AppState::default())` to fill the
    /// non-Theremin fields with their real defaults.
    fn default_params() -> AudioParams {
        let s = crate::state::AppState::default();
        AudioParams::from_app_state(&s)
    }

    #[test]
    fn theremin_silent_when_disabled() {
        let mut v = ThereminVoice::new();
        let mut p = default_params();
        p.theremin_enabled = false;
        p.theremin_y = 1.0; // would be loud if enabled
        let mut peak = 0.0_f32;
        for _ in 0..256 {
            peak = peak.max(v.process(48_000.0, &p).abs());
        }
        assert!(
            peak < 1e-4,
            "disabled Theremin should be silent (peak {peak})"
        );
    }

    #[test]
    fn theremin_silent_when_y_zero() {
        // y = 0 means the player's "volume hand" is on the antenna —
        // output should drop to 0 (after the gain smoother settles).
        let mut v = ThereminVoice::new();
        let mut p = default_params();
        p.theremin_enabled = true;
        p.theremin_y = 0.0;
        // Run long enough for the gain smoother to settle.
        for _ in 0..48_000 {
            v.process(48_000.0, &p);
        }
        let next = v.process(48_000.0, &p);
        assert!(next.abs() < 1e-3, "y=0 should be ~silent (got {next})");
    }

    #[test]
    fn theremin_reaches_target_frequency_with_short_portamento() {
        // With portamento ~= 0 the smoother should converge in a
        // handful of samples.  Verify by checking the smoothed
        // pitch lands within 1 % of the target after running for
        // 100 ms at 48 kHz.
        let mut v = ThereminVoice::new();
        let mut p = default_params();
        p.theremin_enabled = true;
        p.theremin_portamento = 0.0; // ~1 ms tau
        p.theremin_x = 0.5; // 50 * 40^0.5 ≈ 316.23 Hz
        for _ in 0..(48_000 / 10) {
            v.process(48_000.0, &p);
        }
        let target = 50.0 * 40.0_f32.powf(0.5);
        let drift = (v.pitch_smooth_hz - target).abs() / target;
        assert!(
            drift < 0.01,
            "pitch should converge with short portamento; drift {drift}"
        );
    }

    #[test]
    fn theremin_produces_audio_when_x_y_set() {
        // Smoke test: enabled, mid-range x, full y, no portamento —
        // we should observe non-zero output within the first second.
        let mut v = ThereminVoice::new();
        let mut p = default_params();
        p.theremin_enabled = true;
        p.theremin_x = 0.5;
        p.theremin_y = 1.0;
        p.theremin_portamento = 0.0;
        let mut peak = 0.0_f32;
        for _ in 0..48_000 {
            peak = peak.max(v.process(48_000.0, &p).abs());
        }
        assert!(peak > 0.1, "should make audible sound (peak {peak})");
        assert!(peak <= 1.5, "should not run away (peak {peak})");
    }
}
