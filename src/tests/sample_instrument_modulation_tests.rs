// ─── tests/sample_instrument_modulation_tests.rs ────────────────────────────
// SF2 modulation-surface tests for the SampleInstrument voice — mod
// LFO (pitch / filter / volume), vib LFO (pitch), and the modulation
// envelope (pitch / filter).  Extracted from
// `sample_instrument_sfz_tests.rs` once the SF2 mod-env ship pushed
// that file past the 1000-line cap; the modulation tests are a
// cohesive group that's easier to reason about in isolation from the
// region-selection / loop / ADSR coverage.

#[cfg(test)]
mod sample_instrument_modulation_tests {
    use std::sync::Arc;

    use crate::audio::dsp::sample_instrument::{SampleInstrumentVoice, SfzRegionRuntime};
    use crate::state::SfzRegion;

    fn make_params() -> crate::audio::dsp::AudioParams {
        let s = crate::state::AppState::default();
        crate::audio::dsp::AudioParams::from_app_state(&s)
    }

    /// modLfoToPitch / vibLfoToPitch: with non-zero pitch depth the
    /// slot's read rate should fluctuate cycle-by-cycle as the LFO
    /// wraps.  We compare the running max + min output amplitudes
    /// against a no-LFO baseline; a working LFO produces a different
    /// envelope (the rate change shifts where the playhead lands
    /// each frame, perturbing the interpolated output).
    #[test]
    fn region_lfo_pitch_modulates_rate() {
        // Constant-pitch sinusoidal source so the LFO's effect on
        // the read rate is audible as cycle-by-cycle drift in the
        // output sample.
        let buf: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let arc = Arc::new(buf);

        // Helper to run the voice for N samples with a given LFO depth
        // and return the output trace.
        let run = |depth_cents: f32| -> Vec<f32> {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(2047),
                mod_lfo_freq_hz: 6.0,
                mod_lfo_delay_s: 0.0,
                mod_lfo_to_pitch_cents: depth_cents,
                ..Default::default()
            };
            r.sample_path = std::path::PathBuf::from("/lfo.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            (0..8_000).map(|_| v.process(sr, &p)).collect()
        };

        let dry = run(0.0);
        let wet = run(400.0); // ±400 cents — heavy mod-LFO depth
        // Sum of squared differences > 0 means the LFO actually
        // perturbed the playback.  A bit-equal comparison would be
        // fragile, but a meaningful divergence is easy to assert.
        let diff: f32 = dry
            .iter()
            .zip(wet.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        assert!(
            diff > 0.5,
            "modLfoToPitch should perturb the rendered output (diff {diff})"
        );
    }

    /// LFO delay generator: with a non-zero delay the LFO depth is
    /// silent until elapsed.  The early portion of the trace should
    /// be bit-equal to a no-LFO baseline up to the delay window.
    #[test]
    fn region_lfo_delay_holds_modulation_off() {
        let buf: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let arc = Arc::new(buf);

        let run = |delay_s: f32| -> Vec<f32> {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(2047),
                vib_lfo_freq_hz: 6.0,
                vib_lfo_delay_s: delay_s,
                vib_lfo_to_pitch_cents: 600.0,
                ..Default::default()
            };
            r.sample_path = std::path::PathBuf::from("/delay.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            (0..2_000).map(|_| v.process(sr, &p)).collect()
        };

        // 30 ms delay → first 1440 samples (at 48 kHz) should match
        // a comparison with very long delay (LFO never fires).
        let delayed = run(0.030);
        let off = run(10.0);
        let early_diff: f32 = delayed[..1_000]
            .iter()
            .zip(off[..1_000].iter())
            .map(|(a, b)| (a - b).abs())
            .sum();
        assert!(
            early_diff < 1e-3,
            "LFO delay should suppress modulation early (early diff {early_diff})"
        );
    }

    /// modLfoToVolume: positive depth produces a tremolo whose RMS
    /// envelope visibly differs from the no-mod baseline.  The RMS
    /// over a full LFO cycle is *higher* than the unmodulated DC
    /// level because `10^(x/200)` is convex — `mean(10^(±cb/200))
    /// > 10^0` — so the audible effect is a measurable RMS lift on
    /// top of the periodic swing.  Asserting RMS-divergence is more
    /// robust than peak-comparison: even if an SVF or oversample
    /// path nudges peaks, the tremolo's energy-domain footprint
    /// remains.
    #[test]
    fn region_lfo_volume_modulates_amplitude() {
        // Constant-DC source so the only thing varying the output
        // amplitude is the volume LFO; any RMS divergence comes
        // straight from `modLfoToVolume`.
        let buf: Vec<f32> = vec![0.5_f32; 4096];
        let arc = Arc::new(buf);

        let run = |depth_cb: f32| -> f32 {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(4095),
                mod_lfo_freq_hz: 8.0,
                mod_lfo_delay_s: 0.0,
                mod_lfo_to_volume_cb: depth_cb,
                ..Default::default()
            };
            r.sample_path = std::path::PathBuf::from("/lfo_vol.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            let trace: Vec<f32> = (0..12_000).map(|_| v.process(sr, &p)).collect();
            // RMS over 250 ms → covers two LFO cycles at 8 Hz.
            (trace.iter().map(|s| s * s).sum::<f32>() / trace.len() as f32).sqrt()
        };

        let dry = run(0.0);
        let wet = run(200.0); // 20 dB peak-to-peak swing — heavy tremolo
        let ratio = wet / dry.max(1e-6);
        // 10^(±1) RMS ≈ 1 + sinh-style lift; expect a noticeable
        // divergence rather than bit-equal output.  Asserting > 5 %
        // gives a wide safety margin while still failing if the
        // modulation path silently no-ops.
        assert!(
            (ratio - 1.0).abs() > 0.05,
            "modLfoToVolume should perturb RMS amplitude (ratio {ratio})"
        );
    }

    /// modLfoToFilterFc: when the region carries a low-pass filter,
    /// a heavy LFO depth on the cutoff sweeps the filter open + shut
    /// each cycle, producing a measurably different output trace
    /// from the no-mod baseline.  Compares sum-of-squared-deltas vs
    /// a depth=0 reference; a working modulation path produces
    /// non-trivial divergence.
    #[test]
    fn region_lfo_filter_modulates_cutoff() {
        // Square-ish source so the filter has rich harmonics to
        // sculpt as the cutoff sweeps.
        let buf: Vec<f32> = (0..4096)
            .map(|i| if (i / 32) % 2 == 0 { 0.4 } else { -0.4 })
            .collect();
        let arc = Arc::new(buf);

        let run = |depth_cents: f32| -> Vec<f32> {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(4095),
                // Region filter — cutoff in the audible mid so the
                // LFO sweep crosses harmonics on both sides.
                cutoff_hz: 800.0,
                resonance_db: 6.0,
                fil_type: Some(crate::state::sfz::SfzFilType::Lpf2p),
                mod_lfo_freq_hz: 6.0,
                mod_lfo_delay_s: 0.0,
                mod_lfo_to_filter_fc_cents: depth_cents,
                ..Default::default()
            };
            r.sample_path = std::path::PathBuf::from("/lfo_fc.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            (0..12_000).map(|_| v.process(sr, &p)).collect()
        };

        let dry = run(0.0);
        let wet = run(2400.0); // ±2 octaves of cutoff swing
        let diff: f32 = dry
            .iter()
            .zip(wet.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        assert!(
            diff > 0.5,
            "modLfoToFilterFc should sculpt the filtered output (diff {diff})"
        );
    }

    /// modEnvToPitch: an envelope with a heavy attack-stage pitch
    /// depth produces a measurably different output trace than the
    /// no-mod baseline.  Same RMS-divergence pattern as the LFO
    /// tests — assert non-trivial sum-of-squared-deltas to confirm
    /// the env value is plumbing through to the read rate.
    #[test]
    fn region_mod_env_pitch_modulates_rate() {
        let buf: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let arc = Arc::new(buf);

        let run = |depth_cents: f32| -> Vec<f32> {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(2047),
                // Slow attack so the env value crosses a wide range
                // during the test window; long sustain hold keeps
                // the env value high without re-triggering.
                mod_env_attack_s: 0.080,
                mod_env_decay_s: 0.001,
                mod_env_sustain_level: 1.0,
                mod_env_to_pitch_cents: depth_cents,
                ..Default::default()
            };
            r.sample_path = std::path::PathBuf::from("/mod_env_pitch.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            (0..8_000).map(|_| v.process(sr, &p)).collect()
        };

        let dry = run(0.0);
        let wet = run(800.0); // ±8 semitones
        let diff: f32 = dry
            .iter()
            .zip(wet.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        assert!(
            diff > 0.5,
            "modEnvToPitch should shift the rendered output (diff {diff})"
        );
    }

    /// modEnvToFilterFc: the envelope sweeps the filter cutoff.
    /// Heavy depth + a fast attack + slow decay produces a clear
    /// "filter sweep" signature (the classic SF2 plucked-string
    /// articulation) that diverges from the no-mod baseline.
    #[test]
    fn region_mod_env_filter_modulates_cutoff() {
        // Square-ish source so the filter has rich harmonics to
        // sculpt as the env sweeps across them.
        let buf: Vec<f32> = (0..4096)
            .map(|i| if (i / 32) % 2 == 0 { 0.4 } else { -0.4 })
            .collect();
        let arc = Arc::new(buf);

        let run = |depth_cents: f32| -> Vec<f32> {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(4095),
                // Region filter — base cutoff in mid so the env can
                // sweep up + back down across audible harmonics.
                cutoff_hz: 600.0,
                resonance_db: 6.0,
                fil_type: Some(crate::state::sfz::SfzFilType::Lpf2p),
                mod_env_attack_s: 0.030,
                mod_env_decay_s: 0.150,
                mod_env_sustain_level: 0.0,
                mod_env_to_filter_fc_cents: depth_cents,
                ..Default::default()
            };
            r.sample_path = std::path::PathBuf::from("/mod_env_fc.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            (0..12_000).map(|_| v.process(sr, &p)).collect()
        };

        let dry = run(0.0);
        let wet = run(3600.0); // ±3 octaves of cutoff swing
        let diff: f32 = dry
            .iter()
            .zip(wet.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum();
        assert!(
            diff > 0.5,
            "modEnvToFilterFc should sweep the filtered output (diff {diff})"
        );
    }

    /// Region with no mod-env depths declared must take the same
    /// non-modulated path as the existing pre-modenv builds — no
    /// allocation of `region_mod_env`, no audible perturbation.  We
    /// can't observe the Option directly from outside the voice,
    /// but we *can* assert the output trace is bit-equal to a
    /// region built without the new fields touching the default.
    #[test]
    fn region_without_mod_env_is_bit_equal_to_default_path() {
        let buf: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.07).sin() * 0.4).collect();
        let arc = Arc::new(buf);

        let run = |with_zero_depths: bool| -> Vec<f32> {
            let mut v = SampleInstrumentVoice::new();
            let mut r = SfzRegion {
                lokey: 60,
                hikey: 60,
                pitch_keycenter: 60,
                loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
                loop_start: Some(0),
                loop_end: Some(1023),
                ..Default::default()
            };
            if with_zero_depths {
                // Times are non-default but depths are zero, so the
                // gate condition (`abs(to_*_cents) > 0.5`) fails and
                // the env never instantiates.  Ensures the gate
                // really is depth-driven, not time-driven.
                r.mod_env_attack_s = 0.050;
                r.mod_env_decay_s = 0.100;
            }
            r.sample_path = std::path::PathBuf::from("/no_mod_env.wav");
            v.load_sfz(vec![SfzRegionRuntime {
                region: r,
                samples: arc.clone(),
            }]);
            let mut p = make_params();
            p.sample_loop_enabled = false;
            p.sample_volume = 1.0;
            p.sample_attack = 0.0;
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                1.0,
                0.0,
                0.0,
                0.0,
            );
            let sr = 48_000.0;
            (0..3_000).map(|_| v.process(sr, &p)).collect()
        };

        let baseline = run(false);
        let with_times = run(true);
        for (a, b) in baseline.iter().zip(with_times.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "non-zero env times with zero depths should be inert (a {a}, b {b})"
            );
        }
    }
}
