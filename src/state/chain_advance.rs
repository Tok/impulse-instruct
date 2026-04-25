// ─── state/chain_advance.rs ──────────────────────────────────────────────────
// Pure decision logic for "what does this loop boundary mean".  Lives
// here so the audio thread's chain-advance branch is a thin dispatcher
// over a unit-tested kernel — the previous inline logic in
// `audio/mod.rs` mixed clock state, override resolution, transport
// preservation, and morph scheduling, none of which were directly
// testable without standing up an audio engine.
//
// Coverage philosophy: the audio thread shell (cpal callback + rtrb
// drain) is excluded from coverage by design.  Every meaningful
// branch in the chain-advance flow now has a pure-function home and
// matching test, so edits to the policy can't regress silently.

use super::SequencerState;
use super::song::ChainSlotOverride;
use super::transitions::{chain_advance_preserve_non_bass, chain_advance_transport};

/// Outcome of one loop boundary, as decided by the pure
/// classifier.  The audio thread picks the matching handler:
///
/// * `None` — chain disabled or empty; nothing to do.
/// * `BumpRepeatCount(n)` — stay on the current slot, set
///   `chain_repeat_count = n`.
/// * `StopAtEnd` — one-shot song just finished its last slot's last
///   repeat; transport stops and step-zero events should be dropped
///   (the `advance_clock` path emitted a phantom restart already).
/// * `AdvanceTo { … }` — load the named slot, optionally with a
///   morph window and a style override.
#[derive(Clone, Debug, PartialEq)]
pub enum LoopBoundaryAction {
    None,
    BumpRepeatCount(u8),
    StopAtEnd,
    AdvanceTo {
        next_pos: usize,
        next_slot: usize,
        morph_bars: u8,
        eff_bpm: f32,
        eff_swing: f32,
        effective_style: Option<String>,
    },
}

/// Decide what the loop boundary means without touching state.  All
/// inputs are explicit so the policy is testable in isolation.
///
/// `chain` / `overrides` mirror `AppState.chain` / `chain_overrides`.
/// `chain_pos` and `chain_repeat_count` are the values BEFORE this
/// boundary.  `chain_loop` mirrors `AppState.chain_loop`.  `loaded`
/// is the pattern bank slot that would be loaded if we advance; it
/// only matters when the action is `AdvanceTo` (the function reads
/// `pattern_style` / `pattern_bpm_apply` / `bpm` / `swing` from it).
pub fn classify_loop_boundary(
    chain: &[usize],
    overrides: &[ChainSlotOverride],
    chain_pos: usize,
    chain_repeat_count: u8,
    chain_loop: bool,
    loaded: Option<&SequencerState>,
    prior_bpm: f32,
    prior_swing: f32,
) -> LoopBoundaryAction {
    if chain.is_empty() {
        return LoopBoundaryAction::None;
    }
    let cur_pos = chain_pos % chain.len();
    let cur_override = overrides.get(cur_pos);
    let cur_repeats = cur_override.map(|o| o.repeats.max(1)).unwrap_or(1);
    if chain_repeat_count + 1 < cur_repeats {
        return LoopBoundaryAction::BumpRepeatCount(chain_repeat_count + 1);
    }
    if !chain_loop && cur_pos + 1 >= chain.len() {
        return LoopBoundaryAction::StopAtEnd;
    }
    let next_pos = (cur_pos + 1) % chain.len();
    let next_slot = chain[next_pos];
    let next_override = overrides.get(next_pos);
    let morph_bars = next_override.map(|o| o.morph_bars).unwrap_or(0);
    let effective_style = next_override
        .and_then(|o| o.style.clone())
        .or_else(|| loaded.and_then(|l| l.pattern_style.clone()));
    let (eff_bpm, eff_swing) = match next_override.and_then(|o| o.bpm) {
        Some(b) => (b, loaded.map(|l| l.swing).unwrap_or(prior_swing)),
        None => match loaded {
            Some(l) if l.pattern_bpm_apply => (l.bpm, l.swing),
            _ => (prior_bpm, prior_swing),
        },
    };
    LoopBoundaryAction::AdvanceTo {
        next_pos,
        next_slot,
        morph_bars,
        eff_bpm,
        eff_swing,
        effective_style,
    }
}

/// Apply an `AdvanceTo` action to the loaded slot's `SequencerState`,
/// returning the target sequencer for either the immediate-replace
/// path or the morph stash path.  Wraps the existing
/// `chain_advance_transport` / `chain_advance_preserve_non_bass`
/// transitions so the audio thread's choice between them is one
/// pure call instead of two inlined branches.
pub fn build_advance_target(
    loaded: SequencerState,
    prior_seq: &SequencerState,
    chain_loop: bool,
    eff_bpm: f32,
    eff_swing: f32,
    running: bool,
) -> SequencerState {
    let mut loaded = loaded;
    loaded.bpm = eff_bpm;
    loaded.swing = eff_swing;
    loaded.pattern_bpm_apply = true;
    if chain_loop {
        chain_advance_transport(loaded, eff_bpm, eff_swing, running)
    } else {
        chain_advance_preserve_non_bass(loaded, prior_seq, running)
    }
}
