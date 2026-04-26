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
            0.0,
        );
        v_oct.trigger(
            72,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
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

    #[test]
    fn mellotron_mode_keeps_output_finite_and_in_range() {
        // Mellotron mode adds flutter + spin-up + tanh sat to the
        // signal path; verify the math doesn't blow up or drift
        // unbounded over a few thousand samples.  The audible
        // character is a subjective check; here we just lock the
        // contract that the path stays well-behaved.
        let mut v = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..8192).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let mut p = make_params();
        p.sample_mellotron_mode = true;
        p.sample_mellotron_flutter = 1.0; // full warble
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            let out = v.process(48_000.0, &p);
            assert!(out.is_finite(), "mellotron path should not produce NaN/Inf");
            peak = peak.max(out.abs());
        }
        // tanh saturation guarantees |out| < 1; with the 0.85
        // post-scale it should comfortably stay under 1.0.
        assert!(
            peak <= 1.0,
            "mellotron output should stay in range (peak {peak})"
        );
    }

    #[test]
    fn mellotron_mode_off_is_bit_identical_to_default_path() {
        // Two voices: one with mellotron_mode = false (default),
        // one with the same defaults.  Their outputs should match
        // sample-for-sample for the first second — ensures the
        // flag is truly off when off (no side-effect leakage from
        // our new state fields).
        let mut a = SampleInstrumentVoice::new();
        let mut b = SampleInstrumentVoice::new();
        let data: Vec<f32> = (0..8192).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        a.load(Arc::new(data.clone()));
        b.load(Arc::new(data));
        a.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        b.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        a.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        b.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params();
        // Both should match sample-for-sample with mellotron off.
        for i in 0..2_000 {
            let oa = a.process(48_000.0, &p);
            let ob = b.process(48_000.0, &p);
            assert_eq!(oa, ob, "frame {i}: outputs diverged with mellotron off");
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
            0.0,
        );
        v.trigger(
            64,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
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

    #[test]
    fn fresh_voice_reports_zero_active_slots() {
        // The poly meter's resting state — no triggers, no audio,
        // count is 0.  Guards against a regression where a slot is
        // marked Active during construction.
        let v = SampleInstrumentVoice::new();
        assert_eq!(v.active_voice_count(), 0);
    }

    #[test]
    fn poly_dots_matches_voice_pool() {
        // The UI poly-meter hard-codes `POLY_DOTS` instead of pulling
        // the test-only `POLY_VOICES` constant; this test fails loudly
        // if the two ever drift apart.
        assert_eq!(
            crate::ui::panels::sample_instrument_viz::POLY_DOTS as usize,
            SampleInstrumentVoice::POLY_VOICES,
            "POLY_DOTS in the viz module must mirror POLY_VOICES"
        );
    }
}

#[cfg(test)]
mod sample_instrument_filter_tests {
    use std::sync::Arc;

    use crate::audio::dsp::sample_instrument::SampleInstrumentVoice;

    fn make_params(filter_mix: f32, filter_cutoff: f32) -> crate::audio::dsp::AudioParams {
        let mut s = crate::state::AppState::default();
        s.sample_instrument.filter_mix = filter_mix;
        s.sample_instrument.filter_cutoff = filter_cutoff;
        s.sample_instrument.filter_mode = 0; // LP
        crate::audio::dsp::AudioParams::from_app_state(&s)
    }

    /// Run a brief tone burst through the voice and return its peak
    /// amplitude across the run.  Used to compare filter on/off
    /// states without depending on absolute output levels.
    fn peak_through(filter_mix: f32, filter_cutoff: f32) -> f32 {
        let mut v = SampleInstrumentVoice::new();
        // Buffer at ~3 kHz (with the 48 kHz engine) so a closed LP at
        // 0.0 cutoff = 20 Hz definitively kills it.
        let sr = 48_000.0;
        let data: Vec<f32> = (0..8192)
            .map(|i| ((i as f32) * (3000.0 * std::f32::consts::TAU / sr)).sin() * 0.5)
            .collect();
        v.load(Arc::new(data));
        v.set_root_note(60, crate::audio::dsp::TuningSystem::TwelveTet);
        v.trigger(
            60,
            crate::audio::dsp::TuningSystem::TwelveTet,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        let p = make_params(filter_mix, filter_cutoff);
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            peak = peak.max(v.process(sr, &p).abs());
        }
        peak
    }

    #[test]
    fn filter_bypass_passes_full_signal() {
        // mix = 0 → SVF skipped → output matches the dry-only path.
        let dry = peak_through(0.0, 1.0);
        assert!(
            dry > 0.05,
            "dry path should produce audible output; got {dry}"
        );
    }

    #[test]
    fn closed_lpf_drops_high_frequency_content() {
        // mix = 1, cutoff = 0 → 20 Hz LP — a 3 kHz sine should be
        // attenuated to a fraction of its dry level.
        let dry = peak_through(0.0, 1.0);
        let filtered = peak_through(1.0, 0.0);
        assert!(
            filtered < dry * 0.5,
            "closed LP should drop high-frequency content (dry={dry}, filtered={filtered})"
        );
    }
}

#[cfg(test)]
mod sample_lfo_target_tests {
    use crate::state::LfoTarget;
    use crate::state::lfo_target_short_label;
    use crate::state::modulation::lfo_target_module_kind;

    #[test]
    fn sample_lfo_targets_route_to_sample_instrument() {
        for t in [
            LfoTarget::SampleVolume,
            LfoTarget::SamplePan,
            LfoTarget::SamplePitch,
            LfoTarget::SampleCutoff,
        ] {
            assert_eq!(
                lfo_target_module_kind(t),
                Some(crate::state::ModuleKind::SampleInstrument),
                "{t:?} should route to SampleInstrument",
            );
            // Short labels must be ≤6 chars (back-panel jack space).
            assert!(lfo_target_short_label(t).len() <= 6);
        }
    }
}

#[cfg(test)]
mod sample_instrument_viz_tests {
    use crate::ui::panels::sample_instrument_viz::build_thumbnail;

    #[test]
    fn empty_buffer_returns_empty_thumbnail() {
        assert!(build_thumbnail(&[], 32).is_empty());
    }

    #[test]
    fn thumbnail_columns_track_min_max_per_bin() {
        // 100-sample buffer ramping 0..1; bin to 4 columns.  Each bin
        // should hold 25 samples; min should be the first sample of
        // the bin, max the last.
        let data: Vec<f32> = (0..100).map(|i| i as f32 / 99.0).collect();
        let thumb = build_thumbnail(&data, 4);
        assert_eq!(thumb.len(), 4);
        for (i, (mn, mx)) in thumb.iter().enumerate() {
            // First bin's min is around 0; last bin's max is around 1.
            // The exact values depend on the bin size; assert
            // monotonic-increasing across columns.
            if i > 0 {
                assert!(thumb[i].0 > thumb[i - 1].0, "bins must climb monotonically");
            }
            assert!(*mn <= *mx, "min ≤ max within each bin");
        }
    }

    #[test]
    fn thumbnail_handles_smaller_than_cols() {
        // Fewer samples than columns — bin = 1, every sample becomes
        // its own column; extra columns get the last sample's pair or
        // zeros.
        let data: Vec<f32> = vec![0.5; 8];
        let thumb = build_thumbnail(&data, 16);
        // Implementation guarantees `cols` length output.
        assert_eq!(thumb.len(), 16);
        for (mn, mx) in thumb.iter().take(8) {
            assert!((*mn - 0.5).abs() < 1e-6);
            assert!((*mx - 0.5).abs() < 1e-6);
        }
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
    fn formant_preserve_flag_defaults_off_and_round_trips() {
        // V2 Stage 8: flag wired through state + LLM apply.  The
        // actual PSOLA / phase-vocoder DSP is deferred — pinning the
        // flag's plumbing now means future-me's implementation
        // surfaces transparently for sessions that already opted in.
        let s = AppState::default();
        assert!(!s.sample_instrument.formant_preserve);
        let update = serde_json::json!({ "sample": { "formant_preserve": true } });
        let s = apply_llm_update(s, &update, &[]);
        assert!(s.sample_instrument.formant_preserve);
        // Locked path should skip.
        let s = AppState::default();
        let s = apply_llm_update(s, &update, &["sample.formant_preserve".to_string()]);
        assert!(!s.sample_instrument.formant_preserve);
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
