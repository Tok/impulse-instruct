// ─── tests/polymeter_tests.rs ────────────────────────────────────────────────
// Polymeter — per-voice step lengths that don't divide each other
// (or `MAX_STEPS`) must phase cleanly without skipping or
// double-firing steps when the global tick counter would have
// wrapped under the old `% MAX_STEPS` regime.
//
// The classic case: 5-step bass against a 16-step drum kit.  The
// patterns realign every LCM(5, 16) = 80 ticks.  Drive
// `advance_clock` past the would-be wrap point (MAX_STEPS = 64) and
// confirm both voices fire their full sequences with the right
// cadence.

use crate::sequencer::{ClockState, TriggerEvent, advance_clock, samples_per_step};
use crate::state::{DrumVoice, SequencerState, Step, TB303Step};

fn step_active(velocity: f32) -> Step {
    Step {
        active: true,
        velocity,
        probability: 1.0,
        ratchet: 1,
        slice: 0,
    }
}

fn bass_step_active(note: u8) -> TB303Step {
    TB303Step {
        active: true,
        note,
        accent: 0.0,
        slide: 0.0,
        gate: 0.5,
        pan: 0.0,
    }
}

#[test]
fn five_step_bass_against_sixteen_step_kit_fires_full_lcm_cycle() {
    // bass_steps=5, kick_a steps=16.  LCM is 80.  Drive the clock
    // through 80 steps (well past MAX_STEPS=64) and count fires.
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        bass_steps: 5,
        ..SequencerState::default()
    };
    // Voice 0's per-voice step length mirrors bass_steps when set
    // explicitly — required because the trigger path reads
    // `bass_voice_steps[0]` (which defaults to 32).
    seq.bass_voice_steps[0] = 5;
    // Kit kick on step 0 of its 16-step lane.
    seq.drum_steps.insert(DrumVoice::Kick808, 16);
    seq.drum_patterns.get_mut(&DrumVoice::Kick808).unwrap()[0] = step_active(1.0);
    // Activate every bass step so we can count fires per cycle.
    for i in 0..5 {
        seq.bass_pattern[i] = bass_step_active(40 + i as u8);
        seq.bass_patterns[0][i] = bass_step_active(40 + i as u8);
    }

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    // 80 step ticks at sps each — drive in a single block so we
    // see every event in one slice.  +1 sample of slack so the
    // accumulator definitely crosses the 80th boundary.
    let block = sps * 80 + 1;

    let mut clock = ClockState::default();
    let (new_clock, events) = advance_clock(clock.clone(), &seq, block, sr);
    clock = new_clock;

    // Bass: every step active, lane length 5, fires once per tick →
    // 80 ticks should yield 80 bass triggers, evenly distributed
    // across notes 40..=44.
    let mut bass_count = 0_usize;
    let mut bass_per_note = [0_usize; 5];
    let mut kick_count = 0_usize;
    for ev in &events {
        match ev {
            TriggerEvent::BassTrigger { note, .. } => {
                bass_count += 1;
                if (40..45).contains(note) {
                    bass_per_note[(note - 40) as usize] += 1;
                }
            }
            TriggerEvent::DrumTrigger {
                voice: DrumVoice::Kick808,
                ..
            } => {
                kick_count += 1;
            }
            _ => {}
        }
    }
    // 80 ticks / 5-step bass = 16 cycles → each note fires 16 times.
    assert_eq!(bass_count, 80, "bass should fire on every tick");
    for (i, n) in bass_per_note.iter().enumerate() {
        assert_eq!(
            *n, 16,
            "bass note {i} should fire 16 times across the LCM cycle, got {n}",
        );
    }
    // 80 ticks / 16-step kick = 5 cycles, kick on step 0 only.
    assert_eq!(kick_count, 5, "kick should fire 5× across 80 ticks");
    // current_step is the running tick index — 0 → 80 (past
    // MAX_STEPS) without wrap.
    assert_eq!(clock.current_step, 80);
}

#[test]
fn voice_indexing_stays_continuous_across_max_steps_boundary() {
    // Targeted check: with bass_steps=5 the voice index sequence
    // around tick 64 must be …62→2, 63→3, 64→4, 65→0, 66→1 (no
    // skip from 3→0 like the old wrap caused).  We verify by
    // ensuring two specific bass slots fire exactly once each
    // across a 6-tick window straddling the boundary.
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        bass_steps: 5,
        ..SequencerState::default()
    };
    seq.bass_voice_steps[0] = 5;
    // Light up bass slot 4 (the would-be-skipped step under the
    // old wrap) and bass slot 0.
    seq.bass_pattern[0] = bass_step_active(50);
    seq.bass_pattern[4] = bass_step_active(54);
    seq.bass_patterns[0][0] = bass_step_active(50);
    seq.bass_patterns[0][4] = bass_step_active(54);

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    let block = sps * 6 + 1;

    // Start the clock at tick 62 so the 6-step window covers
    // 63 (slot 3, no fire), 64 (slot 4, fires note 54), 65 (slot 0,
    // fires note 50), 66 (slot 1, no fire), 67 (slot 2, no fire).
    let clock = ClockState {
        current_step: 62,
        ..ClockState::default()
    };

    let (_, events) = advance_clock(clock, &seq, block, sr);
    let bass_notes: Vec<u8> = events
        .iter()
        .filter_map(|e| match e {
            TriggerEvent::BassTrigger { note, .. } => Some(*note),
            _ => None,
        })
        .collect();
    assert!(
        bass_notes.contains(&54),
        "slot 4 (note 54) should fire across the would-be MAX_STEPS boundary, got {bass_notes:?}",
    );
    assert!(
        bass_notes.contains(&50),
        "slot 0 (note 50) should fire on the next tick after slot 4, got {bass_notes:?}",
    );
    // Old broken behaviour skipped slot 4 every 64 ticks.  A single
    // pass through this window must hit it exactly once — count to
    // make sure we don't double-fire either.
    let count_54 = bass_notes.iter().filter(|&&n| n == 54).count();
    let count_50 = bass_notes.iter().filter(|&&n| n == 50).count();
    assert_eq!(count_54, 1, "slot 4 should fire exactly once");
    assert_eq!(count_50, 1, "slot 0 should fire exactly once");
}

#[test]
fn loop_count_increments_per_seq_steps_cycle_not_per_max_steps() {
    // loop_count drives chain advancement + probability RNG seed
    // variation.  Under polymeter it should track `seq.steps`
    // boundaries — 32 ticks per loop with the default settings.
    let seq = SequencerState {
        running: true,
        bpm: 240.0,
        steps: 32,
        ..SequencerState::default()
    };

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    // Drive 96 steps — three full 32-step cycles — with sufficient
    // slack to cross every boundary.
    let block = sps * 96 + 1;

    let clock = ClockState::default();
    let (new_clock, _) = advance_clock(clock, &seq, block, sr);
    assert_eq!(
        new_clock.loop_count, 3,
        "three full 32-step cycles must yield loop_count=3, got {}",
        new_clock.loop_count,
    );
    assert_eq!(new_clock.current_step, 96);
}
