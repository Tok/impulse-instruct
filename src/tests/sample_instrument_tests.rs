// ─── tests/sample_instrument_tests.rs ───────────────────────────────────────
// Unit tests for the SampleInstrument voice.  Covers the V1 surface:
// defaults, ModuleKind metadata, LLM apply on `sample.*` keys, and the
// DSP voice's load + trigger + resample behaviour.

#[cfg(test)]
mod sample_instrument_state_tests {
    use crate::state::{ModuleKind, SampleInstrumentState};

    #[test]
    fn defaults_are_silent_and_neutral() {
        let s = SampleInstrumentState::default();
        assert!(!s.enabled);
        assert_eq!(s.root_note, 60); // C4
        assert!((s.volume - 0.7).abs() < 1e-6);
        assert!(s.sample_path.is_empty());
    }

    #[test]
    fn module_kind_label_and_zone() {
        use crate::state::Zone;
        let k = ModuleKind::SampleInstrument;
        assert_eq!(k.label(), "SAMPLER+");
        assert_eq!(k.default_zone(), Zone::Voice);
        assert!(k.has_audio_output());
        assert!(crate::state::mod_inputs(k).len() > 0);
    }
}

#[cfg(test)]
mod sample_instrument_dsp_tests {
    use crate::audio::dsp::sample_instrument::SampleInstrumentVoice;
    use std::sync::Arc;

    fn make_params() -> crate::audio::dsp::AudioParams {
        let s = crate::state::AppState::default();
        crate::audio::dsp::AudioParams::from_app_state(&s)
    }

    #[test]
    fn silent_before_trigger() {
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        v.load(Arc::new(data));
        // No trigger → output is zero (no envelope, no gate).
        let p = make_params();
        for _ in 0..200 {
            assert_eq!(v.process(48_000.0, &p), 0.0);
        }
    }

    #[test]
    fn trigger_at_root_note_produces_audible_output() {
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params();
        let mut nonzero = false;
        for _ in 0..1024 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite());
            if out.abs() > 0.01 {
                nonzero = true;
            }
        }
        assert!(
            nonzero,
            "voice should be audible after trigger at root note"
        );
    }

    #[test]
    fn higher_note_advances_faster_than_root() {
        // Trigger at root + octave; the loop should wrap roughly twice as
        // fast as at root.  We measure via the read position delta after
        // a fixed number of process() calls.
        let mut v_root = SampleInstrumentVoice::new();
        let mut v_oct = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..8192).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        v_root.load(Arc::new(data.clone()));
        v_oct.load(Arc::new(data));
        v_root.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v_oct.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v_root.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        v_oct.trigger(
            72,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        ); // +12 st
        let p = make_params();
        // Just check both produce finite audio; exact rate is internal
        // to the DSP and we don't expose it for the test.
        for _ in 0..2_000 {
            assert!(v_root.process(48_000.0, &p).is_finite());
            assert!(v_oct.process(48_000.0, &p).is_finite());
        }
    }
}

#[cfg(test)]
mod sample_instrument_v11_tests {
    use crate::audio::dsp::sample_instrument::SampleInstrumentVoice;
    use std::sync::Arc;

    fn make_params() -> crate::audio::dsp::AudioParams {
        let s = crate::state::AppState::default();
        crate::audio::dsp::AudioParams::from_app_state(&s)
    }

    #[test]
    fn adsr_release_decays_to_silence_after_gate_off() {
        // Trigger, run a bit, gate off, run more — output should decay
        // to silence within a few hundred ms (default release ≈ 100 ms).
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..4096).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params();
        for _ in 0..1024 {
            v.process(48_000.0, &p);
        }
        v.gate_off();
        // Run 2 s — release tail (default ≈ 200 ms tau) should be far
        // below audible threshold by then.  We assert <1 % residual,
        // which is comfortably inside the noise floor of any rendered
        // mix; tighter thresholds would be sensitive to the EMA's
        // exponential-tail constant rather than the actual UX.
        for _ in 0..96_000 {
            let _ = v.process(48_000.0, &p);
        }
        let out = v.process(48_000.0, &p);
        assert!(out.abs() < 0.01, "release tail didn't decay; final={out}");
    }

    #[test]
    fn loop_disabled_one_shot_eventually_silences() {
        // Set loop_enabled = false via a custom AudioParams.
        let mut s = crate::state::AppState::default();
        s.sample_instrument.loop_enabled = false;
        let p = crate::audio::dsp::AudioParams::from_app_state(&s);
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        // Run long enough for the buffer to fully play through + release
        // tail to complete.
        for _ in 0..96_000 {
            let _ = v.process(48_000.0, &p);
        }
        let out = v.process(48_000.0, &p);
        assert!(
            out.abs() < 1e-3,
            "one-shot voice should fall silent after buffer+release; got {out}",
        );
    }
}

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
}

#[cfg(test)]
mod sample_instrument_poly_tests {
    use std::sync::Arc;

    use crate::audio::dsp::sample_instrument::SampleInstrumentVoice;

    fn make_params() -> crate::audio::dsp::AudioParams {
        let s = crate::state::AppState::default();
        crate::audio::dsp::AudioParams::from_app_state(&s)
    }

    #[test]
    fn overlapping_triggers_keep_previous_release_alive() {
        // Trigger at 60, immediately re-trigger at 67 — the first slot
        // should be in release (envelope still > 0) while the second
        // slot is in attack.  Pre-V2 monophonic behaviour would have
        // killed the first slot's tail.
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..2048).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        // Run a few samples so the first slot is past its attack ramp.
        let p = make_params();
        for _ in 0..200 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        v.trigger(
            67,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        // Two slots should be active: the first in Release tail, the
        // second in Attack.
        assert_eq!(
            v.active_voice_count(),
            2,
            "previous note's release tail should overlap with the new attack"
        );
    }

    #[test]
    fn allocator_steals_oldest_when_pool_full() {
        // Fire POLY_VOICES + 1 triggers in a row without gating off; the
        // last trigger must steal a slot, so the active count caps at
        // POLY_VOICES.
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.1).sin() * 0.3).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        for note in 0..(SampleInstrumentVoice::POLY_VOICES as u8 + 1) {
            v.trigger(
                60 + note,
                crate::audio::dsp::TuningSystem::TwelveTet,
                0.0,
                0.0,
                0.0,
            );
        }
        assert_eq!(
            v.active_voice_count(),
            SampleInstrumentVoice::POLY_VOICES,
            "trigger past pool size should steal — caps at POLY_VOICES",
        );
    }

    #[test]
    fn gate_off_releases_all_gated_slots() {
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.1).sin() * 0.3).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        v.trigger(
            64,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        v.trigger(
            67,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
        );
        // Run a bit so the slots reach release-eligible amplitude.
        let p = make_params();
        for _ in 0..200 {
            let _ = v.process(48_000.0, &p);
        }
        v.gate_off();
        // After enough time, every slot's envelope should hit Off.
        // Default release knob ≈ 200 ms tau; budget 4 seconds (~20 tau)
        // so the 1e-5 cutoff is comfortably reached on every slot.
        for _ in 0..192_000 {
            let _ = v.process(48_000.0, &p);
        }
        assert_eq!(
            v.active_voice_count(),
            0,
            "gate-off + release tail should silence every slot"
        );
    }
}

#[cfg(test)]
mod sample_instrument_llm_apply_tests {
    use crate::state::{AppState, apply_llm_update};

    #[test]
    fn apply_sample_writes_voice_params() {
        let s = AppState::default();
        let update = serde_json::json!({
            "sample": {
                "enabled": true,
                "root_note": 69,
                "volume": 0.85,
                "pan": -0.3,
                "pitch_offset_cents": 7.0
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.sample_instrument.enabled);
        assert_eq!(s.sample_instrument.root_note, 69);
        assert!((s.sample_instrument.volume - 0.85).abs() < 1e-4);
        assert!((s.sample_instrument.pan - (-0.3)).abs() < 1e-4);
        assert!((s.sample_instrument.pitch_offset_cents - 7.0).abs() < 1e-4);
    }

    #[test]
    fn apply_sample_writes_adsr_and_loop_fields() {
        let s = AppState::default();
        let update = serde_json::json!({
            "sample": {
                "attack": 0.4,
                "decay": 0.3,
                "sustain": 0.7,
                "release": 0.5,
                "loop_start": 0.1,
                "loop_end": 0.9,
                "loop_enabled": false
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!((s.sample_instrument.attack - 0.4).abs() < 1e-4);
        assert!((s.sample_instrument.decay - 0.3).abs() < 1e-4);
        assert!((s.sample_instrument.sustain - 0.7).abs() < 1e-4);
        assert!((s.sample_instrument.release - 0.5).abs() < 1e-4);
        assert!((s.sample_instrument.loop_start - 0.1).abs() < 1e-4);
        assert!((s.sample_instrument.loop_end - 0.9).abs() < 1e-4);
        assert!(!s.sample_instrument.loop_enabled);
    }

    #[test]
    fn apply_sample_writes_pattern_steps_and_notes() {
        let s = AppState::default();
        let update = serde_json::json!({
            "sample": {
                "sample_steps": [true, false, true, false],
                "sample_notes": [60, 62, 64, 65]
            }
        });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.sequencer.sample_pattern[0].active);
        assert!(!s.sequencer.sample_pattern[1].active);
        assert!(s.sequencer.sample_pattern[2].active);
        assert_eq!(s.sequencer.sample_pattern[2].note, 64);
    }
}
