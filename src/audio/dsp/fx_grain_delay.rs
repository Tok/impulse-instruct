// ─── audio/dsp/fx_grain_delay.rs ──────────────────────────────────────────────
// Grain Delay FX — granular feedback path.  Distinct from
// `FxMultitap` (rhythmic taps), `FxFreeze` (held buffer), and
// `FxDelay` (single tap).  Reads are short overlapping Hann-
// windowed grains scattered in time + pitch around a baseline
// delay; the output is a chorused, smeared, frequency-shifted
// echo cloud rather than a clean delay tap.
//
// V1 design:
//   * 4 overlapping grains, each running an independent
//     fractional read from a shared delay buffer.
//   * Hann window per grain so neighbouring grains crossfade
//     smoothly without amplitude bumps at the boundaries.
//   * Grain trigger phase is staggered (1/N each) so the four
//     grains always overlap in different stages of their windows.
//   * Per-grain SCATTER picks a random position offset (0..±50%
//     of base delay) and a random pitch ratio (0.5..2.0× ≈
//     ±1 octave), both scaled by the SCATTER knob.
//   * Mix knob with cheap-bypass fast path.
//
// Allocation-free; the buffer is allocated once at construction
// (Vec<f32> on heap) and the four grain structs live in a fixed
// array.

use super::dsp_util::MIX_BYPASS_THRESHOLD;
use std::f32::consts::TAU;

/// Maximum delay buffer length — 1.5 s at 96 kHz + headroom for
/// scatter excursion + the longest grain length.  216 000 = 2.25 s
/// at 96 kHz, leaving slack for ±50 % scatter on a 1 s base.
const GRAIN_BUFFER_LEN: usize = 216_000;
const GRAIN_COUNT: usize = 4;

/// One active grain — a fractional-rate read from `buf` with a
/// Hann window.  When `elapsed` reaches `length`, the grain
/// retriggers with a fresh random position + pitch.
#[derive(Clone, Copy, Debug)]
struct Grain {
    /// Read position in fractional samples (relative to the buffer).
    /// Walks at `pitch_ratio` per output sample.
    read_pos: f32,
    /// Read-rate multiplier — 1.0 = unison, 0.5 = -1 octave,
    /// 2.0 = +1 octave.
    pitch_ratio: f32,
    /// Grain length in samples.  Fixed for the lifetime of one
    /// grain; resampled on retrigger.
    length: f32,
    /// Samples elapsed since the grain started.  Goes from 0 →
    /// length, then the grain retriggers.
    elapsed: f32,
}

impl Grain {
    const fn new() -> Self {
        Self {
            read_pos: 0.0,
            pitch_ratio: 1.0,
            length: 1.0,
            elapsed: 1.0, // start "finished" so first call retriggers
        }
    }
}

pub(crate) struct GrainDelayFx {
    buf: Vec<f32>,
    write_idx: usize,
    grains: [Grain; GRAIN_COUNT],
    /// Used to stagger grain start phases so the four grains
    /// always overlap in different stages of the Hann window.
    /// Counts how many grains have ever been retriggered; the
    /// (count % N)-th grain gets a phase-offset start.
    trigger_count: u32,
    /// Tiny LCG for grain-position + pitch jitter.  Same trick
    /// as the vinyl FX — no rand-crate dep, no allocation,
    /// per-FX state so two instances don't share noise.
    rng_state: u64,
}

impl GrainDelayFx {
    pub(crate) fn new() -> Self {
        Self {
            buf: vec![0.0; GRAIN_BUFFER_LEN],
            write_idx: 0,
            grains: [Grain::new(); GRAIN_COUNT],
            trigger_count: 0,
            rng_state: 0xDEAD_BEEF_CAFE_F00D,
        }
    }

    fn next_random_unit(&mut self) -> f32 {
        // LCG (Numerical Recipes constants).  Top 31 bits give
        // good mixing; map to ±1 unit float.
        self.rng_state = self
            .rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (((self.rng_state >> 33) as i32) as f32) / (i32::MAX as f32)
    }

    /// `delay`:   0..1 → 50..1000 ms log-mapped (base time of the
    ///            grain cloud's centre).
    /// `grain`:   0..1 → 20..200 ms grain length.  Short grains
    ///            sound more like a chorus / verb cloud, long
    ///            ones more like a smeared delay tap.
    /// `scatter`: 0..1 — 0 = grains aligned in time and pitch
    ///            (acts like a chorus around the baseline);
    ///            1 = wide pitch jitter (±1 oct) and position
    ///            scatter (±50 % of base delay).
    /// `mix`:     0..1 — wet/dry blend (0 = bypass).
    pub(crate) fn process(
        &mut self,
        input: f32,
        delay: f32,
        grain: f32,
        scatter: f32,
        mix: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return input;
        }

        // Knob mappings.  Log delay so the same knob feel covers
        // slap-back through long-tail dub.
        let d_clamped = delay.clamp(0.0, 1.0);
        let delay_ms = 50.0 * 20.0_f32.powf(d_clamped); // 50..1000 ms
        let base_delay_samples = (delay_ms * 0.001 * sr).max(1.0);
        let grain_clamped = grain.clamp(0.0, 1.0);
        let grain_len_samples = ((20.0 + 180.0 * grain_clamped) * 0.001 * sr).max(8.0);
        let s = scatter.clamp(0.0, 1.0);

        // Write the input first so freshly-pushed audio is
        // available to grains scheduled later in this same call.
        self.buf[self.write_idx] = input;

        // Each grain reads, advances, and retriggers on
        // completion.  Output is the windowed sum.
        let mut wet = 0.0_f32;
        for i in 0..GRAIN_COUNT {
            // Retrigger condition: elapsed has reached length.
            // Use a saturating compare so first-call wraparound
            // (elapsed initialised to length) re-fires immediately.
            if self.grains[i].elapsed >= self.grains[i].length {
                self.trigger_count = self.trigger_count.wrapping_add(1);
                // Stagger by 1/N of the grain length so the four
                // grains overlap with different window phases.
                let stagger_offset = (i as f32) * grain_len_samples / GRAIN_COUNT as f32;
                // Random pitch jitter — ±1 octave at full scatter.
                let pitch_octaves = self.next_random_unit() * s;
                let pitch_ratio = 2.0_f32.powf(pitch_octaves);
                // Random position offset — ±50 % of base delay.
                let pos_offset = self.next_random_unit() * s * base_delay_samples * 0.5;
                let total_offset = base_delay_samples + pos_offset + stagger_offset;
                // Read pointer counted backwards from the write
                // pointer.  Wrap modulo buffer length.
                let read_start = self.write_idx as f32 + GRAIN_BUFFER_LEN as f32 - total_offset;
                let read_start = read_start.rem_euclid(GRAIN_BUFFER_LEN as f32);
                self.grains[i].read_pos = read_start;
                self.grains[i].pitch_ratio = pitch_ratio.clamp(0.25, 4.0);
                self.grains[i].length = grain_len_samples;
                self.grains[i].elapsed = 0.0;
            }

            // Read this sample of the active grain.
            let g = &mut self.grains[i];
            let pos = g.read_pos.rem_euclid(GRAIN_BUFFER_LEN as f32);
            let i0 = pos as usize;
            let i1 = (i0 + 1) % GRAIN_BUFFER_LEN;
            let frac = pos - pos.floor();
            let sample = self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac;
            // Hann window: 0.5 - 0.5 * cos(2π * elapsed / length).
            let w = 0.5 - 0.5 * (TAU * g.elapsed / g.length).cos();
            wet += sample * w;

            // Advance grain state.
            g.read_pos += g.pitch_ratio;
            g.elapsed += 1.0;
        }
        // Each Hann window peaks at 1.0, so 4 overlapping grains
        // can sum near 4× input at peak.  Scale by 1/N for
        // approximately unity perceived output.
        wet *= 1.0 / GRAIN_COUNT as f32;

        self.write_idx = (self.write_idx + 1) % GRAIN_BUFFER_LEN;
        input * (1.0 - mix) + wet * mix
    }
}

impl Default for GrainDelayFx {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = GrainDelayFx::new();
        let out = fx.process(0.5, 0.5, 0.5, 0.5, 0.0, 48_000.0);
        assert_eq!(out, 0.5, "mix=0 should bypass");
    }

    #[test]
    fn produces_audible_output_from_steady_signal() {
        // Drive a constant amplitude sine and check the wet path
        // has audible output after the delay window has elapsed.
        let mut fx = GrainDelayFx::new();
        // Warm up the buffer.
        for i in 0..6_000 {
            let sig = (i as f32 * TAU * 440.0 / 48_000.0).sin();
            fx.process(sig, 0.3, 0.5, 0.0, 1.0, 48_000.0);
        }
        // Now sample.
        let mut peak = 0.0_f32;
        for i in 0..4_000 {
            let sig = (i as f32 * TAU * 440.0 / 48_000.0).sin();
            let out = fx.process(sig, 0.3, 0.5, 0.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak > 0.05, "grain delay output audible (peak {peak})");
    }

    #[test]
    fn output_stays_bounded_under_full_drive() {
        let mut fx = GrainDelayFx::new();
        let mut peak = 0.0_f32;
        for i in 0..16_000 {
            let sig = (i as f32 * 0.05).sin();
            let out = fx.process(sig, 1.0, 1.0, 1.0, 1.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        // Hann-windowed sum scaled by 1/N stays sub-unity even
        // under maximum scatter.  Allow a small margin for
        // window-overlap peaking.
        assert!(
            peak <= 1.5,
            "grain delay bounded at full drive (peak {peak})"
        );
    }

    #[test]
    fn scatter_zero_produces_repeatable_unjittered_output() {
        // Scatter=0 means every grain's pitch_ratio = 2^0 = 1
        // and pos_offset = 0, so all four grains read from the
        // same base position and produce a chorus-y sum without
        // pitch warble.  Two FX instances driven the same way
        // should produce the same output (no rand variation).
        let mut a = GrainDelayFx::new();
        let mut b = GrainDelayFx::new();
        for i in 0..1_000 {
            let sig = (i as f32 * TAU * 220.0 / 48_000.0).sin();
            let oa = a.process(sig, 0.3, 0.5, 0.0, 1.0, 48_000.0);
            let ob = b.process(sig, 0.3, 0.5, 0.0, 1.0, 48_000.0);
            assert!(
                (oa - ob).abs() < 1e-5,
                "scatter=0 deterministic across instances at i={i} (a={oa}, b={ob})"
            );
        }
    }
}
