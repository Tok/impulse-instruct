// ─── tests/amen_sequencer_tests.rs ───────────────────────────────────────────
// End-to-end coverage of the amen step→slice mapping path.  The DSP-
// level cycle is already covered in samplers_tests.rs; this file
// drives the actual `advance_clock` so the sequencer's `effective_slice`
// resolution + amen_slice_order interaction get exercised together.

use crate::sequencer::{ClockState, TriggerEvent, advance_clock, samples_per_step};
use crate::state::{DrumVoice, SequencerState, Step};

fn step_active() -> Step {
    Step {
        active: true,
        velocity: 1.0,
        probability: 1.0,
        ratchet: 1,
        slice: 0, // 0 = auto-advance — sequencer maps step N → slice N
        cond: 0,
    }
}

/// Reproduce the user-reported "amen only loops the last fragment" bug
/// at the sequencer level.  With every step active and step.slice = 0
/// (default), the sequencer should emit slice values that decode to a
/// rotating slice index — not all the same slice.
#[test]
fn advance_clock_emits_distinct_amen_slices_across_steps() {
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        ..SequencerState::default()
    };
    seq.drum_steps.insert(DrumVoice::Amen, 8);
    if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Amen) {
        for i in 0..8 {
            pattern[i] = step_active();
        }
    }

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    // 8 step ticks + 1 sample slack so the accumulator definitely
    // crosses the 8th boundary.
    let block = sps * 8 + 1;
    let (_, events) = advance_clock(ClockState::default(), &seq, block, sr);

    let amen_slices: Vec<u8> = events
        .iter()
        .filter_map(|ev| match ev {
            TriggerEvent::DrumTrigger {
                voice: DrumVoice::Amen,
                slice,
                ..
            } => Some(*slice),
            _ => None,
        })
        .collect();
    assert_eq!(amen_slices.len(), 8, "every active step must fire once");
    // Sequencer maps step N → effective_slice N+1 (so the DSP's
    // `(slice-1) % slice_count` decodes to N).  All 8 slice values
    // distinct = bug not present.
    let unique: std::collections::BTreeSet<u8> = amen_slices.iter().copied().collect();
    assert_eq!(
        unique.len(),
        8,
        "expected 8 distinct slice values, got {amen_slices:?}"
    );
}

/// `amen_slice_order` permutes step → slice mapping.  When the order
/// vec is populated (e.g. via the panel's slice-order strip), each
/// step's effective slice = `order[vstep % len] + 1`.  Asserts the
/// permutation round-trips through `advance_clock` — a regression
/// here would silently collapse multiple steps onto the same slice.
///
/// Note: `advance_clock` increments the global tick before the
/// pattern lookup (`step += 1` then `vstep = step % len`), so a
/// fresh `ClockState` starting at `current_step = 0` first fires
/// vstep = 1, not 0.  That convention is established and tested in
/// `polymeter_tests.rs`; the expected slice sequence below reflects
/// it (vstep order = 1, 2, 3, 0).
#[test]
fn advance_clock_honours_amen_slice_order_permutation() {
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        ..SequencerState::default()
    };
    seq.drum_steps.insert(DrumVoice::Amen, 4);
    if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Amen) {
        for i in 0..4 {
            pattern[i] = step_active();
        }
    }
    // Reverse order: vstep 0 → slice 3, vstep 1 → slice 2, etc.
    seq.amen_slice_order = vec![3, 2, 1, 0];

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    let block = sps * 4 + 1;
    let (_, events) = advance_clock(ClockState::default(), &seq, block, sr);

    let amen_slices: Vec<u8> = events
        .iter()
        .filter_map(|ev| match ev {
            TriggerEvent::DrumTrigger {
                voice: DrumVoice::Amen,
                slice,
                ..
            } => Some(*slice),
            _ => None,
        })
        .collect();
    // First fired vstep is 1 (sequencer increments before lookup);
    // sequence vstep = 1, 2, 3, 0.  Effective slice = order[vstep] + 1.
    assert_eq!(amen_slices, vec![3, 2, 1, 4]);
}

/// Steps with an explicit `slice` (1..=16) override the auto-advance
/// path.  Pin that `step.slice = 5` flows through `advance_clock`
/// unchanged — guards against a future refactor that accidentally
/// overrides the explicit value with the auto-advance fallback.
#[test]
fn advance_clock_passes_through_explicit_slice() {
    let mut seq = SequencerState {
        running: true,
        bpm: 240.0,
        ..SequencerState::default()
    };
    seq.drum_steps.insert(DrumVoice::Amen, 4);
    if let Some(pattern) = seq.drum_patterns.get_mut(&DrumVoice::Amen) {
        for i in 0..4 {
            pattern[i] = Step {
                slice: 5, // explicit — every step plays slice 4 (DSP idx)
                ..step_active()
            };
        }
    }

    let sr = 48_000.0_f32;
    let sps = samples_per_step(240.0, sr, 4) as usize;
    let block = sps * 4 + 1;
    let (_, events) = advance_clock(ClockState::default(), &seq, block, sr);

    let amen_slices: Vec<u8> = events
        .iter()
        .filter_map(|ev| match ev {
            TriggerEvent::DrumTrigger {
                voice: DrumVoice::Amen,
                slice,
                ..
            } => Some(*slice),
            _ => None,
        })
        .collect();
    assert_eq!(amen_slices, vec![5, 5, 5, 5]);
}
