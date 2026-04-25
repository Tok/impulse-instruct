// ─── tests/transport_helpers_tests.rs ─────────────────────────────────────────
// Pure helpers in `sequencer/mod.rs` that the audio thread used to
// inline.  Lifting them out + testing them locks down two regression-
// prone behaviours: the polymeter-aware step-count delta and the
// gate-off classification used by the chain advance "stop at end" path.

#[cfg(test)]
mod step_count_delta {
    use crate::sequencer::step_count_delta;

    #[test]
    fn no_advance_returns_zero() {
        // Same step → no contribution to global_step_count.
        assert_eq!(step_count_delta(42, 42), 0);
    }

    #[test]
    fn straight_advance_is_difference() {
        assert_eq!(step_count_delta(10, 13), 3);
    }

    #[test]
    fn saturating_when_clock_appears_to_run_backwards() {
        // Defensive: a session reload can land with the saved
        // sequencer.current_step ahead of the freshly-zeroed audio
        // clock.  The delta must clamp at 0 instead of underflowing
        // into a huge u64.
        assert_eq!(step_count_delta(100, 0), 0);
        assert_eq!(step_count_delta(usize::MAX, 0), 0);
    }

    #[test]
    fn does_not_wrap_at_max_steps() {
        // Polymeter fix: previously wrapped at MAX_STEPS=64 silently
        // dropping one slot per wrap.  The delta math is now plain
        // saturating subtraction over the absolute step counter.
        let delta = step_count_delta(63, 70);
        assert_eq!(delta, 7, "no wrap-fallback should kick in at 64");
    }

    #[test]
    fn large_block_advance_returns_full_delta() {
        // High BPM × big engine block can cross many steps in one
        // call.  The delta should sum cleanly.
        let delta = step_count_delta(1_000, 1_032);
        assert_eq!(delta, 32);
    }
}

#[cfg(test)]
mod midi_clock_interval {
    use crate::midi::midi_clock_tick_interval_samples;

    #[test]
    fn one_twenty_bpm_at_48k_is_one_thousand_samples() {
        // 60s / (120 * 24) ≈ 20.833 ms; at 48 kHz that's exactly 1000
        // samples per tick.  Locks the reference value so a future
        // refactor can't quietly change PPQN or the formula.
        let s = midi_clock_tick_interval_samples(120.0, 48_000.0);
        assert!((s - 1000.0).abs() < 1e-6, "got {s}");
    }

    #[test]
    fn faster_bpm_means_fewer_samples_per_tick() {
        let slow = midi_clock_tick_interval_samples(60.0, 48_000.0);
        let fast = midi_clock_tick_interval_samples(180.0, 48_000.0);
        assert!(fast < slow);
        assert!(
            (slow - 3.0 * fast).abs() < 1e-3,
            "tripling BPM should third the interval"
        );
    }

    #[test]
    fn forty_four_one_k_at_120_bpm_matches_classic_value() {
        // 44100 * 60 / (120 * 24) = 918.75 samples per tick.
        let s = midi_clock_tick_interval_samples(120.0, 44_100.0);
        assert!((s - 918.75).abs() < 1e-3, "got {s}");
    }

    #[test]
    fn zero_bpm_clamps_to_finite_interval() {
        // Defensive: a corrupt save shouldn't spin the audio thread on
        // an Inf accumulator.  Helper floors BPM at 1.0.
        let s = midi_clock_tick_interval_samples(0.0, 48_000.0);
        assert!(s.is_finite() && s > 0.0, "got {s}");
    }

    #[test]
    fn negative_bpm_is_treated_as_minimum() {
        let s = midi_clock_tick_interval_samples(-10.0, 48_000.0);
        assert!(s.is_finite() && s > 0.0);
    }
}

#[cfg(test)]
mod is_gate_off {
    use crate::sequencer::TriggerEvent;
    use crate::state::DrumVoice;

    #[test]
    fn note_starts_are_not_gate_offs() {
        let bass = TriggerEvent::BassTrigger {
            voice_idx: 0,
            note: 36,
            accent: 0.0,
            slide: 0.0,
            gate_samples: 22050,
            pan: 0.0,
        };
        assert!(!bass.is_gate_off());
        let drum = TriggerEvent::DrumTrigger {
            voice: DrumVoice::Kick808,
            velocity: 1.0,
            slice: 0,
        };
        assert!(!drum.is_gate_off());
        let pluck = TriggerEvent::PluckTrigger {
            note: 60,
            accent: 0.0,
            slide: 0.0,
        };
        assert!(!pluck.is_gate_off());
    }

    #[test]
    fn every_voice_gate_off_classifies() {
        // Lock the contract: when adding a new voice with a GateOff
        // variant, this test trips immediately if it's not in the
        // matcher.
        let cases = [
            TriggerEvent::BassGateOff { voice_idx: 0 },
            TriggerEvent::HooverGateOff,
            TriggerEvent::An1xGateOff,
            TriggerEvent::PluckGateOff,
            TriggerEvent::WavetableGateOff,
        ];
        for evt in &cases {
            assert!(evt.is_gate_off(), "{evt:?} should classify as gate-off");
        }
    }

    #[test]
    fn drum_triggers_are_not_gate_offs_even_when_velocity_zero() {
        // Drums don't have a gate-off concept — a velocity-0 drum
        // trigger is still a "trigger" (one-shot voice), and
        // `is_gate_off` must NOT mistake it for a release event.
        let drum = TriggerEvent::DrumTrigger {
            voice: DrumVoice::Snare808,
            velocity: 0.0,
            slice: 0,
        };
        assert!(!drum.is_gate_off());
    }
}
