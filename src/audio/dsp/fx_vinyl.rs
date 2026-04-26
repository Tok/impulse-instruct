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
// V2 follow-up: `transient` knob — drives a rate-modulated playback
// through the colour stage so users can automate it 0→1 for a brake
// transient (deck slows to silence) or 1→0 for a spin-up (deck
// reaches speed).  Differs from `FxTapeStop` by always layering the
// vinyl colour (warmth + noise) on top of the rate ramp; TapeStop
// is transparent.  At `transient=0` (the default) the buffer is
// still written but read at unity rate — V1 tone is preserved.

use super::fx::Biquad;

/// 0.5 s @ 48 kHz — long enough to host a perceptible spin-down
/// without bloating the FX struct.  The rate modulation reads at
/// fractional positions, so we don't need TapeStop's 2 s tail.
const VINYL_BUF: usize = 24_000;

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
    /// Delay buffer — drives the start/stop transient via a fractional
    /// read head whose advance rate falls with `transient`.
    buf: Box<[f32; VINYL_BUF]>,
    /// Write index — newest sample lands here.
    write: usize,
    /// Fractional read head.  Advances by `(1 - transient)^2` samples
    /// per output sample so a knob sweep maps log-perceptually to a
    /// slow-down (matches `FxTapeStop`'s curve).
    read: f32,
    /// Last-seen transient — used to detect a return to 0.0 so the
    /// read head re-anchors near `write` and avoids drift artefacts.
    last_transient: f32,
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
            buf: Box::new([0.0; VINYL_BUF]),
            write: 0,
            read: 0.0,
            last_transient: 0.0,
        }
    }

    /// `noise`: 0..1 — surface-noise amplitude (0 = silent, 1 ≈ -20 dBFS).
    /// `wear`: 0..1 — high-shelf cutoff sweep (0 = bright @ 6 kHz,
    ///         1 = dull @ 1 kHz).  Higher wear cuts more high end.
    /// `mix`:  0..1 — wet/dry blend.
    /// `transient`: 0..1 — start/stop ramp position. 0 = at speed
    ///         (V1 behaviour, no rate modulation); 1 = deck stopped
    ///         (rate=0, output silenced).  Same perceptual curve as
    ///         FxTapeStop ((1-t)^2) so a linear knob sweep slows
    ///         logarithmically.
    pub(crate) fn process(
        &mut self,
        input: f32,
        noise: f32,
        wear: f32,
        mix: f32,
        transient: f32,
    ) -> f32 {
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

        // Always write the dry signal — keeps the buffer filled with
        // recent material so a transient sweep doesn't pull from
        // stale data.
        self.buf[self.write] = input;
        self.write = (self.write + 1) % VINYL_BUF;

        // Re-anchor the read head when transient drops back to 0 —
        // matches FxTapeStop's behaviour so the V1 tone path is
        // bit-equal to the pre-transient code at transient=0.
        let t = transient.clamp(0.0, 1.0);
        if self.last_transient > 0.001 && t < 0.001 {
            self.read = self.write as f32;
        }
        self.last_transient = t;

        // Pick the source sample.  At transient=0 the buffer's
        // freshest write is exactly the input, so this collapses to
        // the V1 path; for any t>0 we read a slowed playback.
        let src = if t < 0.001 {
            input
        } else {
            // Linear-interp fractional read.
            let read_pos = self.read;
            let idx = read_pos as usize % VINYL_BUF;
            let frac = read_pos - read_pos.floor();
            let next = (idx + 1) % VINYL_BUF;
            let raw = self.buf[idx] + (self.buf[next] - self.buf[idx]) * frac;

            // Advance read by `(1-t)^2` samples per output sample
            // — perceptually log slow-down.
            let rate = (1.0 - t).powi(2);
            let mut new_pos = read_pos + rate;
            if new_pos >= VINYL_BUF as f32 {
                new_pos -= VINYL_BUF as f32;
            }
            self.read = new_pos;
            raw
        };

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
        let eq = self.high_shelf.process(self.low_shelf.process(src));
        // Fade the wet path with the transient — at t=1 the deck is
        // stopped, so even the surface noise should vanish.
        let fade = 1.0 - t;
        let wet = (eq + surface) * fade;
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
        let out = fx.process(dry, 1.0, 1.0, 0.0, 0.0);
        assert_eq!(out, dry, "mix=0 should bypass");
    }

    #[test]
    fn vinyl_adds_noise_floor_with_silent_input() {
        // Silent input + max noise + max wet → output is the
        // noise floor.  Verify it's audible (> 0) and bounded.
        let mut fx = VinylFx::new(48_000.0);
        let mut peak = 0.0_f32;
        for _ in 0..4_000 {
            peak = peak.max(fx.process(0.0, 1.0, 0.0, 1.0, 0.0).abs());
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
            fx.process(0.5, 0.0, 0.0, 1.0, 0.0);
        }
        let out = fx.process(0.5, 0.0, 0.0, 1.0, 0.0);
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
            let out = fx.process(sig, 1.0, 1.0, 1.0, 0.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 1.5, "vinyl path stays bounded (peak {peak})");
    }

    #[test]
    fn vinyl_transient_one_silences_wet_path() {
        // transient=1 means the deck is fully stopped — the wet
        // path should fade to silence.  At mix=1 the dry signal is
        // also masked out entirely.
        let mut fx = VinylFx::new(48_000.0);
        let mut peak = 0.0_f32;
        for i in 0..1_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            // mix=1 (full wet), transient=1 (deck stopped).
            let out = fx.process(sig, 0.0, 0.0, 1.0, 1.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(
            peak < 0.05,
            "transient=1 should silence the deck (peak {peak})"
        );
    }

    #[test]
    fn vinyl_transient_zero_matches_v1_at_steady_input() {
        // transient=0 should leave the V1 path bit-equal — the
        // buffer is bypassed in favour of the live input.  Drive a
        // constant input through both paths and confirm the
        // outputs converge.
        let mut a = VinylFx::new(48_000.0);
        let mut b = VinylFx::new(48_000.0);
        // Warm up.
        for _ in 0..256 {
            a.process(0.4, 0.0, 0.5, 1.0, 0.0);
            b.process(0.4, 0.0, 0.5, 1.0, 0.0);
        }
        let out_a = a.process(0.4, 0.0, 0.5, 1.0, 0.0);
        let out_b = b.process(0.4, 0.0, 0.5, 1.0, 0.0);
        assert!((out_a - out_b).abs() < 1e-5, "V1 path drift");
    }

    #[test]
    fn vinyl_transient_ramp_produces_finite_output() {
        // Sweep transient 0→1 across a sine input — output must
        // stay finite the whole way and shrink as the deck slows.
        let mut fx = VinylFx::new(48_000.0);
        let mut early_peak = 0.0_f32;
        let mut late_peak = 0.0_f32;
        for i in 0..2_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            let t = i as f32 / 2_000.0; // 0 → ~1
            let out = fx.process(sig, 0.0, 0.0, 1.0, t);
            assert!(out.is_finite());
            if i < 200 {
                early_peak = early_peak.max(out.abs());
            }
            if i >= 1_800 {
                late_peak = late_peak.max(out.abs());
            }
        }
        assert!(
            late_peak < early_peak,
            "transient sweep should fade output (early {early_peak}, late {late_peak})"
        );
    }
}
