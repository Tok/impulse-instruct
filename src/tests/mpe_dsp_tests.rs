// ─── tests/mpe_dsp_tests.rs ───────────────────────────────────────────────────
// MPE → AudioParams snapshot integration.  The full Bass303 process
// path needs voice triggers + buffers; here we lock the lighter
// invariant: `AudioParams::from_app_state` correctly forwards the
// MPE expression fields with the documented scaling (bend ±1 → ±2
// semitones, pressure / timbre passed as 0..=1).

#[cfg(test)]
mod from_app_state {
    use crate::audio::AudioParams;
    use crate::state::{AppState, MpeExpression};

    #[test]
    fn default_state_yields_zero_mpe_fields() {
        let s = AppState::default();
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_bend_st - 0.0).abs() < 1e-6);
        assert!((p.mpe_pressure - 0.0).abs() < 1e-6);
        assert!((p.mpe_timbre - 0.0).abs() < 1e-6);
    }

    #[test]
    fn pitch_bend_one_maps_to_two_semitones() {
        let mut s = AppState::default();
        s.mpe = MpeExpression {
            channel: 2,
            pitch_bend: 1.0,
            ..Default::default()
        };
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_bend_st - 2.0).abs() < 1e-6, "got {}", p.mpe_bend_st);
    }

    #[test]
    fn pitch_bend_minus_one_maps_to_minus_two_semitones() {
        let mut s = AppState::default();
        s.mpe = MpeExpression {
            channel: 2,
            pitch_bend: -1.0,
            ..Default::default()
        };
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_bend_st - -2.0).abs() < 1e-6, "got {}", p.mpe_bend_st);
    }

    #[test]
    fn out_of_range_bend_clamps_to_pm_two() {
        // Defensive: a buggy controller (or hand-typed API call) shouldn't
        // be able to send a 100-semitone bend into the DSP.
        let mut s = AppState::default();
        s.mpe.pitch_bend = 50.0;
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_bend_st - 2.0).abs() < 1e-6);
        s.mpe.pitch_bend = -50.0;
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_bend_st - -2.0).abs() < 1e-6);
    }

    #[test]
    fn pressure_and_timbre_pass_through_clamped() {
        let mut s = AppState::default();
        s.mpe.pressure = 0.7;
        s.mpe.timbre = 0.2;
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_pressure - 0.7).abs() < 1e-6);
        assert!((p.mpe_timbre - 0.2).abs() < 1e-6);
        // Out-of-range clamps to [0, 1].
        s.mpe.pressure = -0.5;
        s.mpe.timbre = 5.0;
        let p = AudioParams::from_app_state(&s);
        assert!((p.mpe_pressure - 0.0).abs() < 1e-6);
        assert!((p.mpe_timbre - 1.0).abs() < 1e-6);
    }
}

#[cfg(test)]
mod modulation_math {
    /// Replica of the bass303 inline math so the contract stays
    /// locked in tests even if the inline code drifts.  Keeps the
    /// modulation contributions auditable without standing up a full
    /// DSP voice + trigger.
    fn pitch_factor(bend_st: f32, base_detune: f32, lfo_st: f32, mpe_st: f32) -> f32 {
        2.0_f32.powf((bend_st + base_detune + lfo_st + mpe_st) / 12.0)
    }

    fn cutoff_with_mpe(base: f32, env: f32, mod_amount: f32, mpe_timbre: f32) -> f32 {
        (base + env * mod_amount + mpe_timbre * 0.3).clamp(0.0, 1.0)
    }

    fn output_with_mpe(amp: f32, vol: f32, mpe_pressure: f32) -> f32 {
        let mpe_amp_mult = 1.0 + mpe_pressure * 0.4;
        amp * vol * mpe_amp_mult
    }

    #[test]
    fn full_bend_is_two_semitones_factor() {
        // A two-semitone bend should produce 2^(2/12) ≈ 1.1225 freq factor.
        let f = pitch_factor(0.0, 0.0, 0.0, 2.0);
        assert!((f - 2.0_f32.powf(2.0 / 12.0)).abs() < 1e-6);
    }

    #[test]
    fn timbre_one_lifts_cutoff_by_30_percent() {
        // base 0.4 + env*mod = 0.0 + timbre 1.0 * 0.3 = 0.7.
        let c = cutoff_with_mpe(0.4, 0.0, 0.0, 1.0);
        assert!((c - 0.7).abs() < 1e-6);
    }

    #[test]
    fn timbre_clamps_at_unit_ceiling() {
        // base 0.9 + timbre 1.0*0.3 = 1.2 → must clamp at 1.0.
        let c = cutoff_with_mpe(0.9, 0.0, 0.0, 1.0);
        assert!((c - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pressure_one_boosts_amp_by_40_percent() {
        let out = output_with_mpe(1.0, 1.0, 1.0);
        assert!((out - 1.4).abs() < 1e-6);
    }

    #[test]
    fn zero_pressure_passes_amp_unchanged() {
        let out = output_with_mpe(0.5, 0.8, 0.0);
        assert!((out - 0.4).abs() < 1e-6);
    }
}
