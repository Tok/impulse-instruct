// ─── tests/dsp_fx_tests.rs ───────────────────────────────────────────────────
// Direct unit tests for the FX-block primitives in audio/dsp/fx.rs:
// Biquad coefficient factories (low_shelf / high_shelf / peak),
// the EqBands wrapper, and Compressor::compress_band's threshold +
// ratio behaviour.  Most assertions are closed-form (DC gain ≈ 10^(g/20),
// 0 dB → unity passthrough, sub-threshold input passes unchanged).
//
// Split out of dsp_tests.rs to keep both files comfortably under the
// 1000-line cap.

#[cfg(test)]
mod biquad_low_shelf_tests {
    use crate::audio::dsp::fx::Biquad;

    /// Drive the biquad with a DC input until it settles, then return
    /// the steady-state output.  Useful for measuring DC gain
    /// regardless of attack transient.
    fn dc_gain(mut b: Biquad) -> f32 {
        let mut last = 0.0;
        for _ in 0..2000 {
            last = b.process(1.0);
        }
        last
    }

    /// Run a half-sample-rate (Nyquist) input — alternating ±1 — and
    /// return the peak output magnitude after settling.
    fn nyquist_peak(mut b: Biquad) -> f32 {
        // Burn-in.
        for i in 0..2000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let _ = b.process(x);
        }
        let mut peak = 0.0_f32;
        for i in 0..200 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            peak = peak.max(b.process(x).abs());
        }
        peak
    }

    #[test]
    fn zero_db_low_shelf_is_near_unity_passthrough_at_dc() {
        let g = dc_gain(Biquad::low_shelf(200.0, 0.0, 44100.0));
        assert!((g - 1.0).abs() < 1e-3, "expected DC≈1, got {g}");
    }

    #[test]
    fn positive_low_shelf_boosts_dc() {
        // +12 dB low shelf at 200 Hz: DC gain should be ~3.98 (10^0.6).
        let g = dc_gain(Biquad::low_shelf(200.0, 12.0, 44100.0));
        assert!((g - 3.98).abs() < 0.05, "expected DC≈3.98, got {g}");
    }

    #[test]
    fn negative_low_shelf_cuts_dc() {
        // -12 dB low shelf: DC gain should be ~0.251 (10^-0.6).
        let g = dc_gain(Biquad::low_shelf(200.0, -12.0, 44100.0));
        assert!((g - 0.251).abs() < 0.01, "expected DC≈0.25, got {g}");
    }

    #[test]
    fn low_shelf_leaves_nyquist_alone_at_zero_gain() {
        let p = nyquist_peak(Biquad::low_shelf(200.0, 0.0, 44100.0));
        assert!((p - 1.0).abs() < 1e-3, "expected Nyquist≈1, got {p}");
    }
}

#[cfg(test)]
mod biquad_high_shelf_tests {
    use crate::audio::dsp::fx::Biquad;

    fn dc_gain(mut b: Biquad) -> f32 {
        let mut last = 0.0;
        for _ in 0..2000 {
            last = b.process(1.0);
        }
        last
    }

    fn nyquist_peak(mut b: Biquad) -> f32 {
        for i in 0..2000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let _ = b.process(x);
        }
        let mut peak = 0.0_f32;
        for i in 0..200 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            peak = peak.max(b.process(x).abs());
        }
        peak
    }

    #[test]
    fn zero_db_high_shelf_is_near_unity_passthrough_at_dc() {
        let g = dc_gain(Biquad::high_shelf(5000.0, 0.0, 44100.0));
        assert!((g - 1.0).abs() < 1e-3, "expected DC≈1, got {g}");
    }

    #[test]
    fn positive_high_shelf_boosts_nyquist() {
        // +12 dB at 5 kHz; the Nyquist response on a 44.1 k SR should
        // be substantially boosted (>2× since 22 kHz is well past 5 kHz).
        let p = nyquist_peak(Biquad::high_shelf(5000.0, 12.0, 44100.0));
        assert!(p > 2.0, "expected boost >2×, got {p}");
    }

    #[test]
    fn negative_high_shelf_cuts_nyquist() {
        let p = nyquist_peak(Biquad::high_shelf(5000.0, -12.0, 44100.0));
        assert!(p < 0.5, "expected cut <0.5, got {p}");
    }

    #[test]
    fn high_shelf_leaves_dc_alone_at_zero_gain() {
        let g = dc_gain(Biquad::high_shelf(5000.0, 0.0, 44100.0));
        assert!((g - 1.0).abs() < 1e-3);
    }
}

#[cfg(test)]
mod biquad_peak_tests {
    use crate::audio::dsp::fx::Biquad;

    fn dc_gain(mut b: Biquad) -> f32 {
        let mut last = 0.0;
        for _ in 0..2000 {
            last = b.process(1.0);
        }
        last
    }

    fn nyquist_peak(mut b: Biquad) -> f32 {
        for i in 0..2000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let _ = b.process(x);
        }
        let mut peak = 0.0_f32;
        for i in 0..200 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            peak = peak.max(b.process(x).abs());
        }
        peak
    }

    #[test]
    fn zero_db_peak_is_near_unity_at_both_ends() {
        let b = || Biquad::peak(1000.0, 1.0, 0.0, 44100.0);
        assert!((dc_gain(b()) - 1.0).abs() < 1e-3);
        assert!((nyquist_peak(b()) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn peak_centred_at_1khz_does_not_disturb_dc() {
        // +12 dB peak at 1 kHz; DC should be roughly unchanged.
        let g = dc_gain(Biquad::peak(1000.0, 1.0, 12.0, 44100.0));
        assert!((g - 1.0).abs() < 0.1, "DC drifted: {g}");
    }
}

#[cfg(test)]
mod biquad_state_tests {
    use crate::audio::dsp::fx::Biquad;

    #[test]
    fn fresh_biquad_starts_with_zero_state() {
        let b = Biquad::low_shelf(200.0, 0.0, 44100.0);
        assert_eq!(b.s1, 0.0);
        assert_eq!(b.s2, 0.0);
    }

    #[test]
    fn impulse_response_is_finite_and_decays() {
        let mut b = Biquad::low_shelf(200.0, 0.0, 44100.0);
        let mut out = Vec::with_capacity(1000);
        out.push(b.process(1.0)); // impulse
        for _ in 0..999 {
            out.push(b.process(0.0));
        }
        // Every sample finite.
        for &y in &out {
            assert!(y.is_finite());
        }
        // Tail magnitude smaller than initial.
        let initial = out[0].abs().max(out[1].abs());
        let tail = out[800..].iter().map(|y| y.abs()).fold(0.0_f32, f32::max);
        assert!(
            tail < initial,
            "tail {tail} should be smaller than initial {initial}"
        );
    }
}

#[cfg(test)]
mod eq_bands_tests {
    use crate::audio::dsp::fx::EqBands;

    #[test]
    fn zero_gain_settings_are_unity_passthrough_at_dc() {
        let mut eq = EqBands::new(44100.0);
        let mut last = 0.0;
        for _ in 0..3000 {
            last = eq.process(1.0, 0.0, 0.0, 0.0);
        }
        assert!((last - 1.0).abs() < 1e-2, "expected ≈1, got {last}");
    }

    #[test]
    fn positive_low_gain_boosts_dc_response() {
        let mut eq = EqBands::new(44100.0);
        let mut last = 0.0;
        // +1 normalised → +12 dB → ~3.98× at DC.
        for _ in 0..3000 {
            last = eq.process(1.0, 1.0, 0.0, 0.0);
        }
        assert!(last > 2.0, "expected boost, got {last}");
    }

    #[test]
    fn negative_low_gain_cuts_dc_response() {
        let mut eq = EqBands::new(44100.0);
        let mut last = 0.0;
        for _ in 0..3000 {
            last = eq.process(1.0, -1.0, 0.0, 0.0);
        }
        assert!(last < 0.5, "expected cut, got {last}");
    }

    #[test]
    fn coefficient_recompute_only_when_gain_changes() {
        // Sanity: changing only the high-band gain shouldn't blow up
        // the low/mid bands.  Run a long signal and verify finiteness.
        let mut eq = EqBands::new(44100.0);
        for i in 0..2000 {
            let g = (i as f32 / 2000.0) * 0.5; // ramp 0..0.5 on the high band
            let y = eq.process(0.3, 0.0, 0.0, g);
            assert!(y.is_finite());
        }
    }
}

#[cfg(test)]
mod compress_band_tests {
    use crate::audio::dsp::fx::Compressor;

    #[test]
    fn sub_threshold_input_passes_unchanged_after_settle() {
        // threshold=1.0 → the dB threshold is 0 dB; any input at unit
        // amplitude or below is below threshold and should pass through
        // approximately unchanged once the envelope settles.
        let mut env = 0.0;
        let mut out = 0.0;
        for _ in 0..5000 {
            out = Compressor::compress_band(0.3, &mut env, 1.0, 0.5, 44100.0);
        }
        assert!((out - 0.3).abs() < 0.05, "expected ≈0.3, got {out}");
    }

    #[test]
    fn supra_threshold_input_is_attenuated_when_ratio_is_active() {
        // threshold=0.0 → -40 dB threshold; unity input is well above it
        // and a high ratio should attenuate.
        let mut env = 0.0;
        let mut out = 0.0;
        for _ in 0..5000 {
            out = Compressor::compress_band(1.0, &mut env, 0.0, 1.0, 44100.0);
        }
        assert!(out < 1.0, "expected attenuation, got {out}");
        assert!(out > 0.0);
    }

    #[test]
    fn envelope_follows_input_magnitude() {
        // Envelope should be > 0 after ramping up against a constant
        // input; this covers the "rising" branch of the env update.
        let mut env = 0.0;
        for _ in 0..5000 {
            let _ = Compressor::compress_band(0.5, &mut env, 0.5, 0.5, 44100.0);
        }
        assert!(env > 0.4, "expected env to track input, got {env}");
    }

    #[test]
    fn envelope_decays_when_input_drops_to_zero() {
        let mut env = 0.0;
        // Charge the envelope with a hot input.
        for _ in 0..5000 {
            let _ = Compressor::compress_band(0.7, &mut env, 0.5, 0.5, 44100.0);
        }
        let charged = env;
        // Now silent input — envelope should fall.
        for _ in 0..10_000 {
            let _ = Compressor::compress_band(0.0, &mut env, 0.5, 0.5, 44100.0);
        }
        assert!(
            env < charged * 0.5,
            "expected decay, got {env} from {charged}"
        );
    }

    #[test]
    fn ratio_zero_means_one_to_one_no_compression() {
        // ratio_norm=0 → 1:1 ratio → gain reduction of 0 → output ≈ input.
        let mut env = 0.0;
        let mut out = 0.0;
        for _ in 0..5000 {
            out = Compressor::compress_band(0.6, &mut env, 0.0, 0.0, 44100.0);
        }
        assert!((out - 0.6).abs() < 0.05, "expected ≈0.6, got {out}");
    }

    #[test]
    fn output_stays_finite_for_silent_input() {
        let mut env = 0.0;
        for _ in 0..1000 {
            let out = Compressor::compress_band(0.0, &mut env, 0.5, 0.5, 44100.0);
            assert!(out.is_finite());
            assert!(env.is_finite());
        }
    }
}
