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
}
