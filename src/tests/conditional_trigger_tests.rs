// ─── tests/conditional_trigger_tests.rs ──────────────────────────────────────
// Conditional triggers: `cond` 0..=3 maps to "fire every 1/2/3/4
// voice cycles", inspired by Monome-style evolving patterns.  Drive
// `advance_clock` for several full voice cycles and confirm steps
// fire on the right cadence — drum + bass paths share the same
// `cond_gate` so testing one of each covers both.

use crate::sequencer::{ClockState, TriggerEvent, advance_clock, cond_gate, samples_per_step};
use crate::state::{DrumVoice, SequencerState, Step, TB303Step};

// ─── cond_gate pure helper ───────────────────────────────────────────────────

#[test]
fn cond_gate_zero_always_fires() {
    for cycle in 0..16 {
        assert!(cond_gate(cycle, 0));
    }
}

#[test]
fn cond_gate_one_fires_every_other_cycle() {
    assert!(cond_gate(0, 1));
    assert!(!cond_gate(1, 1));
    assert!(cond_gate(2, 1));
    assert!(!cond_gate(3, 1));
}

#[test]
fn cond_gate_three_fires_every_fourth_cycle() {
    assert!(cond_gate(0, 3));
    assert!(!cond_gate(1, 3));
    assert!(!cond_gate(2, 3));
    assert!(!cond_gate(3, 3));
    assert!(cond_gate(4, 3));
}

// ─── Step + TB303Step defaults ───────────────────────────────────────────────

#[test]
fn step_defaults_have_cond_zero() {
    let s = Step::default();
    assert_eq!(s.cond, 0);
    let b = TB303Step::default();
    assert_eq!(b.cond, 0);
}

// ─── Drum cond gating in advance_clock ───────────────────────────────────────

fn drum_step(cond: u8) -> Step {
    Step {
        active: true,
        velocity: 1.0,
        probability: 1.0,
        ratchet: 1,
        slice: 0,
        cond,
    }
}

#[test]
fn drum_cond_one_fires_only_on_even_cycles() {
    // 16-step kick pattern with step 0 set to cond=1 (every other
    // cycle).  Drive 4 full cycles → expect 2 fires (cycles 0, 2).
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        ..SequencerState::default()
    };
    seq.drum_steps.insert(DrumVoice::Kick808, 16);
    if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Kick808) {
        for s in pattern.iter_mut() {
            *s = Step::default();
        }
        pattern[0] = drum_step(1);
    }

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    // 4 voice cycles × 16 steps + slack.
    let block = sps * 64 + 1;
    let clock = ClockState::default();
    let (_, events) = advance_clock(clock, &seq, block, sr);

    let kick_count = events
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
    assert_eq!(
        kick_count, 2,
        "cond=1 over 4 cycles should fire 2× (cycles 0 + 2), got {kick_count}",
    );
}

#[test]
fn drum_cond_three_fires_every_fourth_cycle() {
    // step 0 cond=3 → fires only on cycles 0, 4, 8 …  Run for
    // 12 cycles → 3 fires.
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        ..SequencerState::default()
    };
    seq.drum_steps.insert(DrumVoice::Kick808, 16);
    if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Kick808) {
        for s in pattern.iter_mut() {
            *s = Step::default();
        }
        pattern[0] = drum_step(3);
    }

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    let block = sps * 192 + 1; // 12 cycles × 16 steps
    let clock = ClockState::default();
    let (_, events) = advance_clock(clock, &seq, block, sr);

    let kick_count = events
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
    assert_eq!(
        kick_count, 3,
        "cond=3 over 12 cycles should fire 3× (cycles 0/4/8), got {kick_count}",
    );
}

// ─── Bass cond gating ────────────────────────────────────────────────────────

fn bass_step(note: u8, cond: u8) -> TB303Step {
    TB303Step {
        active: true,
        note,
        accent: 0.0,
        slide: 0.0,
        gate: 0.5,
        pan: 0.0,
        cond,
    }
}

#[test]
fn bass_cond_one_fires_every_other_cycle() {
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        bass_steps: 8,
        ..SequencerState::default()
    };
    seq.bass_voice_steps[0] = 8;
    seq.bass_pattern[0] = bass_step(50, 1);
    seq.bass_patterns[0][0] = bass_step(50, 1);

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    // 6 cycles × 8 steps + slack.
    let block = sps * 48 + 1;
    let clock = ClockState::default();
    let (_, events) = advance_clock(clock, &seq, block, sr);

    let bass_count = events
        .iter()
        .filter(|e| matches!(e, TriggerEvent::BassTrigger { note: 50, .. }))
        .count();
    assert_eq!(
        bass_count, 3,
        "cond=1 across 6 cycles should fire 3× (cycles 0/2/4), got {bass_count}",
    );
}

#[test]
fn bass_cond_zero_fires_every_cycle() {
    // Sanity / regression: the default cond=0 must still fire on
    // every voice cycle so existing patterns are unaffected.
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        bass_steps: 4,
        ..SequencerState::default()
    };
    seq.bass_voice_steps[0] = 4;
    seq.bass_pattern[0] = bass_step(60, 0);
    seq.bass_patterns[0][0] = bass_step(60, 0);

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    let block = sps * 20 + 1; // 5 cycles
    let clock = ClockState::default();
    let (_, events) = advance_clock(clock, &seq, block, sr);

    let bass_count = events
        .iter()
        .filter(|e| matches!(e, TriggerEvent::BassTrigger { note: 60, .. }))
        .count();
    assert_eq!(bass_count, 5, "cond=0 must fire every cycle");
}
