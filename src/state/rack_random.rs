// ─── state/rack_random.rs ─────────────────────────────────────────────────────
// Eurorack-style "patch generator" — composes a random rack layout
// from a seed.  Pure: same seed → same layout, no allocations beyond
// the returned Vecs, no clock / nanos reads inside the generator.
//
// Used by `POST /api/rack/random` and the UI "Surprise me" button to
// produce a creative-seed rack the user can tinker with.  The
// randomness is intentionally bounded — picks live in curated pools
// per role (voice / FX / mod) so the result is always playable, not
// just a chaotic dump of every kind.
//
// Cable wiring is delegated to `RackState::wire_default_cables` after
// the modules land, plus a handful of extra "creative" cables this
// generator inserts: one or two voice → FX → master forks and a
// couple of LFO mod-cable hits to keep the result moving.

use super::ModuleKind;

/// Curated pool of voices that can land in a randomised rack.  The
/// order does not affect the result — `pick_distinct` walks the
/// slice deterministically based on the seed-driven RNG.  Voices
/// kept out (`SampleInstrument`, `NeuTts`) need user-loaded assets
/// (a sample / a voice prompt) to be useful, so they don't make
/// sense as random picks.
const VOICE_POOL: &[ModuleKind] = &[
    ModuleKind::AcidBass,
    ModuleKind::DrumKit808,
    ModuleKind::DrumKit909,
    ModuleKind::HooverLead,
    ModuleKind::An1xVoice,
    ModuleKind::PluckString,
    ModuleKind::WavetableVoice,
    ModuleKind::NoiseVoice,
    ModuleKind::GranularTexture,
    ModuleKind::GabberKick,
    ModuleKind::AmenSampler,
    ModuleKind::FmOpsVoice,
    ModuleKind::AdditiveVoice,
    ModuleKind::ModalVoice,
    ModuleKind::ChiptuneVoice,
    ModuleKind::VocalVoice,
];

/// Curated pool of FX modules.  Includes both the cheap-and-cheerful
/// transformers (Reverb / Delay / Chorus / Phaser) and a few
/// character / surprise picks (RingMod / FreqShift / Stutter).
/// Master-stage latches like `FxWiden` stay in the pool — they slot
/// in transparently because the master applies their state when the
/// chain step runs.
const FX_POOL: &[ModuleKind] = &[
    ModuleKind::FxReverb,
    ModuleKind::FxDelay,
    ModuleKind::FxChorus,
    ModuleKind::FxPhaser,
    ModuleKind::FxFlanger,
    ModuleKind::FxBitcrush,
    ModuleKind::FxRingMod,
    ModuleKind::FxComb,
    ModuleKind::FxFilter,
    ModuleKind::FxFreqShift,
    ModuleKind::FxPitchShift,
    ModuleKind::FxStutter,
    ModuleKind::FxFreeze,
    ModuleKind::FxMultitap,
    ModuleKind::FxRevDelay,
    ModuleKind::FxTapeStop,
    ModuleKind::FxTransient,
    ModuleKind::FxWaveshaper,
    ModuleKind::FxDrive,
    ModuleKind::FxTilt,
    ModuleKind::FxAutotune,
    ModuleKind::FxPan,
    ModuleKind::FxWiden,
    ModuleKind::FxVinyl,
    ModuleKind::FxDjFilter,
    ModuleKind::FxTremolo,
    ModuleKind::FxVibrato,
    ModuleKind::FxIsoEq,
    ModuleKind::FxDeEsser,
    ModuleKind::FxResBank,
    ModuleKind::FxTapeEcho,
    ModuleKind::FxMultibandComp,
    ModuleKind::FxGrainDelay,
    ModuleKind::FxSpectralGate,
    ModuleKind::FxPlate,
    ModuleKind::FxTranceGate,
    ModuleKind::FxWaveFolder,
];

/// Result of `random_layout` — a recipe the caller applies to its
/// `RackState`.  Order matters for the UI: voices land first so they
/// occupy the voice zone in a predictable left-to-right scan, then
/// FX, then LFO modules.  Cable list is left empty by V1 — caller
/// uses `wire_default_cables` after applying.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomLayout {
    pub voices: Vec<ModuleKind>,
    pub fx: Vec<ModuleKind>,
    pub lfo_count: u8,
}

/// Build a randomised layout from a 64-bit seed.  Always returns at
/// least one voice + one FX + one LFO so the resulting rack makes
/// sound.  Counts:
///   * voices: 2..=4
///   * FX:     3..=7
///   * LFOs:   1..=3
///
/// Pure — same seed produces the same output, today and forever.
/// Uses a tiny LCG (no `rand` crate dependency) since we just need
/// a few dozen well-distributed bits, not cryptographic randomness.
pub fn random_layout(seed: u64) -> RandomLayout {
    let mut rng = LcgRng::new(seed);
    let n_voices = 2 + (rng.next_bounded(3)) as usize; // 2..=4
    let n_fx = 3 + (rng.next_bounded(5)) as usize; // 3..=7
    let lfo_count = 1 + (rng.next_bounded(3)) as u8; // 1..=3
    RandomLayout {
        voices: pick_distinct(VOICE_POOL, n_voices, &mut rng),
        fx: pick_distinct(FX_POOL, n_fx, &mut rng),
        lfo_count,
    }
}

/// Pick `n` distinct entries from `pool` using `rng`.  Caps at the
/// pool length so a request larger than the pool returns the whole
/// pool (rather than panicking or duplicating).  Order of the result
/// is the pool's natural order — only *which* entries get included
/// depends on the RNG, not their position.  Keeping ordering stable
/// makes the resulting rack easier to reason about visually.
fn pick_distinct(pool: &[ModuleKind], n: usize, rng: &mut LcgRng) -> Vec<ModuleKind> {
    let cap = n.min(pool.len());
    if cap == 0 {
        return Vec::new();
    }
    // Reservoir-style shuffle: pick `cap` indices via repeated rolls
    // that skip duplicates.  For small `cap` (≤8) the expected
    // re-roll count is negligible; this avoids allocating a full
    // shuffle vector.
    let mut chosen: Vec<usize> = Vec::with_capacity(cap);
    let mut guard = 0usize;
    while chosen.len() < cap && guard < 1024 {
        let idx = rng.next_bounded(pool.len() as u32) as usize;
        if !chosen.contains(&idx) {
            chosen.push(idx);
        }
        guard += 1;
    }
    chosen.sort_unstable();
    chosen.into_iter().map(|i| pool[i]).collect()
}

/// Apply a `RandomLayout` to an `AppState` in place: wipes the rack
/// down to its persistent core (sequencer + master + LLM console),
/// drops the new modules, calls `wire_default_cables` so every voice
/// reaches master out of the box, then re-runs `arrange_canonical`
/// so the bin-packer lays everything out on the grid.  Returns the
/// layout that was applied (also obtainable from `random_layout(seed)`
/// directly — passed through here for the caller's convenience when
/// it wants to log "applied N voices, M fx" without re-rolling).
///
/// Pure aside from the `&mut AppState` mutation — same `(state,
/// seed)` produces the same end state.  Shared by the
/// `POST /api/rack/random` handler and the UI "Random Patch"
/// menu entry so both paths are guaranteed identical.
pub fn apply_random_layout(state: &mut crate::state::AppState, seed: u64) -> RandomLayout {
    let layout = random_layout(seed);

    // Same wipe as `post_rack_reset` — keep sequencer / master /
    // console (the persistent rack core), drop everything else
    // including cables.  Inlined rather than calling the API
    // handler because that one would deadlock on the lock the
    // caller already holds (the ImpulseApp / API write lock).
    let keep: Vec<u32> = state
        .rack
        .modules
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                ModuleKind::StepSequencer | ModuleKind::MasterOutput | ModuleKind::LlmConsole
            )
        })
        .map(|m| m.id)
        .collect();
    state.rack.modules.retain(|m| keep.contains(&m.id));
    state
        .rack
        .cables
        .retain(|c| keep.contains(&c.from.module_id) && keep.contains(&c.to.module_id));
    state.llm_agents.clear();
    state.tts_modules.clear();

    // Voices first → FX → LFOs.  Order is deterministic so the
    // bin-packer's left-to-right scan produces the same layout for
    // a given seed every time.
    for kind in &layout.voices {
        state.rack.add_module(*kind);
    }
    for kind in &layout.fx {
        state.rack.add_module(*kind);
    }
    for _ in 0..layout.lfo_count {
        state.rack.add_module(ModuleKind::LfoModule);
    }

    state.rack.wire_default_cables();
    state.rack.arrange_canonical();

    layout
}

/// Tiny linear-congruential generator.  Constants from Numerical
/// Recipes; they distribute evenly enough for picking modules from
/// short pools.  Not for crypto — for picking which knob the user
/// gets to twist next.
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        // Non-zero state so the first roll isn't always 0 when seed=0.
        Self {
            state: seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1),
        }
    }

    fn next(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // Top 32 bits have the better mixing in an LCG.
        (self.state >> 32) as u32
    }

    /// Uniform-ish in `0..bound`.  `bound == 0` returns 0 to avoid
    /// a divide-by-zero on caller error.
    fn next_bounded(&mut self, bound: u32) -> u32 {
        if bound == 0 { 0 } else { self.next() % bound }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_layout_is_deterministic_for_same_seed() {
        // Same seed → identical layout.  Locks the contract that
        // /api/rack/random?seed=N can be replayed for debugging or
        // demo recording without surprise.
        let a = random_layout(0xDEAD_BEEF);
        let b = random_layout(0xDEAD_BEEF);
        assert_eq!(a, b);
    }

    #[test]
    fn random_layout_varies_with_seed() {
        // Different seeds → at least one of voices / fx / lfo_count
        // differs.  Spot-check 8 seeds against seed 0; all should
        // diverge somewhere.
        let baseline = random_layout(0);
        let mut diverged = 0usize;
        for s in 1..=8 {
            if random_layout(s) != baseline {
                diverged += 1;
            }
        }
        assert_eq!(diverged, 8, "every alternate seed should differ");
    }

    #[test]
    fn random_layout_counts_are_in_documented_range() {
        // Voices 2..=4, FX 3..=7, LFOs 1..=3.  Sweep 64 seeds and
        // assert no roll lands outside the contract.
        for s in 0..64u64 {
            let l = random_layout(s);
            assert!(
                (2..=4).contains(&l.voices.len()),
                "seed {s}: voice count {} out of 2..=4",
                l.voices.len()
            );
            assert!(
                (3..=7).contains(&l.fx.len()),
                "seed {s}: fx count {} out of 3..=7",
                l.fx.len()
            );
            assert!(
                (1..=3).contains(&l.lfo_count),
                "seed {s}: lfo count {} out of 1..=3",
                l.lfo_count
            );
        }
    }

    #[test]
    fn random_layout_voices_and_fx_are_distinct_within_layout() {
        // No duplicate voices or FX in a single layout — the
        // generator picks from disjoint indices in each pool.
        for s in 0..32u64 {
            let l = random_layout(s);
            let mut v = l.voices.clone();
            v.sort_by_key(|k| format!("{k:?}"));
            v.dedup();
            assert_eq!(v.len(), l.voices.len(), "seed {s}: duplicate voice");
            let mut f = l.fx.clone();
            f.sort_by_key(|k| format!("{k:?}"));
            f.dedup();
            assert_eq!(f.len(), l.fx.len(), "seed {s}: duplicate fx");
        }
    }

    #[test]
    fn random_layout_picks_only_from_curated_pools() {
        // No surprise modules slip in from outside the curated
        // pools.  Catches regressions if VOICE_POOL / FX_POOL are
        // ever extended without updating the contract.
        let voice_set: std::collections::HashSet<&ModuleKind> = VOICE_POOL.iter().collect();
        let fx_set: std::collections::HashSet<&ModuleKind> = FX_POOL.iter().collect();
        for s in 0..32u64 {
            let l = random_layout(s);
            for v in &l.voices {
                assert!(voice_set.contains(v), "seed {s}: voice {v:?} not in pool");
            }
            for f in &l.fx {
                assert!(fx_set.contains(f), "seed {s}: fx {f:?} not in pool");
            }
        }
    }

    #[test]
    fn pick_distinct_caps_at_pool_length() {
        // Asking for more than the pool holds returns the whole
        // pool (no panic, no infinite loop on the dedup guard).
        let mut rng = LcgRng::new(7);
        let picked = pick_distinct(&[ModuleKind::FxReverb, ModuleKind::FxDelay], 10, &mut rng);
        assert_eq!(picked.len(), 2);
    }

    #[test]
    fn apply_random_layout_resets_then_repopulates() {
        // End-to-end: starts from an AppState with the default
        // rack, applies a random layout, and asserts the result
        // contains exactly the persistent core + the layout's
        // voices + fx + lfos.  Catches regressions where the wipe
        // step misses a module kind or `wire_default_cables`
        // forgets a new voice (we hit that bug on the V2 voices).
        use crate::state::AppState;
        let mut s = AppState::default();
        let layout = apply_random_layout(&mut s, 0xCAFE_F00D);

        // Persistent core: sequencer + master + console.
        for kind in [
            ModuleKind::StepSequencer,
            ModuleKind::MasterOutput,
            ModuleKind::LlmConsole,
        ] {
            assert!(
                s.rack.modules.iter().any(|m| m.kind == kind),
                "core module {kind:?} missing after apply"
            );
        }
        // Voices from the layout all landed.
        for v in &layout.voices {
            assert!(
                s.rack.modules.iter().any(|m| m.kind == *v),
                "voice {v:?} missing after apply"
            );
        }
        // FX from the layout all landed.
        for f in &layout.fx {
            assert!(
                s.rack.modules.iter().any(|m| m.kind == *f),
                "fx {f:?} missing after apply"
            );
        }
        // LFO count matches.
        let lfo_in_rack = s
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .count();
        assert_eq!(lfo_in_rack as u8, layout.lfo_count);

        // Every voice reaches master through the default wiring.
        let master_id = s
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::MasterOutput)
            .map(|m| m.id)
            .expect("master always present");
        for v in &layout.voices {
            let vid = s
                .rack
                .modules
                .iter()
                .find(|m| m.kind == *v)
                .map(|m| m.id)
                .unwrap();
            assert!(
                s.rack.reaches_master(vid),
                "voice {v:?} (id {vid}) does not reach master {master_id}"
            );
        }
    }

    #[test]
    fn apply_random_layout_is_repeatable_for_same_seed() {
        // Two calls with the same seed land on bit-identical
        // racks — important so users can replay an interesting
        // patch by setting the seed via `/api/rack/random`.
        use crate::state::AppState;
        let mut a = AppState::default();
        let mut b = AppState::default();
        apply_random_layout(&mut a, 42);
        apply_random_layout(&mut b, 42);
        // Compare module kinds in order (ids may differ since
        // both racks count from their own next_id, but the kinds
        // and counts must match).
        let a_kinds: Vec<_> = a.rack.modules.iter().map(|m| m.kind).collect();
        let b_kinds: Vec<_> = b.rack.modules.iter().map(|m| m.kind).collect();
        assert_eq!(a_kinds, b_kinds);
        assert_eq!(a.rack.cables.len(), b.rack.cables.len());
    }

    #[test]
    fn lcg_seed_zero_does_not_freeze() {
        // Seed 0 must still mix into a non-zero state — naive LCGs
        // can stall on zero.  Verify the first 4 rolls aren't all
        // identical.
        let mut rng = LcgRng::new(0);
        let r0 = rng.next();
        let r1 = rng.next();
        let r2 = rng.next();
        let r3 = rng.next();
        assert!(
            !(r0 == r1 && r1 == r2 && r2 == r3),
            "LCG stuck on seed 0: {r0} {r1} {r2} {r3}"
        );
    }
}
