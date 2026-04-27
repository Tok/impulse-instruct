// ─── tests/sample_instrument_sfz_tests.rs ────────────────────────────────────
// SFZ-mode + per-region override tests for the SampleInstrument
// voice.  Extracted from `sample_instrument_tests.rs` once that file
// crossed the 1000-line cap; everything related to multi-region SFZ
// playback (zone matching, velocity layers, round-robin, volume_db,
// crossfade gain, per-region ADSR, per-region filter) lives here.

#[cfg(test)]
mod sample_instrument_sfz_mode_tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::audio::dsp::sample_instrument::{SampleInstrumentVoice, SfzRegionRuntime};
    use crate::state::SfzRegion;

    fn region(lokey: u8, hikey: u8, pitch_keycenter: u8, sample_value: f32) -> SfzRegionRuntime {
        let mut r = SfzRegion {
            lokey,
            hikey,
            pitch_keycenter,
            ..Default::default()
        };
        r.sample_path = PathBuf::from("/synthetic.wav");
        // Constant-value buffer — every read returns `sample_value` so a
        // tested process loop produces a clear, region-identifiable
        // output without needing real audio decoding.
        let buf: Vec<f32> = vec![sample_value; 1024];
        SfzRegionRuntime {
            region: r,
            samples: Arc::new(buf),
        }
    }

    fn make_params() -> crate::audio::dsp::AudioParams {
        let s = crate::state::AppState::default();
        crate::audio::dsp::AudioParams::from_app_state(&s)
    }

    #[test]
    fn load_sfz_switches_voice_into_multisample_mode() {
        let mut v = SampleInstrumentVoice::new();
        assert!(!v.is_sfz_mode());
        v.load_sfz(vec![region(36, 47, 42, 0.1)]);
        assert!(v.is_sfz_mode());
        // Loading a single WAV should reset back to single-mode.
        v.load(Arc::new(vec![0.0; 100]));
        assert!(!v.is_sfz_mode());
    }

    #[test]
    fn trigger_picks_region_by_note() {
        // Two distinct regions: low band emits 0.2, high band emits
        // 0.7.  Triggering inside the low band should produce 0.2;
        // inside the high band should produce 0.7.  Confirms region
        // selection actually drives the sample buffer.
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![region(36, 47, 42, 0.2), region(48, 72, 60, 0.7)]);
        let p = make_params();
        v.trigger(
            42,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        // First sample after trigger — attack starts at 0 envelope, so
        // accumulate a few samples and check the running max.
        let mut max_lo = 0.0_f32;
        for _ in 0..1000 {
            max_lo = max_lo.max(v.process(48_000.0, &p).abs());
        }
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let mut max_hi = 0.0_f32;
        for _ in 0..1000 {
            max_hi = max_hi.max(v.process(48_000.0, &p).abs());
        }
        // Output ratio should track the buffer values' ratio (0.7 / 0.2).
        assert!(
            max_lo < max_hi,
            "low region {max_lo} should be quieter than high region {max_hi}"
        );
    }

    #[test]
    fn out_of_range_note_silences_voice() {
        // Region covers c3..=c4 only; triggering at c5 falls outside.
        // Voice must produce silence rather than chasing a stale
        // buffer or reusing the previous region.
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![region(48, 60, 54, 0.5)]);
        v.trigger(
            72, // c5, outside region
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn overlapping_regions_fire_parallel_slots() {
        // Two regions intentionally cover the same note — a layered
        // patch (e.g. close + room mics).  Both should fire on
        // separate polyphony slots so the layered sound plays.
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![
            region(48, 60, 54, 0.3),
            region(48, 60, 54, 0.4), // same key range, different "layer"
        ]);
        v.trigger(
            54,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert_eq!(
            v.active_voice_count(),
            2,
            "overlapping SFZ regions should fire one slot each"
        );
    }

    #[test]
    fn non_overlapping_regions_pick_only_the_matching_one() {
        // Standard Salamander-style mapping: contiguous, non-overlapping
        // bands.  Only one region should fire per note.
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![
            region(48, 53, 50, 0.2), // C3..F3
            region(54, 59, 56, 0.7), // F#3..B3
            region(60, 71, 65, 0.5), // C4..B4
        ]);
        v.trigger(
            56,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert_eq!(
            v.active_voice_count(),
            1,
            "non-overlapping mapping should fire exactly one region per note"
        );
    }

    #[test]
    fn velocity_layer_filters_by_accent() {
        // Two layers at the same key range, different velocity bands.
        // Default trigger uses accent=0.0 → vel=64 → matches the upper
        // layer (lovel=64) only.  Accent=1.0 still vel<128 → upper.
        // Stage-5 accent→velocity mapping deliberately spans 64..127
        // so sequencer-driven hits land in the mf..fff bracket.
        let mut soft = region(60, 60, 60, 0.3);
        soft.region.lovel = 0;
        soft.region.hivel = 63;
        let mut loud = region(60, 60, 60, 0.6);
        loud.region.lovel = 64;
        loud.region.hivel = 127;
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![soft, loud]);
        // Accent 0 → vel 64 → loud only.
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert_eq!(
            v.active_voice_count(),
            1,
            "velocity-layer filter should narrow to one layer per trigger"
        );
    }

    #[test]
    fn round_robin_cycles_through_seq_positions() {
        // Three regions cover the same key/vel.  Each has seq_length=3
        // with seq_position 1/2/3.  Three triggers should hit each
        // region exactly once across the cycle (order driven by the
        // global rr_counter).
        let mut rr1 = region(60, 60, 60, 0.1);
        rr1.region.seq_position = 1;
        rr1.region.seq_length = 3;
        let mut rr2 = region(60, 60, 60, 0.2);
        rr2.region.seq_position = 2;
        rr2.region.seq_length = 3;
        let mut rr3 = region(60, 60, 60, 0.3);
        rr3.region.seq_position = 3;
        rr3.region.seq_length = 3;
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![rr1, rr2, rr3]);
        // Each trigger should fire exactly one region (the one whose
        // seq_position matches the counter).
        for _ in 0..3 {
            v.trigger(
                60,
                crate::audio::dsp::TuningSystem::TwelveTet,
                0.0,
                0.0,
                0.0,
                0.0,
            );
        }
        // All three triggers fired separate slots — total active = 3.
        assert_eq!(
            v.active_voice_count(),
            3,
            "round-robin should fire one region per trigger across the cycle"
        );
    }

    #[test]
    fn region_volume_db_scales_output() {
        // Two regions covering the same note (different velocity bands
        // would normally disambiguate, but for this test we just hand
        // back the first match).  The volume_db on the matched region
        // should multiply the output.
        let mut quiet_region = region(60, 60, 60, 0.5);
        quiet_region.region.volume_db = -6.0; // ~0.5× linear
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![quiet_region]);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params();
        let mut max_out = 0.0_f32;
        for _ in 0..1000 {
            max_out = max_out.max(v.process(48_000.0, &p).abs());
        }
        // -6 dB ≈ 0.501× of buffer value (0.5) × default volume (0.7)
        // = ~0.175.  Allow generous slop for ADSR ramp + accent.
        assert!(max_out < 0.3, "expected -6 dB attenuation; got {max_out}");
    }

    #[test]
    fn region_ampeg_release_overrides_global_knob() {
        // Region with ampeg_release_s = 0.001 should fall to silence
        // within ~50 ms after gate-off, regardless of how long the
        // global `sample_release` knob is set.
        let mut r = region(60, 60, 60, 0.5);
        r.region.ampeg_release_s = 0.001; // 1 ms — effectively instant
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![r]);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        // Baseline: sample_release = 1.0 (long release, ~2 s).
        let mut p = make_params();
        p.sample_release = 1.0;
        for _ in 0..200 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        // Region release of 1 ms = 48 samples per time-constant at
        // 48 k.  exp(-N/48) drops below ~0.003 (the effective audible
        // floor after volume × sample-magnitude scaling) around
        // sample 300.  500-iteration cap leaves plenty of slack.
        let mut decayed = false;
        for _ in 0..500 {
            let out = v.process(48_000.0, &p).abs();
            if out < 0.001 {
                decayed = true;
                break;
            }
        }
        assert!(
            decayed,
            "region's 1 ms ampeg_release_s should override the long global knob"
        );
    }

    #[test]
    fn region_filter_override_attenuates_highs() {
        // Region with a 100 Hz LPF should attenuate a high-frequency
        // sine that played through a no-filter region passes
        // unchanged.  Cross-checks the per-region filter override
        // path against a baseline region without fil_type set.
        // Build two regions sharing the same buffer but different
        // filters.
        use crate::state::sfz::SfzFilType;
        let buf: Vec<f32> = (0..2048)
            .map(|i| {
                // 8 kHz sine at 48 kHz — well above a 100 Hz LPF.
                (i as f32 * 2.0 * std::f32::consts::PI * 8000.0 / 48_000.0).sin() * 0.7
            })
            .collect();
        let arc = std::sync::Arc::new(buf);

        // Filtered region — 100 Hz LPF should crush the 8 kHz tone.
        let mut filtered = SfzRegion {
            lokey: 60,
            hikey: 60,
            pitch_keycenter: 60,
            cutoff_hz: 100.0,
            resonance_db: 0.0,
            fil_type: Some(SfzFilType::Lpf2p),
            ..Default::default()
        };
        filtered.sample_path = std::path::PathBuf::from("/synth.wav");
        let filtered_rt = SfzRegionRuntime {
            region: filtered,
            samples: arc.clone(),
        };

        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![filtered_rt]);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params();
        // Skip attack ramp.
        for _ in 0..512 {
            let _ = v.process(48_000.0, &p);
        }
        let mut peak_filtered = 0.0_f32;
        for _ in 0..1024 {
            peak_filtered = peak_filtered.max(v.process(48_000.0, &p).abs());
        }

        // Same setup but no filter — passes through.
        let mut no_filter = SfzRegion {
            lokey: 60,
            hikey: 60,
            pitch_keycenter: 60,
            ..Default::default()
        };
        no_filter.sample_path = std::path::PathBuf::from("/synth.wav");
        let no_filter_rt = SfzRegionRuntime {
            region: no_filter,
            samples: arc,
        };
        let mut v2 = SampleInstrumentVoice::new();
        v2.load_sfz(vec![no_filter_rt]);
        v2.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        for _ in 0..512 {
            let _ = v2.process(48_000.0, &p);
        }
        let mut peak_dry = 0.0_f32;
        for _ in 0..1024 {
            peak_dry = peak_dry.max(v2.process(48_000.0, &p).abs());
        }
        assert!(
            peak_filtered < peak_dry * 0.4,
            "100 Hz LPF should attenuate 8 kHz signal — filtered peak {peak_filtered}, dry peak {peak_dry}"
        );
    }

    #[test]
    fn region_with_default_ampeg_uses_global_knobs() {
        // Region with all ampeg fields at SfzRegion defaults
        // (attack/decay/release_s = 0, sustain_pct = 100) should
        // fall through to the global knob path.  Set the global
        // release short and confirm the region path picks it up.
        let r = region(60, 60, 60, 0.5);
        let mut v = SampleInstrumentVoice::new();
        v.load_sfz(vec![r]);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let mut p = make_params();
        p.sample_release = 0.0; // → 5 ms (lo end of the knob map)
        for _ in 0..200 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        let mut decayed = false;
        for _ in 0..2000 {
            let out = v.process(48_000.0, &p).abs();
            if out < 0.001 {
                decayed = true;
                break;
            }
        }
        assert!(
            decayed,
            "default ampeg should use global knob — short release should fire"
        );
    }

    /// LoopContinuous: with global loop disabled, the per-region loop
    /// override should still keep the slot reading inside the loop
    /// window rather than draining to silence at sample end.  The
    /// previous behaviour (always honour the global `loop_enabled`
    /// knob) made SF2 sustained patches with `sampleModes = 1` go
    /// silent on long-held notes.
    #[test]
    fn region_loop_continuous_overrides_global_loop_disabled() {
        let mut v = SampleInstrumentVoice::new();
        // Region carries an explicit LoopContinuous loop window; the
        // sample buffer is short enough that without looping the
        // playhead would have walked past the end inside ~64 ms.
        let mut r = SfzRegion {
            lokey: 60,
            hikey: 60,
            pitch_keycenter: 60,
            loop_mode: crate::state::sfz::SfzLoopMode::LoopContinuous,
            loop_start: Some(64),
            loop_end: Some(192),
            ..Default::default()
        };
        r.sample_path = std::path::PathBuf::from("/loop.wav");
        let buf: Vec<f32> = (0..256).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        v.load_sfz(vec![SfzRegionRuntime {
            region: r,
            samples: Arc::new(buf),
        }]);

        // Disable global loop so the only path that keeps the slot
        // alive is the per-region override.
        let mut p = make_params();
        p.sample_loop_enabled = false;
        p.sample_volume = 1.0;
        p.sample_attack = 0.0;
        p.sample_release = 0.5;

        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            1.0,
            0.0,
            0.0,
            0.0,
        );
        // Run ~250 ms — far longer than the 256-sample buffer would
        // play unlooped at 48 kHz.  Active output post-buffer-end
        // proves the loop override is the load-bearing path.
        let sr = 48_000.0;
        let mut total_energy = 0.0_f32;
        for _ in 0..12_000 {
            let s = v.process(sr, &p);
            total_energy += s * s;
        }
        assert!(
            total_energy > 0.05,
            "region LoopContinuous should keep producing audio (energy {total_energy})"
        );
    }

    /// LoopSustain: while gate held, slot loops inside the region's
    /// window.  After gate_off the loop is released and the playhead
    /// spills past `le` toward sample end — the standard SF2 "loop
    /// until release" semantics.
    #[test]
    fn region_loop_sustain_releases_after_gate_off() {
        let mut v = SampleInstrumentVoice::new();
        let mut r = SfzRegion {
            lokey: 60,
            hikey: 60,
            pitch_keycenter: 60,
            loop_mode: crate::state::sfz::SfzLoopMode::LoopSustain,
            loop_start: Some(32),
            loop_end: Some(96),
            ..Default::default()
        };
        r.sample_path = std::path::PathBuf::from("/sustain.wav");
        let buf: Vec<f32> = vec![0.4; 256];
        v.load_sfz(vec![SfzRegionRuntime {
            region: r,
            samples: Arc::new(buf),
        }]);

        let mut p = make_params();
        p.sample_loop_enabled = false;
        p.sample_volume = 1.0;
        p.sample_attack = 0.0;
        // Snappy release so the post-gate-off ADSR doesn't dominate
        // the test budget — we're checking the loop release behaviour,
        // not the envelope's tail.
        p.sample_release = 0.01;

        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            1.0,
            0.0,
            0.0,
            0.0,
        );
        // While gate held, the slot loops inside [32, 96] — the
        // playhead should still be alive after many buffer-lengths
        // worth of samples.
        let sr = 48_000.0;
        let mut held_energy = 0.0_f32;
        for _ in 0..6_000 {
            let s = v.process(sr, &p);
            held_energy += s * s;
        }
        assert!(
            held_energy > 0.05,
            "LoopSustain should hold the loop while gate is true (energy {held_energy})"
        );

        // Release the gate; the loop is now released and the slot
        // plays through to sample end then enters Release silence.
        // After ADSR release time + ~10 ms of buffer drain the slot
        // should be inactive.
        v.gate_off();
        // Drain past sample-end + release tail (≥25 ms with knob=0.01).
        for _ in 0..24_000 {
            let _ = v.process(sr, &p);
        }
        let mut late_energy = 0.0_f32;
        for _ in 0..1_000 {
            let s = v.process(sr, &p);
            late_energy += s * s;
        }
        assert!(
            late_energy < 1e-3,
            "LoopSustain should release after gate_off (late energy {late_energy})"
        );
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

    /// NoLoop region: no loop override emitted, so the slot drains
    /// to sample end and enters Release per the existing single-shot
    /// path.  Sanity check that we didn't accidentally loop everything.
    #[test]
    fn region_no_loop_drains_to_silence() {
        let mut v = SampleInstrumentVoice::new();
        let mut r = SfzRegion {
            lokey: 60,
            hikey: 60,
            pitch_keycenter: 60,
            loop_mode: crate::state::sfz::SfzLoopMode::NoLoop,
            loop_start: Some(64),
            loop_end: Some(192),
            ..Default::default()
        };
        r.sample_path = std::path::PathBuf::from("/noloop.wav");
        let buf: Vec<f32> = vec![0.4; 256];
        v.load_sfz(vec![SfzRegionRuntime {
            region: r,
            samples: Arc::new(buf),
        }]);

        let mut p = make_params();
        // Explicitly disable global loop so the only thing that could
        // hold the slot alive past the buffer is the override — which
        // must NOT fire for a NoLoop region.
        p.sample_loop_enabled = false;
        p.sample_volume = 1.0;
        p.sample_release = 0.01;

        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            1.0,
            0.0,
            0.0,
            0.0,
        );
        let sr = 48_000.0;
        for _ in 0..20_000 {
            let _ = v.process(sr, &p);
        }
        let mut late_energy = 0.0_f32;
        for _ in 0..2_000 {
            let s = v.process(sr, &p);
            late_energy += s * s;
        }
        assert!(
            late_energy < 1e-3,
            "NoLoop region must not loop (late energy {late_energy})"
        );
    }
}
