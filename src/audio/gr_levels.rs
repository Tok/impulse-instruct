// ─── audio/gr_levels.rs ──────────────────────────────────────────────────────
// Shared atomic gain-reduction snapshot for the `GrHistory` viz module.
// Mirrors the `voice_meters::VoiceLevels` pattern — Arc<...> with an
// AtomicU32 holding f32::to_bits — but stores a single value: the
// momentary peak-decay envelope of the most-attenuating dynamics
// processor in the FX chain (FxCompressor / FxLimiter / FxMultibandComp).
//
// Convention: stored value is the linear gain ratio (0..=1, 1.0 = no
// reduction), not dB.  UI converts to dB (`20·log10`) when painting,
// so the wire format stays unambiguous regardless of how the consumer
// wants to display it.  Gain reductions ≥ -inf dB collapse to 0.0
// (full attenuation).
//
// Audio thread: DspState computes a per-step pre/post amplitude ratio
// inside `apply_fx_chain`, takes the *minimum* (most attenuating) ratio
// across the three dynamics steps within each sample, decays the
// envelope toward 1.0 (no reduction) with a ~200 ms release, and
// publishes once per audio callback.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct GrLevels {
    /// Linear gain ratio (0..=1) stored as `f32::to_bits()`.  1.0 = no
    /// reduction; 0.5 ≈ -6 dB; 0.1 ≈ -20 dB.  Default 1.0 (no GR).
    pub level: AtomicU32,
}

impl GrLevels {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            level: AtomicU32::new(1.0_f32.to_bits()),
        })
    }

    /// Read the current linear gain ratio.  Returns 1.0 (no GR) when
    /// unset.
    pub fn read(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Write a fresh linear gain ratio.
    pub fn write(&self, level: f32) {
        self.level.store(level.to_bits(), Ordering::Relaxed);
    }
}

/// Convert a linear gain ratio (0..=1) to dB attenuation (≤ 0).  Used
/// by the UI when painting the GR trace.  Floor at -60 dB so a fully
/// silenced ratio doesn't return -inf and break the y-axis mapping.
pub fn linear_to_gr_db(linear: f32) -> f32 {
    if linear <= 1e-6 {
        -60.0
    } else {
        crate::audio::dsp::lin_to_db(linear).clamp(-60.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_f32_bits() {
        let gr = GrLevels::new();
        gr.write(0.42);
        assert!((gr.read() - 0.42).abs() < 1e-6);
    }

    #[test]
    fn default_is_unity() {
        // Fresh GrLevels reads as 1.0 (no reduction) so a viz that
        // hasn't seen the audio thread yet doesn't paint a phantom
        // attenuation.
        let gr = GrLevels::new();
        assert!((gr.read() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn linear_to_gr_db_unity_is_zero() {
        assert!((linear_to_gr_db(1.0) - 0.0).abs() < 1e-3);
    }

    #[test]
    fn linear_to_gr_db_half_is_minus_six_db() {
        // 20·log10(0.5) ≈ -6.02 dB.
        assert!((linear_to_gr_db(0.5) - (-6.0)).abs() < 0.05);
    }

    #[test]
    fn linear_to_gr_db_floors_at_minus_sixty() {
        // Avoid -inf when the ratio is essentially zero — UI relies on
        // a finite floor for its y-axis.
        assert_eq!(linear_to_gr_db(0.0), -60.0);
        assert_eq!(linear_to_gr_db(-0.5), -60.0); // negative input → floor
    }
}
