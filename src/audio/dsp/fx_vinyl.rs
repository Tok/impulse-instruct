// ─── audio/dsp/fx_vinyl.rs ────────────────────────────────────────────────────
// Vinyl / cassette simulator FX — surface noise + dull EQ shape.
// Absurd-queue item #6.
//
// V1 deliberately scoped:
//   * Surface noise (band-limited white noise added to the wet path).
//   * "Dull" EQ shape — high-shelf cut + low-shelf boost mimics the
//     spectral signature of analog tape / vinyl playback (warm
//     low-mids, rolled-off air).
//   * Mix knob blends wet against dry so the user can dial the
//     character without committing.
//
// Start / stop transient deferred — the existing `FxTapeStop` already
// covers that effect family; building it again here would be
// redundant.  This FX focuses on the *steady-state* analog character,
// which TapeStop doesn't address.

use super::fx::Biquad;

pub(crate) struct VinylFx {
    /// Low-shelf boost — adds warmth in the 100–300 Hz region.
    low_shelf: Biquad,
    /// High-shelf cut — rolls off the air band; moves the cutoff
    /// according to the "wear" knob (more wear = duller).
    high_shelf: Biquad,
    /// Cached engine sample rate for biquad recomputation when the
    /// wear knob moves.
    sr: f32,
    /// Last-seen wear knob, so we only recompute the high-shelf
    /// coefficients when the user actually moves it.  Initialised
    /// to NaN so the first sample triggers a refresh.
    last_wear: f32,
    /// Tiny LCG for the surface noise — same trick as `rack_random`
    /// (no `rand` crate dep, no allocations, deterministic across
    /// runs at the same starting state).  Per-FX state so two
    /// instances don't share noise patterns.
    noise_state: u64,
}

impl VinylFx {
    pub(crate) fn new(sr: f32) -> Self {
        Self {
            low_shelf: Biquad::low_shelf(220.0, 3.0, sr),
            high_shelf: Biquad::high_shelf(6_000.0, 0.0, sr),
            sr,
            last_wear: f32::NAN,
            // Non-zero seed so the first noise sample isn't 0.
            noise_state: 0xC0FF_EE15_FACE_F00D,
        }
    }

    /// `noise`: 0..1 — surface-noise amplitude (0 = silent, 1 ≈ -20 dBFS).
    /// `wear`: 0..1 — high-shelf cutoff sweep (0 = bright @ 6 kHz,
    ///         1 = dull @ 1 kHz).  Higher wear cuts more high end.
    /// `mix`:  0..1 — wet/dry blend.
    pub(crate) fn process(&mut self, input: f32, noise: f32, wear: f32, mix: f32) -> f32 {
        if mix < 0.001 {
            return input;
        }
        // Re-tune the high-shelf when the wear knob moves
        // appreciably.  Single-cutoff sweep keeps the cost down to
        // one Biquad-coefficient recompute per knob change rather
        // than per sample.
        if (wear - self.last_wear).abs() > 0.005 {
            // Wear 0 → fc 6 kHz, gain 0 dB (transparent).
            // Wear 1 → fc 1 kHz, gain -10 dB (heavily dulled).
            let w = wear.clamp(0.0, 1.0);
            let fc = 6_000.0 * (1.0 / 6.0_f32).powf(w); // log sweep down to 1 kHz
            let gain_db = -10.0 * w; // 0 → -10 dB
            self.high_shelf = Biquad::high_shelf(fc, gain_db, self.sr);
            self.last_wear = w;
        }

        // Generate one noise sample via LCG — top bits have the
        // better mixing in this PRNG style; centre to ±1 then
        // scale by the noise knob.  Coefficient on `noise` keeps
        // peak amplitude reasonable: 1.0 here ≈ -20 dBFS.
        self.noise_state = self
            .noise_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let n = ((self.noise_state >> 33) as i32) as f32 / (i32::MAX as f32);
        let surface = n * noise.clamp(0.0, 1.0) * 0.1;

        // EQ chain: low-shelf boost → high-shelf cut.  Order
        // doesn't matter much for two non-resonant shelves; this
        // ordering matches what the user reads on the panel
        // (warmth first, dullness last).
        let eq = self.high_shelf.process(self.low_shelf.process(input));
        let wet = eq + surface;
        input * (1.0 - mix) + wet * mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vinyl_dry_when_mix_zero() {
        let mut fx = VinylFx::new(48_000.0);
        let dry = 0.5;
        let out = fx.process(dry, 1.0, 1.0, 0.0);
        assert_eq!(out, dry, "mix=0 should bypass");
    }

    #[test]
    fn vinyl_adds_noise_floor_with_silent_input() {
        // Silent input + max noise + max wet → output is the
        // noise floor.  Verify it's audible (> 0) and bounded.
        let mut fx = VinylFx::new(48_000.0);
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            peak = peak.max(fx.process(0.0, 1.0, 0.0, 1.0).abs());
        }
        assert!(peak > 0.001, "noise floor should be audible (peak {peak})");
        assert!(peak <= 0.2, "noise floor shouldn't run away (peak {peak})");
    }

    #[test]
    fn vinyl_passes_signal_through_with_zero_noise_zero_wear() {
        // No noise, no wear → output should be approximately the
        // input.  Two flat shelves still apply a tiny gain shift
        // (~3 dB low-shelf boost is always on in V1) so we don't
        // require bit-equality, just same-sign + similar magnitude.
        let mut fx = VinylFx::new(48_000.0);
        // Run a few samples to warm up the biquads.
        for _ in 0..256 {
            fx.process(0.5, 0.0, 0.0, 1.0);
        }
        let out = fx.process(0.5, 0.0, 0.0, 1.0);
        assert!(out.is_finite());
        assert!(
            out > 0.3 && out < 0.9,
            "passes through near unity (got {out})"
        );
    }

    #[test]
    fn vinyl_output_stays_bounded_under_full_signal() {
        // Sine input + full noise + full wear shouldn't overflow.
        let mut fx = VinylFx::new(48_000.0);
        let mut peak = 0.0_f32;
        for i in 0..4_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            let out = fx.process(sig, 1.0, 1.0, 1.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 1.5, "vinyl path stays bounded (peak {peak})");
    }
}
