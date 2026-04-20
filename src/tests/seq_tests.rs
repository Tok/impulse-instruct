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
        let seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };

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

        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        // Activate step 1 of kick 808
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 1,
            slice: 0,
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
    fn preecho_scales_velocity_through_advance_clock() {
        // Activate every step on kick808; install preecho with anchor=8,
        // length=4, velocity_ramp.  Steps 4..=7 are the lead-in window;
        // step 8 is the anchor (full velocity), other steps untouched.
        use crate::sequencer::PreechoConfig;
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        for s in seq
            .drum_patterns
            .get_mut(&DrumVoice::Kick808)
            .unwrap()
            .iter_mut()
        {
            *s = Step {
                active: true,
                velocity: 1.0,
                probability: 1.0,
                ratchet: 1,
                slice: 0,
            };
        }
        seq.preecho.insert(
            "kit_a".to_string(),
            PreechoConfig {
                enabled: true,
                anchors: vec![8],
                length: 4,
                auto_length: false,
                curve: Default::default(),
                velocity_ramp: true,
                ratchet_ramp: false,
                probability_ramp: false,
                accent_ramp: false,
                slide_cascade: false,
                note_approach: crate::sequencer::NoteApproach::Off,
            },
        );
        // Walk enough samples to fire all 16 steps.
        // Step one block at a time and tag each kick trigger with the
        // current_step the clock landed on (advance_clock fires triggers
        // when entering a new step).
        let sps = samples_per_step(120.0, 44100.0) as usize;
        let mut clock = ClockState::default();
        let mut velocities: std::collections::HashMap<usize, f32> = Default::default();
        for _ in 0..20 {
            let prev = clock.current_step;
            let (next, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
            if next.current_step != prev {
                let vstep = next.current_step % 16;
                for e in &events {
                    if let TriggerEvent::DrumTrigger {
                        voice: DrumVoice::Kick808,
                        velocity,
                        ..
                    } = e
                    {
                        velocities.entry(vstep).or_insert(*velocity);
                    }
                }
            }
            clock = next;
        }
        // Step 8 (anchor) should be at full velocity; the lead-in steps
        // 4..=7 should ramp upward; step 0 and step 12 (outside the
        // lead-in window) should be unscaled.
        let v = |s: usize| velocities.get(&s).copied().unwrap_or(0.0);
        assert!(
            (v(8) - 1.0).abs() < 1e-3,
            "anchor expected 1.0, got {}",
            v(8)
        );
        assert!(
            (v(0) - 1.0).abs() < 1e-3,
            "outside-window expected 1.0, got {}",
            v(0)
        );
        assert!(
            (v(12) - 1.0).abs() < 1e-3,
            "outside-window expected 1.0, got {}",
            v(12)
        );
        // Lead-in is monotonically increasing toward the anchor.
        assert!(
            v(4) < v(5) && v(5) < v(6) && v(6) < v(7),
            "expected ramp 4<5<6<7, got {} {} {} {}",
            v(4),
            v(5),
            v(6),
            v(7)
        );
        // Step 7 (d=1, pos=1.0) → vel = 1.0; step 4 (d=4, pos=0.25) → 0.475.
        assert!((v(7) - 1.0).abs() < 1e-3);
        assert!((v(4) - 0.475).abs() < 1e-3);
    }

    #[test]
    fn melodic_preecho_ramps_bass_accent_and_cascades_slide() {
        // Bass voice 0 fires on every step with zero stored accent + slide.
        // Install preecho at anchor=8, length=4, accent_ramp + slide_cascade.
        // Steps 4..=7 should receive ramped accent; step 7 should also get
        // slide=1.0 (d==1).  Steps 0 and 12 pass through unchanged.
        use crate::sequencer::PreechoConfig;
        use crate::state::TB303Step;
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        seq.bass_voice_enabled[0] = true;
        for s in seq.bass_pattern.iter_mut() {
            *s = TB303Step {
                active: true,
                note: 36,
                accent: 0.0,
                slide: 0.0,
                pan: 0.0,
                gate: 0.5,
            };
        }
        seq.preecho.insert(
            "bass".to_string(),
            PreechoConfig {
                enabled: true,
                anchors: vec![8],
                length: 4,
                auto_length: false,
                curve: Default::default(),
                velocity_ramp: false,
                ratchet_ramp: false,
                probability_ramp: false,
                accent_ramp: true,
                slide_cascade: true,
                note_approach: crate::sequencer::NoteApproach::Off,
            },
        );
        let sps = samples_per_step(120.0, 44100.0) as usize;
        let mut clock = ClockState::default();
        let mut accents: std::collections::HashMap<usize, f32> = Default::default();
        let mut slides: std::collections::HashMap<usize, f32> = Default::default();
        for _ in 0..20 {
            let prev = clock.current_step;
            let (next, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
            if next.current_step != prev {
                let vstep = next.current_step % 16;
                for e in &events {
                    if let TriggerEvent::BassTrigger {
                        voice_idx: 0,
                        accent,
                        slide,
                        ..
                    } = e
                    {
                        accents.entry(vstep).or_insert(*accent);
                        slides.entry(vstep).or_insert(*slide);
                    }
                }
            }
            clock = next;
        }
        let a = |s: usize| accents.get(&s).copied().unwrap_or(-1.0);
        let sl = |s: usize| slides.get(&s).copied().unwrap_or(-1.0);
        // Lead-in 4..=7: monotonic ramp, ending at 1.0 on d=1.
        assert!(
            a(4) < a(5) && a(5) < a(6) && a(6) < a(7),
            "expected accent ramp 4<5<6<7, got {} {} {} {}",
            a(4),
            a(5),
            a(6),
            a(7),
        );
        assert!((a(7) - 1.0).abs() < 1e-3, "expected a(7)=1.0, got {}", a(7));
        assert!(
            (a(4) - 0.475).abs() < 1e-3,
            "expected a(4)≈0.475, got {}",
            a(4),
        );
        // Anchor + outside-window keep the stored zero accent.
        assert!(
            (a(8) - 0.0).abs() < 1e-6,
            "anchor kept stored, got {}",
            a(8)
        );
        assert!(
            (a(0) - 0.0).abs() < 1e-6,
            "out-of-window kept, got {}",
            a(0)
        );
        assert!(
            (a(12) - 0.0).abs() < 1e-6,
            "out-of-window kept, got {}",
            a(12)
        );
        // Slide cascade fires only on the d=1 step (7); others keep 0.
        assert!(
            (sl(7) - 1.0).abs() < 1e-3,
            "slide cascade at 7, got {}",
            sl(7)
        );
        assert!(
            (sl(6) - 0.0).abs() < 1e-6,
            "non-cascade step, got {}",
            sl(6)
        );
        assert!(
            (sl(8) - 0.0).abs() < 1e-6,
            "anchor slide kept, got {}",
            sl(8)
        );
    }

    #[test]
    fn melodic_preecho_ramps_hoover_accent_and_cascades_slide() {
        // Mirror of the bass preecho test for the Hoover voice.  Every
        // hoover step fires with zero stored accent/slide; preecho at
        // anchor=8 length=4 should ramp accent on 4..=7 and set slide=1
        // on step 7 only.  Uses hoover_steps=16 so anchor 8 is inside.
        use crate::sequencer::PreechoConfig;
        use crate::state::TB303Step;
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        seq.hoover_steps = 16;
        for s in seq.hoover_pattern.iter_mut() {
            *s = TB303Step {
                active: true,
                note: 57,
                accent: 0.0,
                slide: 0.0,
                pan: 0.0,
                gate: 0.5,
            };
        }
        seq.preecho.insert(
            "hoover".to_string(),
            PreechoConfig {
                enabled: true,
                anchors: vec![8],
                length: 4,
                auto_length: false,
                curve: Default::default(),
                velocity_ramp: false,
                ratchet_ramp: false,
                probability_ramp: false,
                accent_ramp: true,
                slide_cascade: true,
                note_approach: crate::sequencer::NoteApproach::Off,
            },
        );
        let sps = samples_per_step(120.0, 44100.0) as usize;
        let mut clock = ClockState::default();
        let mut accents: std::collections::HashMap<usize, f32> = Default::default();
        let mut slides: std::collections::HashMap<usize, f32> = Default::default();
        for _ in 0..20 {
            let prev = clock.current_step;
            let (next, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
            if next.current_step != prev {
                let vstep = next.current_step % 16;
                for e in &events {
                    if let TriggerEvent::HooverTrigger { accent, slide, .. } = e {
                        accents.entry(vstep).or_insert(*accent);
                        slides.entry(vstep).or_insert(*slide);
                    }
                }
            }
            clock = next;
        }
        let a = |s: usize| accents.get(&s).copied().unwrap_or(-1.0);
        let sl = |s: usize| slides.get(&s).copied().unwrap_or(-1.0);
        assert!(
            a(4) < a(5) && a(5) < a(6) && a(6) < a(7),
            "expected hoover accent ramp 4<5<6<7, got {} {} {} {}",
            a(4),
            a(5),
            a(6),
            a(7),
        );
        assert!((a(7) - 1.0).abs() < 1e-3, "expected a(7)=1.0, got {}", a(7));
        assert!(
            (a(8) - 0.0).abs() < 1e-6,
            "anchor kept stored, got {}",
            a(8)
        );
        assert!(
            (sl(7) - 1.0).abs() < 1e-3,
            "slide cascade at 7, got {}",
            sl(7)
        );
        assert!(
            (sl(6) - 0.0).abs() < 1e-6,
            "non-cascade step, got {}",
            sl(6)
        );
    }

    #[test]
    fn melodic_preecho_ramps_an1x_accent_and_cascades_slide() {
        // Same shape as hoover test, targeting an1x.  Installing preecho
        // under the "an1x" key should only touch an1x triggers and leave
        // hoover / bass alone.
        use crate::sequencer::PreechoConfig;
        use crate::state::TB303Step;
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        seq.an1x_steps = 16;
        for s in seq.an1x_pattern.iter_mut() {
            *s = TB303Step {
                active: true,
                note: 60,
                accent: 0.0,
                slide: 0.0,
                pan: 0.0,
                gate: 0.5,
            };
        }
        seq.preecho.insert(
            "an1x".to_string(),
            PreechoConfig {
                enabled: true,
                anchors: vec![8],
                length: 4,
                auto_length: false,
                curve: Default::default(),
                velocity_ramp: false,
                ratchet_ramp: false,
                probability_ramp: false,
                accent_ramp: true,
                slide_cascade: true,
                note_approach: crate::sequencer::NoteApproach::Off,
            },
        );
        let sps = samples_per_step(120.0, 44100.0) as usize;
        let mut clock = ClockState::default();
        let mut accents: std::collections::HashMap<usize, f32> = Default::default();
        let mut slides: std::collections::HashMap<usize, f32> = Default::default();
        for _ in 0..20 {
            let prev = clock.current_step;
            let (next, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
            if next.current_step != prev {
                let vstep = next.current_step % 16;
                for e in &events {
                    if let TriggerEvent::An1xTrigger { accent, slide, .. } = e {
                        accents.entry(vstep).or_insert(*accent);
                        slides.entry(vstep).or_insert(*slide);
                    }
                }
            }
            clock = next;
        }
        let a = |s: usize| accents.get(&s).copied().unwrap_or(-1.0);
        let sl = |s: usize| slides.get(&s).copied().unwrap_or(-1.0);
        assert!(
            a(4) < a(5) && a(5) < a(6) && a(6) < a(7),
            "expected an1x accent ramp 4<5<6<7, got {} {} {} {}",
            a(4),
            a(5),
            a(6),
            a(7),
        );
        assert!((a(7) - 1.0).abs() < 1e-3, "expected a(7)=1.0, got {}", a(7));
        assert!(
            (sl(7) - 1.0).abs() < 1e-3,
            "slide cascade at 7, got {}",
            sl(7)
        );
        assert!(
            (sl(6) - 0.0).abs() < 1e-6,
            "non-cascade step, got {}",
            sl(6)
        );
    }

    #[test]
    fn note_approach_chromatic_rewrites_bass_lead_in_notes() {
        // Bass voice 0 fires on every step; all steps store note 57 (A3).
        // Preecho anchor=8, length=4, note_approach=Chromatic.
        // Lead-in 4..=7 should play anchor_note - d: 54, 55, 56, 57... wait
        // anchor_note is pattern[8].note=57, and d=4..1 → 53,54,55,56.
        use crate::sequencer::{NoteApproach, PreechoConfig};
        use crate::state::TB303Step;
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        seq.bass_voice_enabled[0] = true;
        for s in seq.bass_pattern.iter_mut() {
            *s = TB303Step {
                active: true,
                note: 57,
                accent: 0.0,
                slide: 0.0,
                pan: 0.0,
                gate: 0.5,
            };
        }
        seq.preecho.insert(
            "bass".to_string(),
            PreechoConfig {
                enabled: true,
                anchors: vec![8],
                length: 4,
                auto_length: false,
                curve: Default::default(),
                velocity_ramp: false,
                ratchet_ramp: false,
                probability_ramp: false,
                accent_ramp: false,
                slide_cascade: false,
                note_approach: NoteApproach::Chromatic,
            },
        );
        let sps = samples_per_step(120.0, 44100.0) as usize;
        let mut clock = ClockState::default();
        let mut notes: std::collections::HashMap<usize, u8> = Default::default();
        for _ in 0..20 {
            let prev = clock.current_step;
            let (next, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
            if next.current_step != prev {
                let vstep = next.current_step % 16;
                for e in &events {
                    if let TriggerEvent::BassTrigger {
                        voice_idx: 0, note, ..
                    } = e
                    {
                        notes.entry(vstep).or_insert(*note);
                    }
                }
            }
            clock = next;
        }
        // Lead-in: d=4 → 57-4=53, d=3 → 54, d=2 → 55, d=1 → 56.
        assert_eq!(notes.get(&4), Some(&53));
        assert_eq!(notes.get(&5), Some(&54));
        assert_eq!(notes.get(&6), Some(&55));
        assert_eq!(notes.get(&7), Some(&56));
        // Anchor stays at stored note.
        assert_eq!(notes.get(&8), Some(&57));
        // Out-of-window stays at stored note.
        assert_eq!(notes.get(&0), Some(&57));
        assert_eq!(notes.get(&12), Some(&57));
    }

    #[test]
    fn note_approach_scale_walks_scale_degrees() {
        // Pattern in A natural minor (root=9).  Anchor at step 8 stores A4=69.
        // Scale approach walks scale degrees down: d=1 → G4=67, d=2 → F4=65,
        // d=3 → E4=64.  Verifies the sequencer resolves through the scale
        // rather than straight semitones.
        use crate::sequencer::{NoteApproach, PreechoConfig};
        use crate::state::{Scale, TB303Step};
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            root_note: 9,
            scale: Scale::NaturalMinor,
            ..SequencerState::default()
        };
        seq.bass_voice_enabled[0] = true;
        for s in seq.bass_pattern.iter_mut() {
            *s = TB303Step {
                active: true,
                note: 69,
                accent: 0.0,
                slide: 0.0,
                pan: 0.0,
                gate: 0.5,
            };
        }
        seq.preecho.insert(
            "bass".to_string(),
            PreechoConfig {
                enabled: true,
                anchors: vec![8],
                length: 3,
                auto_length: false,
                curve: Default::default(),
                velocity_ramp: false,
                ratchet_ramp: false,
                probability_ramp: false,
                accent_ramp: false,
                slide_cascade: false,
                note_approach: NoteApproach::Scale,
            },
        );
        let sps = samples_per_step(120.0, 44100.0) as usize;
        let mut clock = ClockState::default();
        let mut notes: std::collections::HashMap<usize, u8> = Default::default();
        for _ in 0..20 {
            let prev = clock.current_step;
            let (next, events) = advance_clock(clock, &seq, sps + 1, 44100.0);
            if next.current_step != prev {
                let vstep = next.current_step % 16;
                for e in &events {
                    if let TriggerEvent::BassTrigger {
                        voice_idx: 0, note, ..
                    } = e
                    {
                        notes.entry(vstep).or_insert(*note);
                    }
                }
            }
            clock = next;
        }
        // Anchor-adjacent (d=1) → G4 = 67.
        assert_eq!(notes.get(&7), Some(&67));
        // d=2 → F4 = 65.
        assert_eq!(notes.get(&6), Some(&65));
        // d=3 → E4 = 64.
        assert_eq!(notes.get(&5), Some(&64));
    }

    #[test]
    fn ratchet_2_emits_sub_hit_after_step_fires() {
        let mut seq = SequencerState {
            running: true,
            bpm: 120.0,
            ..SequencerState::default()
        };
        seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[1] = Step {
            active: true,
            velocity: 1.0,
            probability: 1.0,
            ratchet: 2,
            slice: 0,
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
