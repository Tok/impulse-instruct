// ─── audio/dsp/fx_trance_gate.rs ─────────────────────────────────────────────
// Pattern-driven 16-step gate synced to the sequencer clock.  Distinct
// from `FxGate` (envelope-driven sidechain ducker / noise gate): this
// one toggles wet/dry on a 16-cell step grid, each cell aligned to a
// musical sub-division of the sequencer bar.  Classic trance / EDM
// "chopped pad" effect.
//
// Pattern is a u16 bitmask — bit N = cell N's gate state.  Cell traversal
// rate is one of 1/4, 1/8, 1/16, 1/32 of a bar (sub-step resolution
// derived from BPM + sample counter so 1/32 doesn't require sequencer-
// side plumbing).  A short ramp on each cell edge keeps the gate from
// clicking when smooth > 0.

/// Number of cells in the gate pattern.  Fixed at 16 to match the
/// sequencer's standard step grid.
use super::dsp_util::MIX_BYPASS_THRESHOLD;
pub(crate) const TG_CELLS: usize = 16;

/// Rate selector encoding.  Stored as a u8 in `FxState`; each value
/// maps to a "cells-per-bar" count.  16 = one cell per sequencer step.
///
/// 0 → 1/4  (4 cells per bar — every quarter note)
/// 1 → 1/8  (8 cells per bar — every eighth)
/// 2 → 1/16 (16 cells per bar — one cell per sequencer step)
/// 3 → 1/32 (32 cells per bar — one cell per half sequencer step)
pub(crate) const TG_RATE_COUNT: u8 = 4;

fn cells_per_bar(rate: u8) -> u32 {
    match rate {
        0 => 4,
        1 => 8,
        2 => 16,
        _ => 32,
    }
}

pub(crate) struct TranceGateFx {
    /// Smoothed gate amplitude (0..1).  Tracks the target each sample
    /// via a one-pole smoother so cell edges don't click.
    gate_amp: f32,
    /// Cached gate target for the current cell — saves recomputing the
    /// bit lookup every sample.  Reset on cell-index change.
    cur_target: f32,
    /// Cell index sampled last sample.  When it differs from the
    /// freshly-computed cell, we re-latch `cur_target`.
    prev_cell: u32,
    /// Sequencer step seen last sample.  Used to detect step boundaries
    /// so we can reset the sub-step phase counter and stay tightly
    /// synced even if the audio block crosses a step.
    prev_seq_step: u32,
    /// Sample counter elapsed since the most recent sequencer step
    /// boundary.  Drives the sub-step phase for 1/32 rate (where one
    /// sequencer step contains two cells).
    samples_in_step: u32,
}

impl TranceGateFx {
    pub(crate) fn new() -> Self {
        Self {
            gate_amp: 1.0,
            cur_target: 1.0,
            prev_cell: u32::MAX,
            prev_seq_step: u32::MAX,
            samples_in_step: 0,
        }
    }

    /// `pattern`: 16-bit bitmask — bit 0 = cell 0, bit 15 = cell 15.
    /// `rate`: 0..3 selector (see `cells_per_bar`).
    /// `smooth`: 0..1 → 0.5..50 ms one-pole smoother time constant.
    /// `mix`: 0..1 wet/dry blend.  Cheap-bypass when < 0.001.
    /// `seq_step`: current sequencer step (`p.sequencer_current_step`).
    /// `bpm`: live engine BPM (`p.sequencer_bpm`).
    /// `sr`: engine sample rate.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn process(
        &mut self,
        sig: f32,
        pattern: u16,
        rate: u8,
        smooth: f32,
        mix: f32,
        seq_step: u32,
        bpm: f32,
        sr: f32,
    ) -> f32 {
        if mix < MIX_BYPASS_THRESHOLD {
            return sig;
        }

        // Track per-step sample counter so 1/32 rate (two cells per
        // sequencer step) has a phase to subdivide on.  Reset on the
        // step boundary detected by sequencer_current_step changes; the
        // counter is implicitly bounded by `samples_per_step` for any
        // realistic BPM.
        if seq_step != self.prev_seq_step {
            self.samples_in_step = 0;
            self.prev_seq_step = seq_step;
        } else {
            self.samples_in_step = self.samples_in_step.saturating_add(1);
        }

        // 16 sequencer steps = one bar.  Cells per bar follow the rate
        // selector.  cells_per_step = cells_per_bar / 16.
        //   1/4  → 0.25 cells/step (one cell every 4 steps)
        //   1/8  → 0.5  cells/step (one cell every 2 steps)
        //   1/16 → 1.0  cells/step (one cell per step)
        //   1/32 → 2.0  cells/step (two cells per step — needs sub-phase)
        let cpb = cells_per_bar(rate);
        let cell_idx = if cpb <= 16 {
            // Each cell spans one or more whole sequencer steps.  Integer
            // division gives a stable cell that doesn't twitch on sub-
            // step phase noise.
            let steps_per_cell = 16 / cpb; // 4, 2, 1
            (seq_step / steps_per_cell) % cpb
        } else {
            // 1/32: two cells per step — the second cell starts at the
            // sequencer-step's halfway point.  Compute the step-phase
            // from samples_in_step / samples_per_step.
            let samples_per_step = (sr * 60.0 / (bpm.max(1.0) * 4.0)).max(1.0);
            let half = if (self.samples_in_step as f32) >= samples_per_step * 0.5 {
                1
            } else {
                0
            };
            (((seq_step % 16) * 2) + half) % cpb
        };

        if cell_idx != self.prev_cell {
            // Cell boundary — re-latch the target from the pattern bit.
            // Cells beyond the bitmask's 16 slots wrap (only possible
            // at 1/32, where cell_idx ranges 0..32; mask down to 0..16
            // so the user's 16-cell pattern repeats twice per bar).
            let bit = ((pattern >> (cell_idx as u16 % TG_CELLS as u16)) & 1) as u8;
            self.cur_target = bit as f32;
            self.prev_cell = cell_idx;
        }

        // One-pole smoother — tau in milliseconds maps from the smooth
        // knob.  0..1 → 0.5..50 ms.  At 0.5 ms even max-rate clicks are
        // suppressed; at 50 ms the gate has an audible swell.
        let tau_ms = 0.5 + smooth.clamp(0.0, 1.0) * 49.5;
        let tau_samples = (tau_ms * 0.001 * sr).max(1.0);
        let coef = 1.0 / tau_samples;
        self.gate_amp += (self.cur_target - self.gate_amp) * coef;

        let wet = sig * self.gate_amp;
        sig * (1.0 - mix) + wet * mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = TranceGateFx::new();
        let dry = 0.5;
        let out = fx.process(dry, 0xFFFF, 2, 0.0, 0.0, 0, 120.0, 48_000.0);
        assert_eq!(out, dry, "mix=0 should bypass");
    }

    #[test]
    fn fully_open_pattern_is_transparent() {
        // pattern = all-ones, mix = 1, smooth = 0 — output should
        // converge to the input level after the smoother settles.
        let mut fx = TranceGateFx::new();
        for _ in 0..2_000 {
            fx.process(0.5, 0xFFFF, 2, 0.0, 1.0, 0, 120.0, 48_000.0);
        }
        let out = fx.process(0.5, 0xFFFF, 2, 0.0, 1.0, 0, 120.0, 48_000.0);
        assert!(
            (out - 0.5).abs() < 1e-3,
            "all-on pattern at full mix should pass dry signal (got {out})"
        );
    }

    #[test]
    fn fully_closed_pattern_silences_at_full_mix() {
        let mut fx = TranceGateFx::new();
        // First sample drops to gate target 0; after several thousand
        // samples the smoother has fully closed.
        for _ in 0..2_000 {
            fx.process(0.5, 0x0000, 2, 0.0, 1.0, 0, 120.0, 48_000.0);
        }
        let out = fx.process(0.5, 0x0000, 2, 0.0, 1.0, 0, 120.0, 48_000.0);
        assert!(
            out.abs() < 1e-3,
            "all-off pattern at full mix should silence (got {out})"
        );
    }

    #[test]
    fn alternating_pattern_responds_to_step_changes() {
        // 0xAAAA = 1010 1010 1010 1010 → odd cells active, even closed
        // at 1/16 rate.  Stepping the sequencer over a few cells should
        // visit both gate states.
        let mut fx = TranceGateFx::new();
        let mut seen_high = false;
        let mut seen_low = false;
        for step in 0..16 {
            // A few hundred samples per step to let the smoother settle.
            for _ in 0..1_000 {
                let out = fx.process(0.8, 0xAAAA, 2, 0.0, 1.0, step, 120.0, 48_000.0);
                if out.abs() > 0.5 {
                    seen_high = true;
                }
                if out.abs() < 0.05 {
                    seen_low = true;
                }
            }
        }
        assert!(
            seen_high && seen_low,
            "alternating pattern must visit both states"
        );
    }

    #[test]
    fn output_stays_bounded_under_chopping() {
        // Sine input + alternating pattern + max smooth + full mix — the
        // smoother and the mix math must keep |out| < |in| at all times.
        let mut fx = TranceGateFx::new();
        let mut peak = 0.0_f32;
        for i in 0..48_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            let step = (i / 6_000) as u32; // crude clock
            let out = fx.process(sig, 0xAAAA, 2, 1.0, 1.0, step, 120.0, 48_000.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(peak <= 0.9, "output bounded by input (peak {peak})");
    }
}
