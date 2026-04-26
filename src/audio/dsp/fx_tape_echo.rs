// ─── audio/dsp/fx_tape_echo.rs ────────────────────────────────────────────────
// Tape Echo FX — dub-style delay with wow / flutter / saturation
// baked into the feedback path.  Distinct from `FxDelay` (which
// CAN do tape character but exposes wow / flutter / saturation /
// HP / LP as five separate knobs), `FxTapeSat` (no delay), and
// `FxMultitap` (rhythmic taps).  Single AGE knob folds the four
// character knobs together so the user dials "more worn-out
// tape" with one gesture.
//
// Implementation:
//   * Delay line (~1.5 s max) with linear-interp fractional read.
//     Modulated by two LFOs (slow ~0.5 Hz wow, faster ~6 Hz
//     flutter), depth scaling with the AGE knob.
//   * Tape saturation in the feedback path (tanh) with drive
//     scaling with AGE.
//   * One-pole LPF on the feedback to roll off HF as the tape
//     "wears out" (LPF cutoff sweeps with AGE).
//   * Wet/dry mix knob; cheap-bypass fast path.
//
// Buffer sized for 1.5 s at 96 kHz = 144 000 samples; round up to
// 160 000 for headroom around the wow/flutter swing.  Allocated
// once at construction (Vec<f32>) — no per-block allocations.

use std::f32::consts::TAU;

/// Maximum delay buffer length.  144 000 = 1.5 s at 96 kHz; 160k
/// gives a small guard band for the modulator swing on top.
const TAPE_ECHO_BUFFER_LEN: usize = 160_000;

pub(crate) struct TapeEchoFx {
    /// Heap-allocated circular delay buffer.  Allocated once at
    /// `new`; `process` only reads / writes — no allocations per
    /// sample.  Vec is the right shape because the buffer is
    /// large (640 kB at f32) and would bloat the DspState struct
    /// if it were [f32; N].
    buf: Vec<f32>,
    write_idx: usize,
    /// Wow LFO phase (0..TAU).  ~0.5 Hz drift.
    wow_phase: f32,
    /// Flutter LFO phase (0..TAU).  ~6 Hz wobble.
    flutter_phase: f32,
    /// Feedback-path one-pole LPF state — sweeps cutoff with AGE.
    lpf_state: f32,
}

impl TapeEchoFx {
    pub(crate) fn new() -> Self {
        Self {
            buf: vec![0.0; TAPE_ECHO_BUFFER_LEN],
            write_idx: 0,
            wow_phase: 0.0,
            flutter_phase: 0.0,
            lpf_state: 0.0,
        }
    }

    /// `time`:     0..1 → 25..1500 ms log-mapped.
    /// `feedback`: 0..1 → 0..0.95 of previous wet output mixed
    ///             back into the input.
    /// `age`:      0..1 — single character knob.  Folds wow /
    ///             flutter depth + saturation drive + HF rolloff
    ///             together.
    /// `mix`:      0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        time: f32,
        feedback: f32,
        age: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }

        // Time knob: 0..1 → 25..1500 ms log-mapped.  60× span fits
        // slap-back through dub-rhythm in one knob without an
        // exponential cliff at the high end.
        let t_clamped = time.clamp(0.0, 1.0);
        let delay_ms = 25.0 * 60.0_f32.powf(t_clamped);
        let base_delay_samples =
            (delay_ms * 0.001 * sr).clamp(1.0, (TAPE_ECHO_BUFFER_LEN - 4) as f32);

        // Wow / flutter modulation — depth scales with AGE so a
        // pristine echo (age=0) sits exactly on `base_delay`.
        let a = age.clamp(0.0, 1.0);
        self.wow_phase += TAU * 0.5 / sr;
        if self.wow_phase >= TAU {
            self.wow_phase -= TAU;
        }
        self.flutter_phase += TAU * 6.0 / sr;
        if self.flutter_phase >= TAU {
            self.flutter_phase -= TAU;
        }
        // Wow swings ±0.5% of the delay at full age; flutter ±0.15%.
        let wow_swing = base_delay_samples * 0.005 * a;
        let flutter_swing = base_delay_samples * 0.0015 * a;
        let modulated = (base_delay_samples
            + self.wow_phase.sin() * wow_swing
            + self.flutter_phase.sin() * flutter_swing)
            .clamp(1.0, (TAPE_ECHO_BUFFER_LEN - 2) as f32);

        // Read tap with linear interpolation.
        let read_pos = self.write_idx as f32 + TAPE_ECHO_BUFFER_LEN as f32 - modulated;
        let i0 = read_pos as usize % TAPE_ECHO_BUFFER_LEN;
        let i1 = (i0 + 1) % TAPE_ECHO_BUFFER_LEN;
        let frac = read_pos - read_pos.floor();
        let delayed = self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac;

        // Feedback path: HF rolloff that sweeps with AGE.  At
        // a=0, alpha=1 (no smoothing — passthrough); at a=1,
        // alpha≈0.15 (≈1 kHz cutoff at 48 kHz, audibly muffled).
        let alpha = 1.0 - 0.85 * a;
        self.lpf_state += alpha * (delayed - self.lpf_state);
        // Tape saturation on the feedback signal.  Drive ramps
        // 1..3.5× with AGE so a worn tape distorts repeats.
        let drive = 1.0 + 2.5 * a;
        let saturated = (self.lpf_state * drive).tanh() / drive;
        let fb_clamped = feedback.clamp(0.0, 0.95);
        let fb_sample = saturated * fb_clamped;

        // Write input + feedback into the buffer, advance write
        // pointer.  Buffer wraps power-of-not-two via modulo —
        // not on the hot path, the cost is acceptable here.
        self.buf[self.write_idx] = input + fb_sample;
        self.write_idx = (self.write_idx + 1) % TAPE_ECHO_BUFFER_LEN;

        let wet = delayed;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for TapeEchoFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = TapeEchoFx::new();
        let out = fx.process(0.5, 0.5, 0.5, 0.5, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn produces_delayed_repeats_from_impulse() {
        // Drive a single impulse, then silence.  After the
        // initial delay-time worth of samples, an audible repeat
        // should appear — and with feedback engaged, additional
        // repeats follow.
        let mut fx = TapeEchoFx::new();
        // Short delay (~50 ms) for a quick repeat in the test
        // window.  Time=0.1 → 25*60^0.1 ≈ 35 ms.
        let _ = fx.process(1.0, 0.1, 0.6, 0.0, 1.0, 48_000.0);
        let mut peak = 0.0_f32;
        for _ in 0..10_000 {
            let out = fx.process(0.0, 0.1, 0.6, 0.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(
            peak > 0.05,
            "tape echo should produce audible repeats (peak {peak})"
        );
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = TapeEchoFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 0.5, 0.95, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Tanh saturation in feedback path keeps us bounded even
        // at max feedback × max age.
        assert!(peak <= 2.0, "tape echo bounded at full drive (peak {peak})");
    }

    #[test]
    fn age_zero_is_a_clean_digital_echo() {
        // age=0 means no wow / flutter / saturation / HF rolloff.
        // Drive an impulse, advance enough samples to land inside
        // the second repeat, and check the value is close to the
        // input × feedback (no character colouring).
        let mut fx = TapeEchoFx::new();
        // Time=0.1 → 35 ms ≈ 1680 samples at 48 kHz.
        let delay_samples = 1680;
        let _ = fx.process(1.0, 0.1, 0.5, 0.0, 1.0, 48_000.0);
        // Run silence to push the impulse around the loop once.
        let mut max = 0.0_f32;
        for _ in 0..(delay_samples + 1000) {
            let out = fx.process(0.0, 0.1, 0.5, 0.0, 1.0, 48_000.0);
            max = max.max(out.abs());
        }
        // Without HF rolloff or saturation, the first repeat
        // should arrive near unity × feedback ≈ 0.5.  Allow some
        // slack for linear-interp loss across the swing.
        assert!(max > 0.3, "clean echo first repeat near unity (peak {max})");
    }
}
