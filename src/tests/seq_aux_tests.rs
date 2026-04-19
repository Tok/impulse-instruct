// ─── tests/seq_aux_tests.rs ─────────────────────────────────────────────────
// Secondary sequencer test groups — euclidean rhythm, LLM step-array
// decoding, probability sampling, pattern temperature.  Split out of
// seq_tests.rs to keep both files under the 1000-line hook limit.

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
        let state = apply_llm_update(state, &update, &[]);
        assert_eq!(kick_active(&state), vec![0, 4, 8, 12]);
    }

    #[test]
    fn index_list_clears_existing_active_steps() {
        // Default kick pattern has 0,4,8,12. Send [2,6] — should clear old and set new.
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "kick_a_steps": [2, 6] }
        });
        let state = apply_llm_update(state, &update, &[]);
        assert_eq!(kick_active(&state), vec![2, 6]);
    }

    #[test]
    fn empty_array_clears_all_steps() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "kick_a_steps": [] }
        });
        let state = apply_llm_update(state, &update, &[]);
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
        let state = apply_llm_update(state, &update, &[]);
        assert_eq!(kick_active(&state), vec![0, 4, 8, 12]);
    }

    #[test]
    fn bass_step_index_list() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "bass_steps": [0, 3, 7, 11] }
        });
        let state = apply_llm_update(state, &update, &[]);
        assert_eq!(bass_active(&state), vec![0, 3, 7, 11]);
    }

    #[test]
    fn bass_step_empty_clears() {
        let state = AppState::default();
        let update = serde_json::json!({
            "sequencer": { "bass_steps": [] }
        });
        let state = apply_llm_update(state, &update, &[]);
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
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        // Clear all kick808 steps, then set only step 0 with the target probability
        if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Kick808) {
            for s in pattern.iter_mut() {
                *s = Step {
                    active: false,
                    velocity: 1.0,
                    probability: 1.0,
                    ratchet: 1,
                    slice: 0,
                };
            }
            pattern[0] = Step {
                active: true,
                velocity: 1.0,
                probability: prob,
                ratchet: 1,
                slice: 0,
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
        // prob=1.0 — step 0 of a 32-step pattern in a 64-step global loop fires 2×/loop
        let loops: u32 = 20;
        let count = kick808_count_over_loops(1.0, loops);
        // 32-step pattern, 64 global steps per loop → 2 visits to step 0 per loop
        let expected = loops as usize * 2;
        assert_eq!(
            count, expected,
            "prob=1.0 over {} loops: expected {}, got {}",
            loops, expected, count
        );
    }

    #[test]
    fn probability_half_fires_roughly_half_the_visits() {
        // Deterministic hash — prob=0.5 over 100 loops × 2 visits = 200 opportunities
        // Expect roughly 100 ± wide margin (50–150)
        let count = kick808_count_over_loops(0.5, 100);
        assert!(
            (50..=150).contains(&count),
            "prob=0.5 over 100 loops: expected ~100, got {}",
            count
        );
    }
}

#[cfg(test)]
mod pattern_temperature_tests {
    use crate::state::TB303Step;
    use crate::state::sequencer_state::pattern_temperature_acc;

    // Match crate::ui::theme::NOTE_TEMP — kept local so the test stays in
    // the state crate without an UI dep.
    const HUTH: [f32; 12] = [
        -1.00, -0.87, -0.50, 0.00, 0.50, 1.00, 0.87, 0.34, -0.17, -0.64, -0.91, -1.00,
    ];

    fn step(note: u8, gate: f32, accent: bool) -> TB303Step {
        TB303Step {
            active: true,
            note,
            accent: if accent { 1.0 } else { 0.0 },
            slide: 0.0,
            gate,
            pan: 0.0,
        }
    }

    #[test]
    fn empty_pattern_zero_weight() {
        let (s, w) = pattern_temperature_acc(&[], 0, &HUTH);
        assert_eq!(s, 0.0);
        assert_eq!(w, 0.0);
    }

    #[test]
    fn inactive_steps_skipped() {
        let mut steps = vec![TB303Step::default(); 4];
        // All inactive — no contribution.
        let (s, w) = pattern_temperature_acc(&steps, 4, &HUTH);
        assert_eq!(w, 0.0);
        assert_eq!(s, 0.0);
        // Activate one F note.
        steps[0] = step(65, 1.0, false);
        let (s, w) = pattern_temperature_acc(&steps, 4, &HUTH);
        assert!((w - 1.0).abs() < 1e-6);
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn accent_boosts_weight() {
        let plain = vec![step(65, 1.0, false)];
        let accented = vec![step(65, 1.0, true)];
        let (_, wp) = pattern_temperature_acc(&plain, 1, &HUTH);
        let (_, wa) = pattern_temperature_acc(&accented, 1, &HUTH);
        assert!(wa > wp, "accent should weigh more: {wa} vs {wp}");
        // Boost factor is 1.5.
        assert!((wa - 1.5 * wp).abs() < 1e-6);
    }

    #[test]
    fn c_pattern_reads_cold_f_pattern_reads_warm() {
        let cs: Vec<_> = (0..8).map(|_| step(60, 1.0, false)).collect();
        let fs: Vec<_> = (0..8).map(|_| step(65, 1.0, false)).collect();
        let (sc, wc) = pattern_temperature_acc(&cs, 8, &HUTH);
        let (sf, wf) = pattern_temperature_acc(&fs, 8, &HUTH);
        assert!((sc / wc - (-1.0)).abs() < 1e-6);
        assert!((sf / wf - 1.0).abs() < 1e-6);
    }

    #[test]
    fn len_truncates() {
        let steps = vec![step(60, 1.0, false), step(65, 1.0, false)];
        // len=1 → only the cold C counts.
        let (s, w) = pattern_temperature_acc(&steps, 1, &HUTH);
        assert!((s / w - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn zero_gate_skipped() {
        let steps = vec![step(60, 0.0, false), step(65, 1.0, false)];
        let (s, w) = pattern_temperature_acc(&steps, 2, &HUTH);
        // Only F (warm) contributes.
        assert!((w - 1.0).abs() < 1e-6);
        assert!((s - 1.0).abs() < 1e-6);
    }

    #[test]
    fn note_octaves_wrap() {
        // C0, C5, C9 should all read as cold C.
        let steps = vec![
            step(0, 1.0, false),
            step(60, 1.0, false),
            step(108, 1.0, false),
        ];
        let (s, w) = pattern_temperature_acc(&steps, 3, &HUTH);
        assert!((s / w - (-1.0)).abs() < 1e-6);
    }
}
