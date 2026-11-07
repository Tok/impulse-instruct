// ─── state/song.rs ───────────────────────────────────────────────────────────
// Song mode — per-chain-slot overrides for the pattern chain.
//
// The existing chain is a flat `Vec<usize>` of pattern-bank indices and
// the per-pattern `pattern_style` / `pattern_bpm_apply` flags drive
// style / BPM transitions on advance.  That works for arranging
// distinct patterns but breaks down the moment a user wants the SAME
// pattern to appear twice in a song with different tempos / styles /
// repeat counts (e.g. drop repeats in half at the end).
//
// Song mode tackles that by attaching an optional `ChainSlotOverride`
// to each chain position.  The override lives alongside the chain
// (parallel `Vec<ChainSlotOverride>`) rather than inside the pattern,
// so the same bank slot can appear under different overrides.  Missing
// / absent entries fall back to the pattern's intrinsic flags — so
// existing chains keep working unchanged.

use serde::{Deserialize, Serialize};

/// Per-chain-slot override.  Any field set to `Some` wins over the
/// pattern's own `pattern_style` / `pattern_bpm_apply`; `None` means
/// "no override — use whatever the loaded pattern specifies".
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainSlotOverride {
    /// BPM to force at this chain slot.  When set, the loaded pattern's
    /// `pattern_bpm_apply` flag is ignored — this BPM is applied.
    #[serde(default)]
    pub bpm: Option<f32>,
    /// Style id to apply at this chain slot.  When set, wins over the
    /// loaded pattern's `pattern_style` — so the same pattern can drive
    /// different style transitions depending on its chain position.
    #[serde(default)]
    pub style: Option<String>,
    /// How many pattern loops this slot plays before advancing.  Default
    /// 1 (classic chain behaviour).  Clamped to `1..=64` by setters —
    /// 64 is a sane upper bound that still leaves the song responsive
    /// without burning minutes on a stuck slot.
    #[serde(default = "default_repeats")]
    pub repeats: u8,
    /// Pattern-morph length in loops when entering this slot.  `0`
    /// (the default) is the classic hard cut: the new pattern replaces
    /// the previous one instantly on advance.  `>0` schedules a
    /// step-by-step crossfade — over `morph_bars` loop boundaries,
    /// indices from the new pattern progressively replace the old
    /// using a bit-reversal dispersal order so the rhythm evolves
    /// gradually rather than swapping front-half / back-half.  See
    /// `crate::state::ChainMorph`.  Clamped to `0..=8` by setters.
    #[serde(default)]
    pub morph_bars: u8,
}

fn default_repeats() -> u8 {
    1
}

impl Default for ChainSlotOverride {
    fn default() -> Self {
        Self {
            bpm: None,
            style: None,
            repeats: default_repeats(),
            morph_bars: 0,
        }
    }
}

impl ChainSlotOverride {
    /// True when the override carries no information — purely decorative.
    /// Used to strip no-op entries from serialized song payloads.
    pub fn is_empty(&self) -> bool {
        self.bpm.is_none() && self.style.is_none() && self.repeats <= 1 && self.morph_bars == 0
    }
}
