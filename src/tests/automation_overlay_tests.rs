// ─── tests/automation_overlay_tests.rs ───────────────────────────────────────
// Pure helpers behind the sequencer's automation-lane sparkline.
// Phase math + curve sampling — no UI / egui involved.

#[cfg(test)]
mod phase_advance {
    use crate::state::{free_phase_per_step, rate_knob_to_hz, synced_phase_per_step};

    #[test]
    fn rate_knob_zero_floors_above_zero_hz() {
        // Rate knob at 0 must NOT yield 0 Hz — the LFO would be a flat
        // line at the offset value, indistinguishable from "off".  We
        // floor at 0.01 Hz so the user still sees a slow sweep.
        let hz = rate_knob_to_hz(0.0);
        assert!(hz >= 0.01 && hz < 0.02, "rate=0 → ~0.01 Hz, got {hz}");
    }

    #[test]
    fn rate_knob_max_caps_at_20_hz() {
        let hz = rate_knob_to_hz(1.0);
        assert!(hz > 19.9 && hz <= 20.0, "rate=1 → ~20 Hz, got {hz}");
    }

    #[test]
    fn rate_knob_quartic_shape_keeps_low_end_fine() {
        // Quartic shape: a knob at 0.5 should land far below the
        // midpoint of 0.01..20, so the user has fine control near the
        // bottom of the range.
        let hz = rate_knob_to_hz(0.5);
        assert!(hz < 5.0, "rate=0.5 should be slow (<5 Hz), got {hz}");
    }

    #[test]
    fn synced_phase_one_bar_at_16th_grid_advances_one_sixteenth() {
        // step_division=4 (16ths), sync_beats=4 (one bar = 4 quarters).
        // Sixteen 16th-note steps fit in one bar → phase / step = 1/16.
        let phase = synced_phase_per_step(4, 4.0);
        assert!((phase - 1.0 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn synced_phase_quarter_period_at_16th_grid_advances_one_quarter() {
        // sync_beats=1 means one quarter-note per cycle; on a 16th
        // grid that's 4 steps per cycle → 0.25 phase per step.
        let phase = synced_phase_per_step(4, 1.0);
        assert!((phase - 0.25).abs() < 1e-6);
    }

    #[test]
    fn free_phase_at_120_bpm_16th_with_1_hz_lfo() {
        // step = 60 / (120 * 4) = 0.125 s.  Phase per step = rate * 0.125.
        let phase = free_phase_per_step(1.0, 120.0, 4);
        assert!((phase - 0.125).abs() < 1e-6);
    }

    #[test]
    fn synced_phase_clamps_low_sync_beats() {
        // sync_beats below 1/16 (0.0625) is meaningless and would
        // produce gigantic per-step phase advances; helper clamps it.
        let phase = synced_phase_per_step(4, 0.0001);
        let clamped = synced_phase_per_step(4, 0.0625);
        assert!((phase - clamped).abs() < 1e-6);
    }
}

#[cfg(test)]
mod curve_sampling {
    use crate::state::{BassLfoTarget, BassState, bass_lfo_curve_for_view};

    fn synth_with_lfo(
        target: BassLfoTarget,
        depth: f32,
        rate: f32,
        sync: bool,
        sync_beats: f32,
        phase_offset: f32,
        waveform: crate::state::LfoWaveform,
    ) -> BassState {
        BassState {
            lfo_target: target,
            lfo_depth: depth,
            lfo_rate: rate,
            lfo_bpm_sync: sync,
            lfo_sync_beats: sync_beats,
            lfo_phase: phase_offset,
            lfo_waveform: waveform,
            ..BassState::default()
        }
    }

    #[test]
    fn target_off_yields_all_zeros() {
        let synth = synth_with_lfo(
            BassLfoTarget::Off,
            1.0,
            0.5,
            false,
            1.0,
            0.0,
            crate::state::LfoWaveform::Sine,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 16);
        assert!(curve.iter().all(|&v| v.abs() < 1e-6));
    }

    #[test]
    fn zero_depth_yields_all_zeros() {
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            0.0,
            0.5,
            true,
            4.0,
            0.0,
            crate::state::LfoWaveform::Sine,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 16);
        assert!(curve.iter().all(|&v| v.abs() < 1e-6));
    }

    #[test]
    fn synced_sine_one_bar_completes_one_cycle_in_16_steps() {
        // sync_beats=4, step_division=4 → phase advances 1/16 per step.
        // 16 steps = full cycle → start at 0, peak around step 4,
        // crossing at step 8, trough at step 12.
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            1.0,
            0.5,
            true,
            4.0,
            0.0,
            crate::state::LfoWaveform::Sine,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 16);
        assert_eq!(curve.len(), 16);
        // step 0 ≈ 0
        assert!(curve[0].abs() < 1e-3, "step 0 ≈ 0, got {}", curve[0]);
        // step 4 ≈ +1 (peak)
        assert!(curve[4] > 0.99, "step 4 ≈ +1, got {}", curve[4]);
        // step 8 ≈ 0 (zero crossing)
        assert!(curve[8].abs() < 1e-3, "step 8 ≈ 0, got {}", curve[8]);
        // step 12 ≈ -1 (trough)
        assert!(curve[12] < -0.99, "step 12 ≈ -1, got {}", curve[12]);
    }

    #[test]
    fn depth_scales_amplitude() {
        let depth = 0.4_f32;
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            depth,
            0.5,
            true,
            4.0,
            0.25, // start at peak
            crate::state::LfoWaveform::Sine,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 1);
        // First sample with phase_offset=0.25 = sin(2π/4) = 1, scaled
        // by depth → 0.4.
        assert!((curve[0] - depth).abs() < 1e-3, "got {}", curve[0]);
    }

    #[test]
    fn square_wave_alternates_between_plus_and_minus_depth() {
        // sync_beats=1 (one quarter per cycle), 16th grid → 4 steps
        // per cycle.  Square is +1 for half, -1 for half → first 2
        // steps +depth, next 2 -depth.
        let depth = 0.5_f32;
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            depth,
            0.5,
            true,
            1.0,
            0.0,
            crate::state::LfoWaveform::Square,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 4);
        assert!((curve[0] - depth).abs() < 1e-3);
        assert!((curve[1] - depth).abs() < 1e-3);
        assert!((curve[2] + depth).abs() < 1e-3);
        assert!((curve[3] + depth).abs() < 1e-3);
    }

    #[test]
    fn sample_and_hold_returns_zero_in_v1() {
        // S&H needs the DSP's noise source for real values; the
        // overlay can't fake them without diverging from playback.
        // V1 paints flat — verify that contract.
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            1.0,
            0.5,
            true,
            1.0,
            0.0,
            crate::state::LfoWaveform::SampleAndHold,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 16);
        assert!(curve.iter().all(|&v| v.abs() < 1e-6));
    }

    #[test]
    fn page_offset_advances_phase_correctly() {
        // page_start_step shifts the sampling window — at sync_beats=4
        // (full bar per cycle), step 16 of page 1 == step 0 of page 0.
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            1.0,
            0.5,
            true,
            4.0,
            0.0,
            crate::state::LfoWaveform::Sine,
        );
        let page0 = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 16);
        let page1 = bass_lfo_curve_for_view(&synth, 120.0, 4, 16, 16);
        // Page 1 step 0 starts where page 0 ended (full cycle later)
        // → same value as page 0 step 0.
        assert!((page1[0] - page0[0]).abs() < 1e-3);
    }

    #[test]
    fn output_is_clamped_to_unit_interval() {
        let synth = synth_with_lfo(
            BassLfoTarget::FilterCutoff,
            1.0,
            0.5,
            true,
            4.0,
            0.0,
            crate::state::LfoWaveform::Sine,
        );
        let curve = bass_lfo_curve_for_view(&synth, 120.0, 4, 0, 64);
        for v in &curve {
            assert!(*v >= -1.0 && *v <= 1.0, "out of range: {v}");
        }
    }
}
