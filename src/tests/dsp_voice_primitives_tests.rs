// ─── tests/dsp_voice_primitives_tests.rs ─────────────────────────────────────
// Tests for the low-level DSP voice primitives in audio/dsp/voices.rs:
// LadderFilter, OnePole, NoiseGen, Envelope, ADSR helpers, osc_sample,
// and the DrumVoice → index mapping.  Split from dsp_tests.rs (was 930
// lines) so both files stay comfortably under the 1000-line cap — the
// FX-math + state-propagation tests stay in dsp_tests.rs.

#[cfg(test)]
mod ladder_filter_tests {
    use crate::audio::dsp::voices::LadderFilter;

    #[test]
    fn zero_input_zero_output_in_steady_state() {
        let mut f = LadderFilter::default();
        // Run a few samples of silence — the filter shouldn't ring without input.
        for _ in 0..10 {
            let out = f.process(0.0, 0.5, 0.0);
            assert!(out.abs() < 1e-6);
        }
    }

    #[test]
    fn output_attenuates_high_frequencies_relative_to_dc() {
        let mut f = LadderFilter::default();
        // Drive DC for 2000 samples — should approach the input.
        let mut dc_steady = 0.0;
        for _ in 0..2000 {
            dc_steady = f.process(0.5, 0.05, 0.0);
        }
        // Now feed a high-frequency square wave for 2000 samples.
        let mut f2 = LadderFilter::default();
        let mut hf_peak = 0.0_f32;
        for i in 0..2000 {
            let sq = if i % 2 == 0 { 0.5 } else { -0.5 };
            let out = f2.process(sq, 0.05, 0.0);
            hf_peak = hf_peak.max(out.abs());
        }
        assert!(
            hf_peak < dc_steady.abs(),
            "low cutoff should attenuate HF more than DC: hf_peak={hf_peak}, dc={dc_steady}"
        );
    }

    #[test]
    fn output_stays_finite_at_high_resonance() {
        let mut f = LadderFilter::default();
        // Self-oscillation territory; tanh saturation should keep things bounded.
        for _ in 0..1000 {
            let out = f.process(0.1, 0.5, 1.0);
            assert!(out.is_finite(), "filter blew up: {out}");
            assert!(out.abs() < 5.0, "filter ran away: {out}");
        }
    }
}

#[cfg(test)]
mod one_pole_tests {
    use crate::audio::dsp::voices::OnePole;

    #[test]
    fn coeff_one_freezes_state_to_initial_value() {
        let mut f = OnePole::default();
        // First call seeds state from input (since state starts at 0):
        // state = 0*1 + input*0 = 0; output = 0.
        let _ = f.process(1.0, 1.0);
        // Subsequent calls keep returning 0 because input is zero-weighted.
        assert_eq!(f.process(0.5, 1.0), 0.0);
    }

    #[test]
    fn coeff_zero_passes_input_through() {
        let mut f = OnePole::default();
        assert_eq!(f.process(0.7, 0.0), 0.7);
        assert_eq!(f.process(-0.3, 0.0), -0.3);
    }

    #[test]
    fn smoothing_converges_to_step_input() {
        let mut f = OnePole::default();
        let mut last = 0.0;
        for _ in 0..200 {
            last = f.process(1.0, 0.95);
        }
        assert!(
            (last - 1.0).abs() < 0.01,
            "should converge close to 1.0, got {last}"
        );
    }
}

#[cfg(test)]
mod noise_gen_tests {
    use crate::audio::dsp::voices::NoiseGen;

    #[test]
    fn output_stays_in_signed_unit_range() {
        let mut g = NoiseGen::new(42);
        for _ in 0..1000 {
            let v = g.next();
            assert!((-1.0..=1.0).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    fn fixed_seed_is_deterministic() {
        let mut a = NoiseGen::new(123);
        let mut b = NoiseGen::new(123);
        for _ in 0..50 {
            assert_eq!(a.next(), b.next());
        }
    }

    #[test]
    fn distinct_seeds_diverge_within_a_few_samples() {
        let mut a = NoiseGen::new(1);
        let mut b = NoiseGen::new(2);
        let mut diverged = false;
        for _ in 0..16 {
            if a.next() != b.next() {
                diverged = true;
                break;
            }
        }
        assert!(diverged, "different seeds should produce different streams");
    }

    #[test]
    fn zero_seed_is_replaced_to_avoid_xorshift_lock() {
        let mut g = NoiseGen::new(0);
        // The constructor's `seed.max(1)` keeps xorshift32 out of its
        // degenerate fixed-point at zero.
        let v = g.next();
        assert!(v.is_finite());
        assert!(v != 0.0 || g.next() != 0.0, "stream got stuck at zero");
    }
}

#[cfg(test)]
mod envelope_tests {
    use crate::audio::dsp::voices::Envelope;

    #[test]
    fn fresh_envelope_is_inactive_and_returns_zero() {
        let mut e = Envelope::default();
        assert!(!e.active);
        assert_eq!(e.tick(0.99), 0.0);
    }

    #[test]
    fn trigger_arms_envelope_at_unity() {
        let mut e = Envelope::default();
        e.trigger();
        assert!(e.active);
        assert_eq!(e.value, 1.0);
    }

    #[test]
    fn tick_decays_geometrically_while_active() {
        let mut e = Envelope::default();
        e.trigger();
        let v1 = e.tick(0.5);
        let v2 = e.tick(0.5);
        assert_eq!(v1, 0.5);
        assert_eq!(v2, 0.25);
    }

    #[test]
    fn tiny_value_deactivates_envelope() {
        let mut e = Envelope::default();
        e.trigger();
        // Very low decay coefficient drops the envelope under 1e-6 quickly.
        for _ in 0..40 {
            let _ = e.tick(0.5);
        }
        assert!(!e.active, "envelope should have deactivated");
        assert_eq!(e.tick(0.99), 0.0);
    }
}

#[cfg(test)]
mod adsr_tests {
    use crate::audio::dsp::voices::{AdsrPhase, adsr_samples, adsr_tick};

    #[test]
    fn adsr_samples_minimum_clamps_to_one() {
        // v=0 → 1 ms; at 1000 Hz that's exactly 1 sample, but the floor
        // protects against absurdly short attacks at very low SRs too.
        let n = adsr_samples(0.0, 100.0, 10_000.0);
        assert_eq!(n, 1.0);
    }

    #[test]
    fn adsr_samples_quadratic_curve_grows_with_v() {
        let sr = 44100.0;
        let n_quarter = adsr_samples(0.25, sr, 10_000.0);
        let n_half = adsr_samples(0.5, sr, 10_000.0);
        let n_full = adsr_samples(1.0, sr, 10_000.0);
        assert!(n_quarter < n_half);
        assert!(n_half < n_full);
        // Quadratic: doubling v roughly quadruples the resulting time.
        assert!(n_half / n_quarter > 3.0);
    }

    #[test]
    fn adsr_attack_phase_advances_value_toward_unity() {
        let mut phase = AdsrPhase::Attack;
        let mut val = 0.0;
        for _ in 0..2000 {
            adsr_tick(&mut phase, &mut val, true, 0.05, 0.5, 0.5, 0.5, 44100.0);
        }
        assert!(val > 0.5, "attack should ramp the value up, got {val}");
    }

    #[test]
    fn adsr_release_phase_decays_to_zero() {
        let mut phase = AdsrPhase::Release;
        let mut val = 0.5;
        // Quadratic curve: at v=0.05 release is ~76 ms; we need a bit
        // over that to fall below the 1e-4 idle threshold.
        for _ in 0..50_000 {
            adsr_tick(&mut phase, &mut val, false, 0.5, 0.5, 0.5, 0.05, 44100.0);
        }
        assert_eq!(val, 0.0);
        assert_eq!(phase, AdsrPhase::Idle);
    }

    #[test]
    fn adsr_release_re_enters_attack_when_gate_returns() {
        let mut phase = AdsrPhase::Release;
        let mut val = 0.3;
        // Gate goes high while in Release — the engine should retrigger.
        adsr_tick(&mut phase, &mut val, true, 0.5, 0.5, 0.5, 0.5, 44100.0);
        assert_eq!(phase, AdsrPhase::Attack);
    }

    #[test]
    fn adsr_sustain_drops_to_release_when_gate_clears() {
        let mut phase = AdsrPhase::Sustain;
        let mut val = 0.7;
        adsr_tick(&mut phase, &mut val, false, 0.5, 0.5, 0.7, 0.5, 44100.0);
        assert_eq!(phase, AdsrPhase::Release);
    }

    #[test]
    fn adsr_idle_phase_does_nothing() {
        let mut phase = AdsrPhase::Idle;
        let mut val = 0.0;
        for _ in 0..100 {
            adsr_tick(&mut phase, &mut val, false, 0.5, 0.5, 0.5, 0.5, 44100.0);
        }
        assert_eq!(phase, AdsrPhase::Idle);
        assert_eq!(val, 0.0);
    }
}

#[cfg(test)]
mod osc_sample_tests {
    use crate::audio::dsp::voices::osc_sample;

    #[test]
    fn saw_ramps_minus1_to_plus1() {
        let mut ns = 1u32;
        assert!((osc_sample(0, 0.0, &mut ns) + 1.0).abs() < 1e-5);
        assert!((osc_sample(0, 0.5, &mut ns) - 0.0).abs() < 1e-5);
        assert!((osc_sample(0, 1.0, &mut ns) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn square_flips_at_half_phase() {
        let mut ns = 1u32;
        assert_eq!(osc_sample(1, 0.0, &mut ns), 1.0);
        assert_eq!(osc_sample(1, 0.49, &mut ns), 1.0);
        assert_eq!(osc_sample(1, 0.5, &mut ns), -1.0);
        assert_eq!(osc_sample(1, 0.99, &mut ns), -1.0);
    }

    #[test]
    fn triangle_peaks_at_half_phase() {
        let mut ns = 1u32;
        assert!((osc_sample(2, 0.0, &mut ns) + 1.0).abs() < 1e-5);
        assert!((osc_sample(2, 0.5, &mut ns) - 1.0).abs() < 1e-5);
        assert!((osc_sample(2, 1.0, &mut ns) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn sine_matches_textbook_quarter_cycles() {
        let mut ns = 1u32;
        assert!((osc_sample(3, 0.0, &mut ns) - 0.0).abs() < 1e-5);
        assert!((osc_sample(3, 0.25, &mut ns) - 1.0).abs() < 1e-5);
        assert!((osc_sample(3, 0.75, &mut ns) + 1.0).abs() < 1e-5);
    }

    #[test]
    fn noise_mode_advances_the_state() {
        let mut ns = 1u32;
        let _ = osc_sample(99, 0.0, &mut ns);
        assert_ne!(ns, 1u32, "LCG should advance the state");
    }

    #[test]
    fn noise_output_stays_in_signed_unit_range() {
        let mut ns = 42u32;
        for _ in 0..500 {
            let v = osc_sample(99, 0.0, &mut ns);
            assert!(v.is_finite());
            assert!((-1.0..=1.0).contains(&v));
        }
    }
}

#[cfg(test)]
mod drum_voice_idx_tests {
    use crate::audio::dsp::voices::drum_voice_idx;
    use crate::state::DrumVoice;

    #[test]
    fn block_808_indices_match_expected_layout() {
        assert_eq!(drum_voice_idx(&DrumVoice::Kick808), 0);
        assert_eq!(drum_voice_idx(&DrumVoice::Snare808), 1);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatClosed808), 2);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatOpen808), 3);
        assert_eq!(drum_voice_idx(&DrumVoice::TomHi808), 4);
        assert_eq!(drum_voice_idx(&DrumVoice::TomMid808), 5);
        assert_eq!(drum_voice_idx(&DrumVoice::TomLo808), 6);
    }

    #[test]
    fn block_909_starts_after_808_block() {
        assert_eq!(drum_voice_idx(&DrumVoice::Kick909), 7);
        assert_eq!(drum_voice_idx(&DrumVoice::Snare909), 8);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatClosed909), 9);
        assert_eq!(drum_voice_idx(&DrumVoice::HihatOpen909), 10);
        assert_eq!(drum_voice_idx(&DrumVoice::Clap909), 11);
        assert_eq!(drum_voice_idx(&DrumVoice::Rim909), 12);
    }

    #[test]
    fn amen_and_gabber_are_at_the_tail() {
        assert_eq!(drum_voice_idx(&DrumVoice::Amen), 13);
        assert_eq!(drum_voice_idx(&DrumVoice::GabberKick), 14);
    }

    #[test]
    fn every_drum_voice_maps_to_a_unique_slot() {
        let voices = [
            DrumVoice::Kick808,
            DrumVoice::Snare808,
            DrumVoice::HihatClosed808,
            DrumVoice::HihatOpen808,
            DrumVoice::TomHi808,
            DrumVoice::TomMid808,
            DrumVoice::TomLo808,
            DrumVoice::Kick909,
            DrumVoice::Snare909,
            DrumVoice::HihatClosed909,
            DrumVoice::HihatOpen909,
            DrumVoice::Clap909,
            DrumVoice::Rim909,
            DrumVoice::Amen,
            DrumVoice::GabberKick,
        ];
        let mut seen = std::collections::HashSet::new();
        for v in &voices {
            assert!(seen.insert(drum_voice_idx(v)), "duplicate index for {v:?}");
        }
        assert_eq!(seen.len(), voices.len());
    }
}
