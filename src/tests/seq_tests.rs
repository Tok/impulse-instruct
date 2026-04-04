#[cfg(test)]
mod sequencer_tests {
    use crate::sequencer::{ClockState, TriggerEvent, advance_clock, samples_per_step};
    use crate::state::{DrumVoice, MAX_STEPS, SequencerState, Step};

    #[test]
    fn samples_per_step_at_120bpm_44100hz() {
        // 120 BPM → 2 beats/s → 8 16th-notes/s → 5512.5 samples/step
        let sps = samples_per_step(120.0, 44100.0);
        let expected = 44100.0 * 60.0 / (120.0 * 4.0);
        assert!((sps - expected).abs() < 0.01, "got {}", sps);
    }

    #[test]
    fn advance_clock_does_not_tick_when_stopped() {
        let seq = SequencerState {
            running: false,
            ..SequencerState::default()
        };
        let clock = ClockState::default();
        let (new_clock, events) = advance_clock(clock, &seq, 512, 44100.0);
        assert!(events.is_empty());
        assert_eq!(new_clock.current_step, 0);
    }

    #[test]
    fn advance_clock_wraps_at_max_steps() {
        // current_step is a global tick counter that wraps at MAX_STEPS (64).
        // Per-voice lengths are applied as modulo at trigger time.
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState {
            current_step: MAX_STEPS - 1,
            ..ClockState::default()
        };

        let (new_clock, _) = advance_clock(clock, &seq, sps + 1, 44100.0);
        assert_eq!(
            new_clock.current_step, 0,
            "should wrap from MAX_STEPS-1 to 0"
        );
    }

    #[test]
    fn advance_clock_fires_active_steps() {
        use crate::sequencer::TriggerEvent;

        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        // Activate step 1 of kick 808
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 1,
        };

        let sps = samples_per_step(120.0, 44100.0) as usize;
        let clock = ClockState::default();

        let (_, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
        let has_kick = events.iter().any(|e| {
            matches!(
                e,
                TriggerEvent::DrumTrigger {
                    voice: DrumVoice::Kick808,
                    ..
                }
            )
        });
        assert!(has_kick, "expected kick trigger, got {:?}", events);
    }

    #[test]
    fn ratchet_2_emits_sub_hit_after_step_fires() {
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 2,
        };

        let sps = samples_per_step(120.0, 44100.0);
        // First block fires step 0 (ratchet=2 → first hit + schedule 1 sub-hit)
        let clock = ClockState::default();
        let (clock2, events1) = advance_clock(clock, &seq, sps as usize + 1, 44100.0);
        let first_kick = events1
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    TriggerEvent::DrumTrigger {
                        voice: DrumVoice::Kick808,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(first_kick, 1, "first block should fire one kick");
        assert!(
            clock2.ratchet_remaining[0] > 0,
            "sub-hit should be pending after ratchet=2"
        );

        // Second block advances past the half-step interval → sub-hit fires
        let (_, events2) = advance_clock(clock2, &seq, sps as usize / 2 + 1, 44100.0);
        let sub_kick = events2
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    TriggerEvent::DrumTrigger {
                        voice: DrumVoice::Kick808,
                        ..
                    }
                )
            })
            .count();
        assert!(sub_kick >= 1, "ratchet sub-hit should fire in second block");
    }
}

#[cfg(test)]
mod euclidean_tests {
    use crate::sequencer::euclidean_rhythm;

    #[test]
    fn euclid_pulse_count_matches() {
        for (pulses, steps) in [(4, 16), (5, 8), (3, 7), (1, 4), (7, 7), (0, 8)] {
            let r = euclidean_rhythm(pulses, steps);
            assert_eq!(r.len(), steps, "len mismatch {}/{}", pulses, steps);
            let count = r.iter().filter(|&&x| x).count();
            assert_eq!(count, pulses, "pulse count mismatch {}/{}", pulses, steps);
        }
    }

    #[test]
    fn euclid_edge_cases() {
        assert_eq!(euclidean_rhythm(0, 8), vec![false; 8]);
        assert_eq!(euclidean_rhythm(8, 8), vec![true; 8]);
        assert!(euclidean_rhythm(0, 0).is_empty());
    }

    #[test]
    fn euclid_4_in_16_is_four_on_floor() {
        // Classic: 4-on-the-floor places pulses at indices 0, 4, 8, 12.
        let r = euclidean_rhythm(4, 16);
        assert!(
            r[0] && r[4] && r[8] && r[12],
            "4-on-floor placement wrong: {:?}",
            r
        );
    }
}

#[cfg(test)]
mod step_array_tests {
    use crate::state::{AppState, DrumVoice, apply_llm_update};

    fn kick_active(state: &AppState) -> Vec<usize> {
        state.sequencer.drum_patterns[&DrumVoice::Kick808]
            .iter()
            .enumerate()
            .filter(|(_, s)| s.active)
            .map(|(i, _)| i)
            .collect()
    }

    fn bass_active(state: &AppState) -> Vec<usize> {
        state
            .sequencer
            .bass_pattern
            .iter()
            .enumerate()
            .filter(|(_, s)| s.active)
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn index_list_activates_only_listed_steps() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "kick_a_steps": [0, 4, 8, 12] }
        });
        let state = apply_llm_update(state, &update);
        assert_eq!(kick_active(&state), vec![0, 4, 8, 12]);
    }

    #[test]
    fn index_list_clears_existing_active_steps() {
        // Default kick pattern has 0,4,8,12. Send [2,6] — should clear old and set new.
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "kick_a_steps": [2, 6] }
        });
        let state = apply_llm_update(state, &update);
        assert_eq!(kick_active(&state), vec![2, 6]);
    }

    #[test]
    fn empty_array_clears_all_steps() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "kick_a_steps": [] }
        });
        let state = apply_llm_update(state, &update);
        assert!(
            kick_active(&state).is_empty(),
            "empty array should clear all kick steps"
        );
    }

    #[test]
    fn integer_zero_one_inline_works_as_bool() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "kick_a_steps": [1,0,0,0,1,0,0,0,1,0,0,0,1,0,0,0] }
        });
        let state = apply_llm_update(state, &update);
        assert_eq!(kick_active(&state), vec![0, 4, 8, 12]);
    }

    #[test]
    fn bass_step_index_list() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "bass_steps": [0, 3, 7, 11] }
        });
        let state = apply_llm_update(state, &update);
        assert_eq!(bass_active(&state), vec![0, 3, 7, 11]);
    }

    #[test]
    fn bass_step_empty_clears() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "bass_steps": [] }
        });
        let state = apply_llm_update(state, &update);
        assert!(bass_active(&state).is_empty());
    }
}

#[cfg(test)]
mod probability_tests {
    use crate::sequencer::{ClockState, TriggerEvent, advance_clock, samples_per_step};
    use crate::state::{DrumVoice, SequencerState, Step};

    /// Build a sequencer with only step 0 of Kick808 active, all other kick steps cleared.
    /// The 16-step pattern cycles 4× per global 64-step loop, so step 0 visits 4 times per loop.
    fn seq_with_single_kick(prob: f32) -> SequencerState {
        let mut seq = SequencerState::default();
        seq.running = true;
        seq.bpm = 120.0;
        // Clear all kick808 steps, then set only step 0 with the target probability
        if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Kick808) {
            for s in pattern.iter_mut() {
                *s = Step {
                    active: false,
                    velocity: 1.0,
                    probability: 1.0,
                    ratchet: 1,
                };
            }
            pattern[0] = Step {
                active: true,
                velocity: 1.0,
                probability: prob,
                ratchet: 1,
            };
        }
        seq
    }

    fn kick808_count_over_loops(prob: f32, loops: u32) -> usize {
        let sr = 44100.0_f32;
        let sps = samples_per_step(120.0, sr) as usize;
        let seq = seq_with_single_kick(prob);
        let mut clock = ClockState::default();
        let mut count = 0usize;
        loop {
            let (new_clock, events) = advance_clock(clock.clone(), &seq, sps + 1, sr);
            clock = new_clock;
            count += events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        TriggerEvent::DrumTrigger {
                            voice: DrumVoice::Kick808,
                            ..
                        }
                    )
                })
                .count();
            if clock.loop_count >= loops {
                break;
            }
        }
        count
    }

    #[test]
    fn probability_zero_never_fires() {
        // prob=0.0 — kick should never fire
        let count = kick808_count_over_loops(0.0, 20);
        assert_eq!(count, 0, "prob=0.0 fired {} times", count);
    }

    #[test]
    fn probability_one_fires_every_visit() {
        // prob=1.0 — step 0 of a 16-step pattern in a 64-step global loop fires 4×/loop
        let loops: u32 = 20;
        let count = kick808_count_over_loops(1.0, loops);
        // 16-step pattern, 64 global steps per loop → 4 visits to step 0 per loop
        let expected = loops as usize * 4;
        assert_eq!(
            count, expected,
            "prob=1.0 over {} loops: expected {}, got {}",
            loops, expected, count
        );
    }

    #[test]
    fn probability_half_fires_roughly_half_the_visits() {
        // Deterministic hash — prob=0.5 over 100 loops × 4 visits = 400 opportunities
        // Expect roughly 200 ± wide margin (150–250)
        let count = kick808_count_over_loops(0.5, 100);
        assert!(
            count >= 100 && count <= 300,
            "prob=0.5 over 100 loops: expected ~200, got {}",
            count
        );
    }
}
