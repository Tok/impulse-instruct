// ─── state/modal.rs ───────────────────────────────────────────────────────────
// Modal / struck-physical-model voice — 8 parallel resonant
// biquads excited by a short LP-filtered noise burst on each
// trigger.  The "ratio preset" picks the harmonic relationship
// between the modes — integer multiples for harmonic/string-like
// tones, the inharmonic ratios that characterise bells / tubular
// chimes / metal bars for the more distinctive percussion
// timbres.
//
// Each mode's per-sample decay is derived from a single
// `decay_scale` knob (longer = more bell-like, shorter = more
// like a damped wood block).  Higher modes always die faster
// than the fundamental, which is what makes a real struck
// resonator sound recognisable — the bright "ping" attack
// quickly settles into the warmer fundamental.

use serde::{Deserialize, Serialize};

/// Number of modes the resonator bank exposes.  8 is enough for
/// most struck-percussion timbres (most idealised bells / chimes
/// have their identifying character in the first 6-8 modes); 16
/// would only deepen the metallic shimmer of long decays at extra
/// per-sample cost.
pub const MODAL_MODES: usize = 8;

/// Number of ratio presets — must match the table in
/// `audio/dsp/modal.rs::RATIO_PRESETS`.
pub const MODAL_RATIO_PRESETS: u8 = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModalState {
    pub enabled: bool,
    pub volume: f32,
    #[serde(default)]
    pub pan: f32,
    /// Per-mode level 0..1 — drawable on the panel as the
    /// "spectrum" of the resonator bank.  Values are normalised
    /// at process time so a fully-pegged bank stays bounded.
    pub levels: [f32; MODAL_MODES],
    /// Excitation brightness 0..1 → LP cutoff of the noise burst
    /// that feeds the resonators on trigger.  Low = woody / soft
    /// mallet hit; high = bright / metallic stick hit.
    pub brightness: f32,
    /// Global decay-time scale 0..1 → maps to ~5 ms (very damped)
    /// up to ~5 s ring on the fundamental.  Each higher mode
    /// dies ~30% faster per index step, baked into the DSP.
    pub decay_scale: f32,
    /// Ratio preset 0..=3.  Picks the harmonic relationship
    /// between modes:
    ///   0 — Harmonic: integer multiples (1, 2, 3, …, 8) —
    ///       string- / pluck-like tones.
    ///   1 — Bell: idealised church bell (1, 2.76, 5.4, …) —
    ///       distinctly inharmonic with a strong "hum tone" feel.
    ///   2 — Tubular: idealised tubular chime — narrower
    ///       inharmonic spread than the bell.
    ///   3 — Metal: idealised metal bar (marimba-like + glassy
    ///       overtones).
    /// Clamped at apply time so out-of-range values from the LLM
    /// never blow past the table.
    #[serde(default)]
    pub ratio_preset: u8,
}

impl Default for ModalState {
    fn default() -> Self {
        // Default profile: bell-like — the most distinctive of
        // the four presets, and the most musically interesting
        // straight out of the box.  Per-mode levels follow a
        // standard "low / hum / quint / nominal / minor third"
        // bell-spectrum shape; user can redraw on the histogram.
        let mut levels = [0.0_f32; MODAL_MODES];
        levels[0] = 1.0; // strike tone
        levels[1] = 0.7; // hum
        levels[2] = 0.55; // prime
        levels[3] = 0.4; // tierce
        levels[4] = 0.28;
        levels[5] = 0.2;
        levels[6] = 0.15;
        levels[7] = 0.1;
        Self {
            enabled: false,
            volume: 0.7,
            pan: 0.0,
            levels,
            brightness: 0.6,
            decay_scale: 0.6,
            ratio_preset: 1, // Bell
        }
    }
}
