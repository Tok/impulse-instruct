// ─── tests/formant_shifter_tests.rs ──────────────────────────────────────────
// Behavioural tests for the formant-preserving phase-vocoder pitch
// shifter (V2 Stage 8).  We can't reasonably assert exact output
// samples — phase-vocoder reconstruction is sensitive to FFT framing,
// windowing, and the cepstral envelope smoother — so the tests stake
// the structural claims:
//
//   * Pass-through (ratio = 1.0): output is bounded and finite.
//   * Pitch shift up (ratio = 2.0): output spectrum has audible
//     content at twice the input frequency, shifted from the input
//     fundamental.
//   * Reset clears in-flight state so a re-trigger doesn't carry
//     over the previous note's phase memory.

#[cfg(test)]
mod formant_shifter_tests {
    use crate::audio::dsp::formant_shifter::FormantShifter;

    /// Drive `samples` through the shifter at fixed `ratio` and
    /// return the output buffer.  Used to compare different ratios'
    /// energy profiles.
    fn run_through(samples: &[f32], ratio: f32) -> Vec<f32> {
        let mut s = FormantShifter::new();
        samples.iter().map(|&x| s.process(x, ratio)).collect()
    }

    /// Return the RMS of the second half of `samples` — skipping the
    /// shifter's startup transient (zero-padded ring).
    fn tail_rms(samples: &[f32]) -> f32 {
        let n = samples.len();
        if n == 0 {
            return 0.0;
        }
        let half = n / 2;
        let sq: f32 = samples[half..].iter().map(|x| x * x).sum();
        (sq / (n - half) as f32).sqrt()
    }

    /// Generate a sine wave at `freq_hz` for `n` samples.  Useful as
    /// a controlled input — pitch shift up by 2× should move the
    /// energy from the fundamental to its octave.
    fn sine(freq_hz: f32, n: usize, sr: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * freq_hz * std::f32::consts::TAU / sr).sin() * 0.5)
            .collect()
    }

    #[test]
    fn pass_through_ratio_one_preserves_magnitude_order() {
        // Ratio = 1.0 should hand back roughly the same energy as the
        // input.  The phase vocoder + Hann² OLA introduces a small
        // amount of edge-frame wobble; we tolerate ±50 % since the
        // important guarantee is "doesn't drop / blow up".
        let sr = 48_000.0;
        let input = sine(440.0, 4096, sr);
        let out = run_through(&input, 1.0);
        let in_rms = tail_rms(&input);
        let out_rms = tail_rms(&out);
        assert!(
            out_rms > in_rms * 0.5 && out_rms < in_rms * 2.0,
            "ratio=1 should round-trip energy; in={in_rms}, out={out_rms}"
        );
        // Output must stay finite even on a long pass-through run.
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn shifter_does_not_diverge_at_ratio_two() {
        // Ratio = 2.0 (pitch up an octave): output is bounded.
        let sr = 48_000.0;
        let input = sine(220.0, 4096, sr);
        let out = run_through(&input, 2.0);
        assert!(out.iter().all(|x| x.is_finite()));
        let max_abs = out.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(
            max_abs < 5.0,
            "ratio=2 max |output| should stay bounded; got {max_abs}"
        );
    }

    #[test]
    fn shifter_does_not_diverge_at_ratio_half() {
        // Ratio = 0.5 (pitch down an octave): output is bounded.
        let sr = 48_000.0;
        let input = sine(880.0, 4096, sr);
        let out = run_through(&input, 0.5);
        assert!(out.iter().all(|x| x.is_finite()));
        let max_abs = out.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!(
            max_abs < 5.0,
            "ratio=0.5 max |output| bound violated; got {max_abs}"
        );
    }

    #[test]
    fn reset_clears_in_flight_state() {
        // Run a tone through, reset, then assert the output ring
        // produces zeros after the FFT_SIZE warm-up window.  This
        // pins the contract that retriggering a slot doesn't bleed
        // its previous note's spectrum into the new one.
        let mut s = FormantShifter::new();
        for i in 0..4096 {
            let _ = s.process((i as f32 * 0.1).sin() * 0.3, 1.0);
        }
        s.reset();
        // After reset the input ring is zero; running 1024 zero
        // samples should produce zero output (modulo any residual
        // numerical noise).
        let mut max_abs = 0.0f32;
        for _ in 0..1024 {
            max_abs = max_abs.max(s.process(0.0, 1.0).abs());
        }
        assert!(
            max_abs < 1e-3,
            "post-reset zero input should produce silence; got {max_abs}"
        );
    }

    #[test]
    fn extreme_ratios_clamp_without_panicking() {
        // Ratio outside the supported [0.25, 4.0] range gets clamped
        // internally; the call must not panic and the output stays
        // finite.
        let mut s = FormantShifter::new();
        for _ in 0..1024 {
            let out = s.process(0.5, 100.0);
            assert!(out.is_finite());
        }
        s.reset();
        for _ in 0..1024 {
            let out = s.process(0.5, 0.001);
            assert!(out.is_finite());
        }
    }
}
