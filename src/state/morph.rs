// ─── state/morph.rs ──────────────────────────────────────────────────────────
// Pattern morphing on chain advance — step-by-step crossfade between an
// outgoing pattern and an incoming one over a configurable number of
// loop boundaries.
//
// Trigger: when the song chain advances into a slot whose
// `ChainSlotOverride.morph_bars > 0`, the audio thread stashes the
// new pattern as `ChainMorph::target` and *keeps the prior pattern
// playing*.  On every subsequent loop boundary the morph's progress
// fraction grows (`bars_done / bars_total`); each tick replaces a
// growing fraction of step indices with their counterparts in the
// target.  When `bars_done == bars_total` the live sequencer is
// snapped to the target and the morph state is cleared.
//
// Why bit-reversal for the index order: a linear "front half then
// back half" replacement makes the morph feel like a wipe, which is
// musically jarring.  Bit-reversal disperses replacements evenly
// across the pattern (rank 0 → index 0, rank 1 → index N/2, rank 2
// → index N/4, …) so the rhythm gains the new pattern's character
// gradually instead of swapping in chunks.

use serde::{Deserialize, Serialize};

use super::sequencer_state::SequencerState;

/// Live morph in flight — keeps the target pattern alongside the
/// progress counters.  `target` is boxed so a `None` morph (the
/// common case) doesn't bloat `AppState`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainMorph {
    pub target: Box<SequencerState>,
    pub bars_total: u8,
    pub bars_done: u8,
}

impl ChainMorph {
    pub fn new(target: SequencerState, bars_total: u8) -> Self {
        Self {
            target: Box::new(target),
            bars_total: bars_total.clamp(1, 8),
            bars_done: 0,
        }
    }

    /// True when this morph is fully replaced — caller should snap
    /// the sequencer to `target` and clear the morph state.
    pub fn is_complete(&self) -> bool {
        self.bars_done >= self.bars_total
    }
}

/// Bit-reversal dispersal rank — used to decide which step indices
/// get swapped first as the morph progresses.  For a power-of-two
/// `len`, the rank order is the standard radix-2 FFT bit-reversal:
/// rank 0 → 0, rank 1 → len/2, rank 2 → len/4, etc.  For non-power-
/// of-two lengths we mod the bit-reversed value back into range, so
/// some indices share a rank — acceptable since we use the rank as
/// a threshold comparator, not a permutation.
#[inline]
pub fn bit_reverse_rank(i: usize, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let bits = (len.next_power_of_two().trailing_zeros()).max(1);
    let mut x = i as u32;
    let mut r: u32 = 0;
    for _ in 0..bits {
        r = (r << 1) | (x & 1);
        x >>= 1;
    }
    (r as usize) % len
}

/// Threshold for "is index `i` swapped in by now?" given the current
/// morph progress.  Returns `true` when step `i` should be sourced
/// from the target pattern, `false` to keep the previous pattern's
/// step at that index.
#[inline]
pub fn step_swapped(i: usize, len: usize, bars_done: u8, bars_total: u8) -> bool {
    if bars_total == 0 || len == 0 {
        return false;
    }
    let rank = bit_reverse_rank(i, len);
    let threshold = (bars_done as usize * len) / bars_total as usize;
    rank < threshold
}

/// Apply one morph tick to the live sequencer.  Each tick increments
/// `bars_done`; if the morph is complete the live sequencer is
/// replaced wholesale with the target.  Otherwise the live sequencer's
/// six pattern arrays (drum patterns, bass voices + legacy mirror,
/// hoover, an1x, pluck, wavetable) get a fraction of their step slots
/// replaced with the same-index step from the target, ordered by
/// `bit_reverse_rank`.  Step counts and non-pattern fields are left
/// untouched until the final tick — only the per-step flags evolve.
pub fn morph_tick(live: SequencerState, morph: &mut ChainMorph) -> SequencerState {
    morph.bars_done = morph.bars_done.saturating_add(1);
    if morph.is_complete() {
        return (*morph.target).clone();
    }
    let mut s = live;
    let bd = morph.bars_done;
    let bt = morph.bars_total;
    let target = morph.target.as_ref();

    // Drum patterns — same key set as the live state; keys missing in
    // the target are left alone (skipping morph for that voice).
    let drum_keys: Vec<_> = s.drum_patterns.keys().cloned().collect();
    for voice in drum_keys {
        if let (Some(live_p), Some(tgt_p)) = (
            s.drum_patterns.get_mut(&voice),
            target.drum_patterns.get(&voice),
        ) {
            let len = live_p.len().min(tgt_p.len());
            for i in 0..len {
                if step_swapped(i, len, bd, bt) {
                    live_p[i] = tgt_p[i];
                }
            }
        }
    }

    // Bass voices — per-voice patterns plus legacy mirror.
    let voice_count = s.bass_patterns.len().min(target.bass_patterns.len());
    for vi in 0..voice_count {
        let len = s.bass_patterns[vi]
            .len()
            .min(target.bass_patterns[vi].len());
        for i in 0..len {
            if step_swapped(i, len, bd, bt) {
                s.bass_patterns[vi][i] = target.bass_patterns[vi][i];
            }
        }
    }
    let len = s.bass_pattern.len().min(target.bass_pattern.len());
    for i in 0..len {
        if step_swapped(i, len, bd, bt) {
            s.bass_pattern[i] = target.bass_pattern[i];
        }
    }

    // Single-voice melodic lanes.
    morph_tb303_pattern(&mut s.hoover_pattern, &target.hoover_pattern, bd, bt);
    morph_tb303_pattern(&mut s.an1x_pattern, &target.an1x_pattern, bd, bt);
    morph_tb303_pattern(&mut s.pluck_pattern, &target.pluck_pattern, bd, bt);
    morph_tb303_pattern(&mut s.wavetable_pattern, &target.wavetable_pattern, bd, bt);

    s
}

fn morph_tb303_pattern(
    live: &mut [super::TB303Step],
    target: &[super::TB303Step],
    bars_done: u8,
    bars_total: u8,
) {
    let len = live.len().min(target.len());
    for i in 0..len {
        if step_swapped(i, len, bars_done, bars_total) {
            live[i] = target[i];
        }
    }
}
