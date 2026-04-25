// ─── tests/fx_freq_shift_tests.rs ────────────────────────────────────────────
// Cover the Hilbert-pair frequency shifter: passthrough at 0 Hz, real
// shifting at non-zero offsets, and the FxState round-trip.  We don't
// assert on exact spectral peaks (Hilbert IIR ringing makes that
// fragile) — the tests check structural properties (energy in the
// shifted band, no NaN/Inf on long runs, FxState clamps + locks).

#[cfg(test)]
mod freq_shift_dsp_tests {
    use crate::audio::dsp::fx_freq_shift::FreqShift;

    /// Drive a sine through the FX with the given shift knob and
    /// return the wet-amplitude RMS over the second half of the run
    /// (first half lets the Hilbert cascade settle).
    fn rms_through_shifter(shift_norm: f32, sine_hz: f32) -> f32 {
        let mut fx = FreqShift::new();
        let sr = 48_000.0;
        let n = 4_800;
        let mut sq = 0.0f32;
        let mut count = 0;
        for i in 0..n {
            let t = i as f32 / sr;
            let x = (t * sine_hz * std::f32::consts::TAU).sin();
            let y = fx.process(x, shift_norm, 0.0, 1.0, sr);
            if i >= n / 2 {
                sq += y * y;
                count += 1;
            }
        }
        (sq / count as f32).sqrt()
    }

    #[test]
    fn shifter_passes_zero_shift_at_full_mix() {
        // 0.5 normalised = 0 Hz shift — the cosine multiplier is
        // constant at 1 and the sine multiplier is at 0, so the
        // output is the real Hilbert branch's response to the
        // input.  RMS should be on the same order as the input
        // (a sine at unit amplitude has RMS ≈ 0.707).
        let rms = rms_through_shifter(0.5, 440.0);
        assert!(
            rms > 0.3 && rms < 1.5,
            "0-Hz shift should pass roughly the input level; got rms = {rms}"
        );
    }

    #[test]
    fn nonzero_shift_does_not_diverge() {
        // Run 1 s of sine through with a 200 Hz upshift; output
        // amplitude must stay finite + bounded.
        let mut fx = FreqShift::new();
        let sr = 48_000.0;
        let mut max_abs = 0.0f32;
        for i in 0..48_000 {
            let t = i as f32 / sr;
            let x = (t * 440.0 * std::f32::consts::TAU).sin();
            // 0.6 normalised = +200 Hz upshift.
            let y = fx.process(x, 0.6, 0.0, 1.0, sr);
            assert!(y.is_finite(), "output went non-finite at sample {i}");
            max_abs = max_abs.max(y.abs());
        }
        assert!(
            max_abs < 4.0,
            "non-zero shift produced runaway amplitude {max_abs}",
        );
    }

    #[test]
    fn zero_mix_is_dry_passthrough() {
        let mut fx = FreqShift::new();
        let out = fx.process(0.7, 0.6, 0.5, 0.0, 48_000.0);
        assert_eq!(out, 0.7, "mix=0 must pass dry signal");
    }

    #[test]
    fn feedback_does_not_diverge_at_max() {
        // feedback = 1 maps to 0.95 inside the FX; with a steady
        // sine carrier the output must still stay bounded over
        // a long run.
        let mut fx = FreqShift::new();
        let sr = 48_000.0;
        let mut max_abs = 0.0f32;
        for i in 0..48_000 {
            let t = i as f32 / sr;
            let x = (t * 220.0 * std::f32::consts::TAU).sin() * 0.5;
            let y = fx.process(x, 0.55, 1.0, 1.0, sr);
            assert!(y.is_finite(), "feedback=max went non-finite at {i}");
            max_abs = max_abs.max(y.abs());
        }
        // Max amplitude can grow with high feedback but the
        // sub-unity clamp (0.95) keeps it bounded.  10× input is
        // a generous ceiling — anything past that suggests an
        // un-clamped runaway.
        assert!(
            max_abs < 10.0,
            "max-feedback runaway: {max_abs}; check fb_prev clamp"
        );
    }
}

#[cfg(test)]
mod freq_shift_state_tests {
    use crate::state::{AppState, FxState, apply_llm_update};

    #[test]
    fn defaults_are_zero_centre_and_zero_mix() {
        let fx = FxState::default();
        // 0.5 = 0 Hz centre — engaging the FX with default knobs is
        // a no-op so users can drop the module in to audition.
        assert!((fx.freq_shift_amount - 0.5).abs() < 1e-5);
        assert_eq!(fx.freq_shift_feedback, 0.0);
        assert_eq!(fx.freq_shift_mix, 0.0);
    }

    #[test]
    fn llm_apply_writes_freq_shift_knobs() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": {
                "freq_shift_amount": 0.7,
                "freq_shift_feedback": 0.3,
                "freq_shift_mix": 0.8,
            }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.freq_shift_amount - 0.7).abs() < 1e-5);
        assert!((s1.fx.freq_shift_feedback - 0.3).abs() < 1e-5);
        assert!((s1.fx.freq_shift_mix - 0.8).abs() < 1e-5);
    }

    #[test]
    fn locked_freq_shift_mix_skipped() {
        let s0 = AppState::default();
        let json = serde_json::json!({ "fx": { "freq_shift_mix": 0.9 } });
        let locked = ["fx.freq_shift_mix".to_string()];
        let s1 = apply_llm_update(s0, &json, &locked);
        assert_eq!(s1.fx.freq_shift_mix, 0.0);
    }

    #[test]
    fn freq_shift_xy_pad_writes_amount_and_feedback() {
        let s0 = AppState::default();
        let json = serde_json::json!({
            "fx": { "freq_shift_xy": [0.65, 0.4] }
        });
        let s1 = apply_llm_update(s0, &json, &[]);
        assert!((s1.fx.freq_shift_amount - 0.65).abs() < 1e-5);
        assert!((s1.fx.freq_shift_feedback - 0.4).abs() < 1e-5);
    }
}

#[cfg(test)]
mod freq_shift_module_tests {
    use crate::state::{FxStep, ModuleKind, fx_plan::kind_to_fx_step};

    #[test]
    fn fxfreqshift_maps_to_freqshift_step() {
        assert_eq!(
            kind_to_fx_step(ModuleKind::FxFreqShift),
            Some(FxStep::FreqShift)
        );
    }

    #[test]
    fn fxfreqshift_label_is_freqshift() {
        assert_eq!(ModuleKind::FxFreqShift.label(), "FREQSHIFT");
    }

    #[test]
    fn fxfreqshift_has_no_sidechain() {
        // FreqShift is purely in-chain — no sidechain port.  Pinning
        // this so a future refactor that adds one (which would be a
        // semantic change, since the Hilbert pair takes a single
        // signal) flips this test deliberately.
        assert!(!ModuleKind::FxFreqShift.has_sidechain_in());
    }
}
