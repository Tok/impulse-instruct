// ─── tests/dsp_fx_extras_tests.rs ────────────────────────────────────────────
// Unit tests for the FX structs that live in `audio/dsp/fx_extras.rs` —
// Tier-1 and Tier-3 extras (Flanger / Limiter / SVF / Comb / Tilt / Transient
// / Exciter and Multitap / RevDelay / TapeStop / Stutter).  Split out of
// `dsp_fx_tests.rs` to keep that file under the 1000-line cap; the older
// file retains the tests for the structs in `audio/dsp/fx.rs`.

#[cfg(test)]
mod multitap_tests {
    use crate::audio::dsp::fx_extras::Multitap;

    #[test]
    fn zero_mix_zero_feedback_returns_dry() {
        let mut m = Multitap::new();
        assert_eq!(m.process(0.4, 0.3, 0.5, 0.0, 0.0, 48_000.0), 0.4);
    }

    #[test]
    fn full_wet_emits_audible_taps() {
        let mut m = Multitap::new();
        let mut nonzero = false;
        for i in 0..10_000 {
            let x = if i < 64 { 1.0 } else { 0.0 }; // impulse
            let out = m.process(x, 0.2, 0.7, 0.4, 1.0, 48_000.0);
            assert!(out.is_finite());
            if i > 200 && out.abs() > 0.05 {
                nonzero = true;
            }
        }
        assert!(nonzero, "multitap should emit echoes after the impulse");
    }
}

#[cfg(test)]
mod rev_delay_tests {
    use crate::audio::dsp::fx_extras::RevDelay;

    #[test]
    fn zero_mix_returns_dry() {
        let mut r = RevDelay::new();
        assert_eq!(r.process(0.5, 0.3, 0.0, 0.0, 48_000.0), 0.5);
    }

    #[test]
    fn full_wet_finite_under_random_input() {
        let mut r = RevDelay::new();
        for i in 0..20_000 {
            let x = (i as f32 * 0.1).sin() * 0.4;
            let out = r.process(x, 0.2, 0.5, 1.0, 48_000.0);
            assert!(out.is_finite());
        }
    }
}

#[cfg(test)]
mod tape_stop_tests {
    use crate::audio::dsp::fx_extras::TapeStop;

    #[test]
    fn mix_zero_returns_dry() {
        let mut t = TapeStop::new();
        assert_eq!(t.process(0.6, 0.0, 0.5, 48_000.0), 0.6);
    }

    #[test]
    fn full_mix_silences_output_after_long_hold() {
        let mut t = TapeStop::new();
        for i in 0..2_000 {
            t.process((i as f32 * 0.05).sin() * 0.4, 0.0, 0.5, 48_000.0);
        }
        // At mix=1 the output mask collapses to silence regardless of
        // buffer content (the (1-mix) wet-scale takes the lowpassed
        // wet down to zero).
        let out = t.process(0.5, 1.0, 0.5, 48_000.0);
        assert!(out.abs() < 1e-3, "expected silence at full stop, got {out}");
    }
}

#[cfg(test)]
mod freeze_tests {
    use crate::audio::dsp::fx_extras::Freeze;

    #[test]
    fn mix_zero_returns_dry() {
        let mut f = Freeze::new();
        assert_eq!(f.process(0.5, 0.0, 48_000.0), 0.5);
    }

    #[test]
    fn engaged_freeze_is_finite_and_eventually_audible() {
        // Push 2 s of a sine through with mix=0 (priming), then engage
        // freeze and run another 2 s.  Output should remain finite and
        // produce some signal once the captured spectrum has been
        // synthesised.
        let mut f = Freeze::new();
        let sr = 48_000.0_f32;
        // Prime.
        for i in 0..(sr as usize * 2) {
            let x = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.5;
            let _ = f.process(x, 0.0, sr);
        }
        // Engage and resynthesise.
        let mut nonzero = false;
        for i in 0..(sr as usize * 2) {
            let x = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.5;
            let out = f.process(x, 1.0, sr);
            assert!(out.is_finite());
            if i > 4096 && out.abs() > 0.01 {
                nonzero = true;
            }
        }
        assert!(nonzero, "freeze should emit audible output once engaged");
    }

    #[test]
    fn disengaging_freeze_returns_to_pass_through() {
        let mut f = Freeze::new();
        let sr = 48_000.0_f32;
        // Engage briefly.
        for i in 0..2_000 {
            let x = (i as f32 * 0.05).sin() * 0.4;
            let _ = f.process(x, 0.6, sr);
        }
        // Disengage — output should equal input within float tolerance.
        let out = f.process(0.314, 0.0, sr);
        assert!((out - 0.314).abs() < 1e-5);
    }
}

#[cfg(test)]
mod stutter_tests {
    use crate::audio::dsp::fx_extras::Stutter;

    #[test]
    fn mix_zero_returns_dry() {
        let mut s = Stutter::new();
        assert_eq!(s.process(0.3, 0.5, 0.5, 0.0, 120.0, 48_000.0), 0.3);
    }

    #[test]
    fn full_wet_stays_finite_across_multiple_periods() {
        let mut s = Stutter::new();
        for i in 0..40_000 {
            let x = (i as f32 * 0.05).sin() * 0.4;
            let out = s.process(x, 0.5, 0.6, 1.0, 120.0, 48_000.0);
            assert!(out.is_finite());
        }
    }
}
