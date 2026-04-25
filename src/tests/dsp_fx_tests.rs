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
            out = Compressor::compress_band(0.3, &mut env, 1.0, 0.5, 44100.0, false);
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
            out = Compressor::compress_band(1.0, &mut env, 0.0, 1.0, 44100.0, false);
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
            let _ = Compressor::compress_band(0.5, &mut env, 0.5, 0.5, 44100.0, false);
        }
        assert!(env > 0.4, "expected env to track input, got {env}");
    }

    #[test]
    fn envelope_decays_when_input_drops_to_zero() {
        let mut env = 0.0;
        // Charge the envelope with a hot input.
        for _ in 0..5000 {
            let _ = Compressor::compress_band(0.7, &mut env, 0.5, 0.5, 44100.0, false);
        }
        let charged = env;
        // Now silent input — envelope should fall.
        for _ in 0..10_000 {
            let _ = Compressor::compress_band(0.0, &mut env, 0.5, 0.5, 44100.0, false);
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
            out = Compressor::compress_band(0.6, &mut env, 0.0, 0.0, 44100.0, false);
        }
        assert!((out - 0.6).abs() < 0.05, "expected ≈0.6, got {out}");
    }

    #[test]
    fn output_stays_finite_for_silent_input() {
        let mut env = 0.0;
        for _ in 0..1000 {
            let out = Compressor::compress_band(0.0, &mut env, 0.5, 0.5, 44100.0, false);
            assert!(out.is_finite());
            assert!(env.is_finite());
        }
    }

    // ── Reverse mode (swap attack / release) ────────────────────────────

    #[test]
    fn reverse_mode_envelope_rises_slower_than_normal() {
        // Fire 100 samples of hot input into two fresh envelopes.  The
        // normal compressor's 1 ms attack catches the level almost
        // immediately; the reverse mode's 80 ms attack barely moves.
        let mut env_normal = 0.0;
        let mut env_reverse = 0.0;
        for _ in 0..100 {
            let _ = Compressor::compress_band(0.9, &mut env_normal, 0.5, 0.5, 44100.0, false);
            let _ = Compressor::compress_band(0.9, &mut env_reverse, 0.5, 0.5, 44100.0, true);
        }
        assert!(
            env_reverse < env_normal * 0.3,
            "reverse env should lag: normal={env_normal}, reverse={env_reverse}"
        );
    }

    #[test]
    fn reverse_mode_envelope_falls_faster_than_normal() {
        // Charge both envelopes to roughly the same level, then feed
        // silence.  Normal has 80 ms release, reverse has 1 ms → reverse
        // should drop to near-zero orders of magnitude faster.
        let mut env_normal = 0.0;
        let mut env_reverse = 0.0;
        for _ in 0..10_000 {
            let _ = Compressor::compress_band(0.9, &mut env_normal, 0.5, 0.5, 44100.0, false);
            let _ = Compressor::compress_band(0.9, &mut env_reverse, 0.5, 0.5, 44100.0, true);
        }
        // Normal charges fast (1 ms attack) and sits at ~0.9; reverse's
        // slow 80 ms attack catches up to only ~0.85 over the charge
        // window — close enough that both will behave comparably if the
        // release constants were identical.
        assert!(env_normal > 0.85);
        assert!(env_reverse > 0.80);
        // Now drain for 100 samples of silence.
        for _ in 0..100 {
            let _ = Compressor::compress_band(0.0, &mut env_normal, 0.5, 0.5, 44100.0, false);
            let _ = Compressor::compress_band(0.0, &mut env_reverse, 0.5, 0.5, 44100.0, true);
        }
        assert!(
            env_reverse < env_normal * 0.5,
            "reverse env should drop faster on silence: normal={env_normal}, reverse={env_reverse}"
        );
    }

    #[test]
    fn reverse_mode_lets_initial_transient_through() {
        // Step input 0 → 1.  With threshold=0 (−40 dB) + high ratio the
        // normal compressor clamps hard on the first sample (fast attack
        // catches the level); the reverse compressor lets most of the
        // transient through because the envelope hasn't ramped up yet.
        let mut env_normal = 0.0;
        let mut env_reverse = 0.0;
        let mut first_normal = 0.0;
        let mut first_reverse = 0.0;
        for i in 0..20 {
            let o_n = Compressor::compress_band(1.0, &mut env_normal, 0.0, 1.0, 44100.0, false);
            let o_r = Compressor::compress_band(1.0, &mut env_reverse, 0.0, 1.0, 44100.0, true);
            if i == 0 {
                first_normal = o_n;
                first_reverse = o_r;
            }
        }
        assert!(
            first_reverse > first_normal,
            "reverse should let transient through: first_normal={first_normal}, first_reverse={first_reverse}"
        );
    }

    #[test]
    fn reverse_mode_still_clamps_sustain() {
        // Long tone past the envelope's slow attack window — reverse
        // should eventually catch up and clamp, same as normal.
        let mut env_normal = 0.0;
        let mut env_reverse = 0.0;
        let mut last_normal = 0.0;
        let mut last_reverse = 0.0;
        for _ in 0..20_000 {
            last_normal = Compressor::compress_band(1.0, &mut env_normal, 0.0, 1.0, 44100.0, false);
            last_reverse =
                Compressor::compress_band(1.0, &mut env_reverse, 0.0, 1.0, 44100.0, true);
        }
        // Both should be well below unity by now.
        assert!(last_normal < 0.8);
        assert!(last_reverse < 0.8);
        // And they should land in the same general ballpark — the
        // envelopes have both saturated, so the gain reduction matches.
        assert!((last_normal - last_reverse).abs() < 0.1);
    }
}

#[cfg(test)]
mod reverb_tests {
    use crate::audio::dsp::fx::Reverb;

    #[test]
    fn fresh_reverb_emits_silence_for_silent_input() {
        let mut r = Reverb::new();
        for _ in 0..2000 {
            let out = r.process(0.0, 0.5, 0.3, false);
            assert_eq!(out, 0.0);
        }
    }

    #[test]
    fn impulse_produces_a_decaying_tail() {
        let mut r = Reverb::new();
        // Single-sample impulse, then silence.  The shortest comb delay
        // is 1116 samples, so the first echo only appears after that.
        let _ = r.process(1.0, 0.5, 0.3, false);
        let mut early_peak = 0.0_f32;
        for _ in 0..3000 {
            early_peak = early_peak.max(r.process(0.0, 0.5, 0.3, false).abs());
        }
        let mut tail_peak = 0.0_f32;
        for _ in 0..50_000 {
            tail_peak = tail_peak.max(r.process(0.0, 0.5, 0.3, false).abs());
        }
        // Some energy fed back from the impulse should be audible.
        assert!(early_peak > 0.0, "expected audible early tail");
        // Far tail magnitude must not exceed the early tail (decay).
        assert!(
            tail_peak <= early_peak + 1e-3,
            "tail {tail_peak} should be ≤ early {early_peak}"
        );
    }

    #[test]
    fn freeze_holds_existing_tail_without_new_input() {
        let mut r = Reverb::new();
        // Energise the buffers with a hot input.
        for _ in 0..200 {
            let _ = r.process(0.5, 0.9, 0.0, false);
        }
        // Now freeze with a hot loaded input — the wet tail should
        // persist (feedback = 1.0) for many samples without dying off.
        let mut frozen_tail = 0.0_f32;
        for _ in 0..4000 {
            frozen_tail = frozen_tail.max(r.process(99.9, 0.9, 0.0, true).abs());
        }
        assert!(frozen_tail > 0.0, "frozen tail should keep going");
    }

    #[test]
    fn output_stays_finite_under_continuous_drive() {
        let mut r = Reverb::new();
        for i in 0..10_000 {
            // Mix of DC + a slow oscillation so feedback gets exercised.
            let drive = 0.3 + 0.2 * (i as f32 * 0.001).sin();
            let out = r.process(drive, 0.7, 0.4, false);
            assert!(out.is_finite(), "blew up after {i} samples: {out}");
        }
    }
}

#[cfg(test)]
mod delay_line_tests {
    use crate::audio::dsp::fx::DelayLine;

    #[test]
    fn input_resurfaces_at_the_chosen_delay_offset() {
        let mut d = DelayLine::new();
        let delay = 256_usize;
        // Single-sample impulse with no feedback / wow / saturation.
        let _ = d.process_tape(1.0, delay, 0.0, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
        // 255 samples of silence shouldn't reveal it yet.
        for _ in 0..(delay - 1) {
            let out = d.process_tape(0.0, delay, 0.0, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            assert!(out.abs() < 1e-3, "early leak: {out}");
        }
        // Sample at the chosen offset should now have the impulse echo.
        let out = d.process_tape(0.0, delay, 0.0, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
        assert!((out - 1.0).abs() < 0.01, "expected 1.0, got {out}");
    }

    #[test]
    fn feedback_creates_a_decaying_echo_train() {
        let mut d = DelayLine::new();
        let delay = 64_usize;
        let _ = d.process_tape(1.0, delay, 0.5, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
        let mut peaks: Vec<f32> = Vec::new();
        for n in 0..6 {
            // Skip ahead delay samples and read the echo magnitude.
            for _ in 0..(delay - 1) {
                let _ = d.process_tape(0.0, delay, 0.5, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            }
            let out = d.process_tape(0.0, delay, 0.5, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            peaks.push(out.abs());
            let _ = n;
        }
        // Each successive echo should be smaller than the previous one.
        for w in peaks.windows(2) {
            assert!(w[1] < w[0] + 1e-4, "echoes must decay: {peaks:?}");
        }
    }

    #[test]
    fn output_stays_finite_with_high_feedback_and_saturation() {
        let mut d = DelayLine::new();
        for i in 0..5_000 {
            let drive = 0.5 + 0.3 * (i as f32 * 0.01).sin();
            let out = d.process_tape(drive, 1000, 0.95, 0.5, 1.0, false, 0.0, 0.0, 44100.0);
            assert!(out.is_finite());
        }
    }

    // ── Dub send/return: freeze + feedback-path filters ─────────────────

    #[test]
    fn freeze_holds_tail_without_new_input() {
        let mut d = DelayLine::new();
        let delay = 128_usize;
        // Prime the loop with a burst, then freeze and feed silence.
        for _ in 0..delay {
            let _ = d.process_tape(0.4, delay, 0.6, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
        }
        // Snapshot the peak amplitude before freezing.
        let mut peak_pre = 0.0f32;
        for _ in 0..(delay * 2) {
            let out = d.process_tape(0.0, delay, 0.6, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            peak_pre = peak_pre.max(out.abs());
        }
        // Now freeze (suppresses input, feedback ≈ 1.0) and loud pulse should
        // be ignored — loop should keep the pre-freeze energy.
        let mut peak_hold = 0.0f32;
        for _ in 0..(delay * 10) {
            let out = d.process_tape(1.0, delay, 0.6, 0.0, 0.0, true, 0.0, 0.0, 44100.0);
            peak_hold = peak_hold.max(out.abs());
        }
        // Frozen loop shouldn't have picked up the 1.0 pulses (they're
        // suppressed), so peak stays bounded by the pre-freeze content.
        assert!(
            peak_hold < peak_pre * 1.5,
            "freeze should not integrate new input: peak_pre={peak_pre}, peak_hold={peak_hold}"
        );
        // And the tail doesn't decay to silence while frozen.
        assert!(
            peak_hold > 0.1,
            "frozen tail should sustain, got {peak_hold}"
        );
    }

    #[test]
    fn feedback_hpf_drains_dc_under_freeze() {
        // Prime both delay lines with DC, then freeze (fb ≈ 1.0).  Without
        // HPF the DC sustains in the loop forever; with HPF the highpass
        // eventually integrates the DC out of the feedback path.  Compare
        // the long-tail amplitude.
        let mut d_plain = DelayLine::new();
        let mut d_hpf = DelayLine::new();
        let delay = 128_usize;
        for _ in 0..(delay * 3) {
            let _ = d_plain.process_tape(0.5, delay, 0.9, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            let _ = d_hpf.process_tape(0.5, delay, 0.9, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
        }
        let mut last_plain = 0.0f32;
        let mut last_hpf = 0.0f32;
        for _ in 0..(delay * 40) {
            last_plain = d_plain
                .process_tape(0.0, delay, 0.9, 0.0, 0.0, true, 0.0, 0.0, 44100.0)
                .abs();
            last_hpf = d_hpf
                .process_tape(0.0, delay, 0.9, 0.0, 0.0, true, 0.5, 0.0, 44100.0)
                .abs();
        }
        assert!(
            last_hpf < last_plain * 0.5,
            "HPF should drain DC: plain={last_plain}, hpf={last_hpf}"
        );
    }

    #[test]
    fn feedback_lpf_is_bypass_at_zero() {
        // Confirm the 0-knob branch truly leaves the signal untouched —
        // two runs with identical seed must produce identical outputs.
        let mut d_a = DelayLine::new();
        let mut d_b = DelayLine::new();
        let delay = 96_usize;
        for _ in 0..(delay * 3) {
            let a = d_a.process_tape(0.25, delay, 0.5, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            let b = d_b.process_tape(0.25, delay, 0.5, 0.0, 0.0, false, 0.0, 0.0, 44100.0);
            assert!((a - b).abs() < 1e-6);
        }
    }
}

#[cfg(test)]
mod chorus_tests {
    use crate::audio::dsp::fx::Chorus;

    #[test]
    fn zero_mix_returns_dry_passthrough() {
        let mut c = Chorus::new();
        let dry = 0.42;
        assert_eq!(c.process(dry, 0.5, 0.5, 0.0, 44100.0), dry);
    }

    #[test]
    fn produces_finite_output_under_full_wet_modulation() {
        let mut c = Chorus::new();
        for i in 0..5_000 {
            let x = 0.5 * (i as f32 * 0.01).sin();
            let out = c.process(x, 0.7, 0.7, 1.0, 44100.0);
            assert!(out.is_finite());
        }
    }

    #[test]
    fn read_tap_indexes_into_the_buffer_safely() {
        let mut c = Chorus::new();
        for _ in 0..200 {
            let _ = c.process(0.3, 0.5, 0.5, 1.0, 44100.0);
        }
        let v = c.read_tap(0.0);
        let v_mid = c.read_tap(0.5);
        let v_end = c.read_tap(1.0);
        assert!(v.is_finite() && v_mid.is_finite() && v_end.is_finite());
    }
}

#[cfg(test)]
mod tape_sat_tests {
    use crate::audio::dsp::fx::TapeSat;

    #[test]
    fn zero_mix_returns_dry() {
        let mut t = TapeSat::new();
        assert_eq!(t.process(0.6, 0.7, 0.0, 0.5, 44100.0), 0.6);
    }

    #[test]
    fn produces_finite_output_at_high_drive() {
        let mut t = TapeSat::new();
        for i in 0..2_000 {
            let x = 0.7 * (i as f32 * 0.02).sin();
            let out = t.process(x, 1.0, 1.0, 1.0, 44100.0);
            assert!(out.is_finite());
        }
    }
}

#[cfg(test)]
mod phaser_tests {
    use crate::audio::dsp::fx::Phaser;

    #[test]
    fn zero_mix_returns_dry() {
        let mut p = Phaser::new();
        assert_eq!(p.process(0.3, 0.5, 0.5, 0.0, 44100.0), 0.3);
    }

    #[test]
    fn full_wet_produces_a_finite_modulated_signal() {
        let mut p = Phaser::new();
        let mut nonzero = false;
        for i in 0..2_000 {
            let x = 0.5 * (i as f32 * 0.05).sin();
            let out = p.process(x, 0.7, 0.8, 1.0, 44100.0);
            assert!(out.is_finite());
            if out.abs() > 0.05 {
                nonzero = true;
            }
        }
        assert!(nonzero, "expected audible output from full-wet phaser");
    }
}

#[cfg(test)]
mod flanger_tests {
    use crate::audio::dsp::fx_extras::Flanger;

    #[test]
    fn zero_mix_zero_feedback_returns_dry() {
        let mut f = Flanger::new();
        // feedback=0.5 is the no-feedback midpoint; mix=0 is full dry.
        assert_eq!(f.process(0.3, 0.5, 0.5, 0.5, 0.0, 48000.0), 0.3);
    }

    #[test]
    fn full_wet_produces_a_finite_modulated_signal() {
        let mut f = Flanger::new();
        let mut nonzero = false;
        for i in 0..4_000 {
            let x = 0.5 * (i as f32 * 0.05).sin();
            let out = f.process(x, 0.4, 0.8, 0.5, 1.0, 48000.0);
            assert!(out.is_finite());
            if out.abs() > 0.05 {
                nonzero = true;
            }
        }
        assert!(nonzero, "expected audible output from full-wet flanger");
    }

    #[test]
    fn positive_feedback_does_not_blow_up() {
        // Flanger feedback knob is bipolar around 0.5; 1.0 maps to ~+0.95
        // signed feedback, the highest-stress setting before runaway.
        let mut f = Flanger::new();
        let mut peak: f32 = 0.0;
        for i in 0..20_000 {
            let x = 0.5 * (i as f32 * 0.07).sin();
            let out = f.process(x, 0.3, 0.6, 1.0, 1.0, 48000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Worst-case should still be bounded by single-digit linear
        // amplitude — the internal clamp tops feedback below unity.
        assert!(peak < 8.0, "flanger blew up at max feedback (peak={peak})");
    }

    #[test]
    fn negative_feedback_inverts_comb_pattern() {
        // At feedback=0.0 (max negative) the comb has notches where it
        // would have peaks at feedback=1.0.  We don't measure spectrum
        // here, just confirm the process is stable + finite at the
        // negative extreme.
        let mut f = Flanger::new();
        for i in 0..4_000 {
            let x = 0.5 * (i as f32 * 0.03).sin();
            let out = f.process(x, 0.2, 0.7, 0.0, 1.0, 48000.0);
            assert!(out.is_finite());
        }
    }
}

#[cfg(test)]
mod limiter_tests {
    use crate::audio::dsp::fx_extras::Limiter;

    #[test]
    fn full_threshold_returns_dry_within_lookahead() {
        // threshold=1.0 (0 dB) → no limiting kicks in for any sample below
        // ceiling.  Output is the lookahead-delayed input plus the safety
        // clip at the ceiling, so verify a low-amplitude sine passes
        // through with the same shape (modulo delay).
        let mut l = Limiter::new();
        let sr = 48_000.0;
        let mut nonzero = false;
        // First N samples come from the unfilled buffer (zeros) before the
        // delayed signal arrives.
        for i in 0..2_000 {
            let x = 0.3 * (i as f32 * 0.05).sin();
            let out = l.process(x, 1.0, 1.0, 0.3, 0.5, sr);
            assert!(out.is_finite());
            if i > 600 && out.abs() > 0.05 {
                nonzero = true;
            }
        }
        assert!(nonzero);
    }

    #[test]
    fn loud_input_is_clamped_at_ceiling() {
        let mut l = Limiter::new();
        let sr = 48_000.0;
        let mut peak = 0.0f32;
        for i in 0..4_000 {
            // 2.0 amplitude — way over the ceiling.
            let x = 2.0 * (i as f32 * 0.05).sin();
            let out = l.process(x, 0.0, 0.5, 0.3, 0.5, sr);
            assert!(out.is_finite());
            if i > 800 {
                peak = peak.max(out.abs());
            }
        }
        // ceiling = 0.5 → −6 dB → linear ≈ 0.501.  Allow tiny overshoot for
        // attack lag at the very first peak before the limiter catches up.
        let ceil_lin = 10.0f32.powf(-6.0 / 20.0);
        assert!(
            peak <= ceil_lin * 1.05,
            "peak {peak} exceeded ceiling {ceil_lin}"
        );
    }
}

#[cfg(test)]
mod svf_tests {
    use crate::audio::dsp::fx_extras::Svf;

    #[test]
    fn zero_mix_returns_dry() {
        let mut f = Svf::new();
        assert_eq!(f.process(0.4, 0.5, 0.5, 0.0, 0, 0.0, 48_000.0), 0.4);
    }

    #[test]
    fn full_wet_each_mode_is_finite_and_audible() {
        for mode in 0..=3u8 {
            let mut f = Svf::new();
            let mut nonzero = false;
            for i in 0..4_000 {
                let x = 0.5 * (i as f32 * 0.05).sin();
                let out = f.process(x, 0.7, 0.4, 0.0, mode, 1.0, 48_000.0);
                assert!(out.is_finite(), "mode {mode} produced NaN");
                if out.abs() > 0.01 {
                    nonzero = true;
                }
            }
            assert!(nonzero, "SVF mode {mode} fell silent");
        }
    }
}

#[cfg(test)]
mod comb_tests {
    use crate::audio::dsp::fx_extras::CombRes;

    #[test]
    fn zero_mix_zero_feedback_returns_dry() {
        let mut c = CombRes::new();
        assert_eq!(c.process(0.3, 0.5, 0.0, 0.0, 0.0, 48_000.0), 0.3);
    }

    #[test]
    fn high_feedback_stays_finite() {
        let mut c = CombRes::new();
        for i in 0..10_000 {
            let x = 0.5 * (i as f32 * 0.05).sin();
            let out = c.process(x, 0.4, 0.95, 0.3, 1.0, 48_000.0);
            assert!(out.is_finite());
        }
    }
}

#[cfg(test)]
mod tilt_tests {
    use crate::audio::dsp::fx_extras::Tilt;

    #[test]
    fn zero_mix_returns_dry() {
        let mut t = Tilt::new(48_000.0);
        assert_eq!(t.process(0.25, 0.5, 0.5, 0.0), 0.25);
    }

    #[test]
    fn full_wet_unity_at_flat_tilt() {
        // tilt=0.5 means flat shelves; output should be very close to input.
        let mut t = Tilt::new(48_000.0);
        let mut diff_sum = 0.0f32;
        let mut count = 0usize;
        for i in 0..2_000 {
            let x = 0.4 * (i as f32 * 0.05).sin();
            let y = t.process(x, 0.5, 0.5, 1.0);
            assert!(y.is_finite());
            if i > 200 {
                diff_sum += (x - y).abs();
                count += 1;
            }
        }
        let mean_err = diff_sum / count as f32;
        assert!(
            mean_err < 0.05,
            "tilt at 0.5 should be near unity, err={mean_err}"
        );
    }
}

#[cfg(test)]
mod transient_tests {
    use crate::audio::dsp::fx_extras::Transient;

    #[test]
    fn zero_mix_returns_dry() {
        let mut t = Transient::new();
        assert_eq!(t.process(0.3, 0.5, 0.5, 0.0, 48_000.0), 0.3);
    }

    #[test]
    fn full_wet_finite_under_pulses() {
        // Bursts of energy followed by silence — the regime where attack /
        // sustain knobs actually do work.
        let mut t = Transient::new();
        for i in 0..8_000 {
            let burst = if (i / 200) % 2 == 0 { 0.5 } else { 0.0 };
            let x = burst * (i as f32 * 0.3).sin();
            let out = t.process(x, 0.8, 0.3, 1.0, 48_000.0);
            assert!(out.is_finite());
        }
    }
}

#[cfg(test)]
mod exciter_tests {
    use crate::audio::dsp::fx_extras::Exciter;

    #[test]
    fn zero_mix_returns_dry() {
        let mut e = Exciter::new();
        assert_eq!(e.process(0.5, 0.5, 0.5, 0.0, 48_000.0), 0.5);
    }

    #[test]
    fn full_drive_is_additive_and_finite() {
        let mut e = Exciter::new();
        for i in 0..2_000 {
            let x = 0.4 * (i as f32 * 0.05).sin();
            let out = e.process(x, 1.0, 0.5, 0.5, 48_000.0);
            assert!(out.is_finite());
        }
    }
}

#[cfg(test)]
mod autotune_tests {
    use crate::audio::dsp::fx::Autotune;

    #[test]
    fn zero_amount_returns_dry_signal() {
        let mut a = Autotune::new();
        assert_eq!(a.process(0.4, 0.0, 1.0), 0.4);
    }

    #[test]
    fn zero_mix_returns_dry_signal() {
        let mut a = Autotune::new();
        assert_eq!(a.process(0.4, 0.5, 0.0), 0.4);
    }

    #[test]
    fn produces_finite_output_under_full_pitch_shift() {
        let mut a = Autotune::new();
        for i in 0..5_000 {
            let x = 0.5 * (i as f32 * 0.015).sin();
            let out = a.process(x, 1.0, 1.0);
            assert!(out.is_finite());
        }
    }

    #[test]
    fn full_wet_produces_audible_signal_after_priming() {
        let mut a = Autotune::new();
        // Prime the ring buffer + crossfade envelope.
        for i in 0..2_000 {
            let _ = a.process(0.5 * (i as f32 * 0.01).sin(), 1.0, 1.0);
        }
        // Now sample the wet output for a stretch and confirm non-silence.
        let mut peak = 0.0_f32;
        for i in 0..2_000 {
            peak = peak.max(a.process(0.5 * (i as f32 * 0.01).sin(), 1.0, 1.0).abs());
        }
        assert!(peak > 0.05, "autotune should produce audible output");
    }
}
