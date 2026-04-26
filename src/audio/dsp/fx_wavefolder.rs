// ─── audio/dsp/fx_wavefolder.rs ──────────────────────────────────────────────
// West Coast wavefolder FX — when the driven input crosses ±threshold,
// the signal "folds" back on itself, multiplying harmonics.  Distinct
// from the clip / drive / saturation / waveshaper bank already in
// place: those compress the signal into a soft / hard ceiling, the
// fold instead reflects it, producing the bright, complex harmonic
// content that defines Buchla / Serge / Make Noise sources.
//
// Two fold curves are available, blended by the `symmetry` knob:
//   * Triangle fold (symmetry = 1) — sharp Serge-style folds, lots
//     of odd + even harmonics, bright transient on each fold edge.
//   * Sine fold (symmetry = 0) — Buchla-style smoother fold, more
//     even-order energy, softer attack on the fold curve.

/// Threshold the fold reflects around.  Fixed at 1.0 because audio
/// signals are conventionally normalised to ±1; the user reaches
/// further into the fold by turning DRIVE up.
const FOLD_THRESHOLD: f32 = 1.0;

pub(crate) struct WaveFolderFx;

impl WaveFolderFx {
    pub(crate) fn new() -> Self {
        Self
    }

    /// `drive`: 0..1 → 1..10× input gain.  Drive is what pushes the
    ///          signal above ±threshold; without drive the fold is
    ///          inactive and the FX passes the dry input through with
    ///          a small fold-curve colouration.
    /// `bias`:  0..1 — DC offset applied before folding (knob centre 0.5
    ///          = no offset; <0.5 / >0.5 shift the fold asymmetrically).
    ///          Asymmetric folding produces a different harmonic series
    ///          (more even-order energy when bias is off-centre).
    /// `symmetry`: 0..1 — fold curve blend.  0 = pure sine fold (Buchla),
    ///          1 = pure triangle fold (Serge).  Mid values cross-fade.
    /// `mix`:   0..1 wet/dry blend.  Cheap-bypass when < 0.001.
    pub(crate) fn process(
        &mut self,
        input: f32,
        drive: f32,
        bias: f32,
        symmetry: f32,
        mix: f32,
    ) -> f32 {
        if mix < 0.001 {
            return input;
        }

        // 1..10× input gain — folds engage somewhere past drive ≈ 0.1
        // for a typical ±0.5 input.
        let g = 1.0 + drive.clamp(0.0, 1.0) * 9.0;
        // Centre the bias knob at 0 (knob 0.5).  ±0.5 of the threshold
        // is enough to noticeably skew the fold without pushing the
        // wavefolder into a runaway DC region.
        let b = bias.clamp(0.0, 1.0) - 0.5;
        // Clamp the driven input to a sane range so a future inf input
        // (NaN guard) can't push the closed-form fold into trig territory
        // it can't return from.
        let x = (input * g + b).clamp(-50.0, 50.0);

        // ─── Triangle fold (closed form, no iterations) ───────────────
        // Maps any input to the equivalent point on a unit-period
        // triangular wave reflecting around ±FOLD_THRESHOLD.
        let n = (x + FOLD_THRESHOLD) / (2.0 * FOLD_THRESHOLD);
        let frac = n - n.floor();
        let tri = if (n.floor() as i32).rem_euclid(2) == 0 {
            frac * 2.0 * FOLD_THRESHOLD - FOLD_THRESHOLD
        } else {
            FOLD_THRESHOLD - frac * 2.0 * FOLD_THRESHOLD
        };

        // ─── Sine fold ────────────────────────────────────────────────
        // sin(x · π / (2·thresh)) · thresh — wraps inputs above
        // threshold smoothly back through zero, producing the gentler
        // Buchla-style fold curve.
        let sin_fold = (x * std::f32::consts::FRAC_PI_2 / FOLD_THRESHOLD).sin() * FOLD_THRESHOLD;

        // Blend the two fold curves.  Symmetry 0 = pure sine, 1 = pure
        // triangle.  The symmetry name nods to the fact the triangle
        // fold's edges are perfectly symmetric while the sine fold has
        // asymmetric harmonic distribution.
        let s = symmetry.clamp(0.0, 1.0);
        let folded = sin_fold * (1.0 - s) + tri * s;

        input * (1.0 - mix) + folded * mix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_when_mix_zero() {
        let mut fx = WaveFolderFx::new();
        let dry = 0.5;
        let out = fx.process(dry, 0.5, 0.5, 0.5, 0.0);
        assert_eq!(out, dry, "mix=0 should bypass");
    }

    #[test]
    fn output_bounded_by_threshold() {
        // Both fold curves are bounded by ±FOLD_THRESHOLD.  At full
        // wet + full drive, the output absolute value never exceeds
        // the threshold (1.0).
        let mut fx = WaveFolderFx::new();
        let mut peak = 0.0_f32;
        for i in 0..10_000 {
            let sig = (i as f32 * 0.05).sin() * 0.8;
            let out = fx.process(sig, 1.0, 0.5, 1.0, 1.0);
            assert!(out.is_finite());
            peak = peak.max(out.abs());
        }
        assert!(
            peak <= FOLD_THRESHOLD + 1e-3,
            "fold output bounded by threshold (peak {peak})"
        );
    }

    #[test]
    fn drive_above_threshold_produces_audible_folds() {
        // A constant 0.5 input + drive=1 (× 10 = 5) should fold
        // multiple times — output amplitude varies cycle-by-cycle as
        // a swept input traverses fold edges.  Verify the output
        // sweeps through a meaningful range rather than holding flat.
        let mut fx = WaveFolderFx::new();
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for i in 0..2_000 {
            let sig = (i as f32 * 0.01).sin() * 0.5; // slow sweep
            let out = fx.process(sig, 1.0, 0.5, 1.0, 1.0);
            min = min.min(out);
            max = max.max(out);
        }
        let range = max - min;
        assert!(
            range > 1.0,
            "high drive should sweep the fold across a wide range (got {range})"
        );
    }

    #[test]
    fn passthrough_at_zero_drive_zero_bias() {
        // Drive=0 (1× gain), bias=0.5 (no offset), pure triangle
        // fold (symmetry=1) — for a ±0.5 sine input the signal stays
        // below threshold so fold is essentially a passthrough +
        // small triangle-fold curve linearity.
        let mut fx = WaveFolderFx::new();
        let mut max_err = 0.0_f32;
        for i in 0..1_000 {
            let sig = (i as f32 * 0.05).sin() * 0.5;
            let out = fx.process(sig, 0.0, 0.5, 1.0, 1.0);
            max_err = max_err.max((out - sig).abs());
        }
        assert!(
            max_err < 0.05,
            "no drive + no bias should leave a small-signal nearly untouched (max_err {max_err})"
        );
    }

    #[test]
    fn bias_offset_changes_output() {
        // Same input + drive but different bias should produce
        // different folded outputs (asymmetric fold path).
        let mut a = WaveFolderFx::new();
        let mut b = WaveFolderFx::new();
        let mut diff = 0.0_f32;
        for i in 0..1_000 {
            let sig = (i as f32 * 0.05).sin() * 0.5;
            let out_a = a.process(sig, 0.7, 0.5, 1.0, 1.0); // bias centred
            let out_b = b.process(sig, 0.7, 0.9, 1.0, 1.0); // bias high
            diff += (out_a - out_b).abs();
        }
        assert!(
            diff > 1.0,
            "bias should perceptibly shift the fold (diff {diff})"
        );
    }
}
