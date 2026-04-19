// ─── sequencer/preecho.rs ────────────────────────────────────────────────────
// Pre-echo pattern modulator — KeyKit-inspired reverse-echo lead-in.
//
// Semantics (per the expert's framing): the user declares a set of
// "anchor" step indices that form a pulse/groove signature.  The steps
// leading up to each anchor get shaped into a build-up — velocity ramp,
// ratchet build, probability ramp, accent ramp, slide cascade — so the
// pattern feels like it's "reaching for" the anchor.  Wrap-around is
// intentional: the tail of the bar leads into the downbeat at step 0.
//
// v2 additions on top of v1:
//   • curve shapes (linear / exp / log / cosine) for every scalar ramp
//   • probability ramp (drums and any voice with a stored probability)
//   • auto-length: length is derived from the gap to the prior anchor,
//     so uneven anchor spacings produce variable-length build-ups
//     without per-anchor config
//   • unified `preecho_apply` returning a single `PreechoApply` struct
//     instead of the v1 `(f32, u8)` / `(Option<f32>, Option<f32>)` pair
//     — one entry point for drum, bass, and future hoover/an1x callers

use serde::{Deserialize, Serialize};

/// Ramp-curve shape.  Applied to the normalised position `pos ∈ [0, 1]`
/// before it's scaled into velocity / accent / probability / ratchet
/// space.  Controls the "feel" of the build-up — is it a late rush, an
/// early bloom, or an ease-in-out sweep?
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RampCurve {
    /// `y = pos` — uniform ramp, v1 behaviour.
    #[default]
    Linear,
    /// `y = pos²` — slow start, late rush ("loading, then launch").
    Exp,
    /// `y = 1 − (1 − pos)²` — fast start, plateau ("opens up immediately").
    Log,
    /// `y = pos² · (3 − 2·pos)` (smoothstep) — ease in/out, cinematic.
    Cosine,
}

impl RampCurve {
    /// Shape a linear position into the curve's value.  Input is clamped
    /// to `[0, 1]`; output is within the same range.
    pub fn apply(self, pos: f32) -> f32 {
        let p = pos.clamp(0.0, 1.0);
        match self {
            Self::Linear => p,
            Self::Exp => p * p,
            Self::Log => 1.0 - (1.0 - p) * (1.0 - p),
            Self::Cosine => p * p * (3.0 - 2.0 * p),
        }
    }
}

/// Per-voice pre-echo configuration.  Empty `anchors`, `length == 0`
/// (and `!auto_length`), or `enabled == false` disables the effect
/// entirely.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreechoConfig {
    /// Master enable — a single toggle to bypass the modulator without
    /// clearing anchors or settings.  Defaults to true so existing
    /// configs keep working and freshly-created ones are armed as soon
    /// as anchors + length are set.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Step indices that anchor the groove.  Out-of-range entries (≥
    /// pattern steps) are ignored at apply time.
    #[serde(default)]
    pub anchors: Vec<u8>,
    /// Number of lead-in steps before each anchor.  0 = disabled unless
    /// `auto_length` is true.  1..=16 is the useful range.
    #[serde(default)]
    pub length: u8,
    /// When true, the lead-in window for each anchor is `gap_to_prev − 1`
    /// where `gap_to_prev` is the forward-distance to the previous anchor
    /// (wrap-aware).  The `length` field is ignored.  Useful for patterns
    /// with uneven anchor spacings — each anchor gets a build-up that
    /// exactly fills the space between it and the one before it, without
    /// extending into the prior anchor's step.
    #[serde(default)]
    pub auto_length: bool,
    /// Shape of the ramp.  Applied to every scalar ramp (velocity /
    /// ratchet / probability / accent).  Default `Linear` keeps v1
    /// behaviour.
    #[serde(default)]
    pub curve: RampCurve,
    /// When true, scale velocity 0.3 → 1.0 across the lead-in under the
    /// configured curve.
    #[serde(default)]
    pub velocity_ramp: bool,
    /// When true, add 0 → 3 to ratchet count across the lead-in
    /// (saturated at the u8-max in downstream code, but practically
    /// clamps to the sequencer's ratchet range).
    #[serde(default)]
    pub ratchet_ramp: bool,
    /// When true, override the step's stored probability with a
    /// 0.3 → 1.0 ramp so early lead-in steps fire less often, building
    /// up density toward the anchor.  Voices without a probability
    /// field (bass / hoover / an1x) ignore this.
    #[serde(default)]
    pub probability_ramp: bool,
    /// Melodic counterpart of `velocity_ramp`: ramp `TB303Step.accent`
    /// from 0.3 (earliest lead-in step) → 1.0 (anchor-adjacent) on
    /// bass voices.  Overrides the step's stored accent while inside
    /// the lead-in window; anchor step itself plays at user intensity.
    #[serde(default)]
    pub accent_ramp: bool,
    /// Set `TB303Step.slide` to 1.0 on the step immediately before each
    /// anchor (`d == 1`) so the note slides into the downbeat.  No
    /// effect on other lead-in steps — just the last one cascades.
    #[serde(default)]
    pub slide_cascade: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for PreechoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            anchors: Vec::new(),
            length: 0,
            auto_length: false,
            curve: RampCurve::Linear,
            velocity_ramp: false,
            ratchet_ramp: false,
            probability_ramp: false,
            accent_ramp: false,
            slide_cascade: false,
        }
    }
}

impl PreechoConfig {
    /// Preecho is active if enabled, anchors are non-empty, and a
    /// window exists — either explicit `length > 0` or `auto_length`.
    pub fn is_active(&self) -> bool {
        self.enabled && !self.anchors.is_empty() && (self.length > 0 || self.auto_length)
    }
}

/// Resolved modulation for a single step.  `preecho_apply` returns one
/// of these every call; callers pick the fields relevant to their voice
/// type (drums read velocity_mul / ratchet_add / probability_override;
/// bass reads accent_override / slide_override / probability_override).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreechoApply {
    /// Multiplier on the step's stored velocity (1.0 = no-op).
    pub velocity_mul: f32,
    /// Additive bump to the step's stored ratchet count (0 = no-op).
    pub ratchet_add: u8,
    /// When `Some`, replaces the step's stored probability instead of
    /// multiplying (so v2 can fire leading steps at a deliberate
    /// density without caring what the user set).
    pub probability_override: Option<f32>,
    /// When `Some`, replaces `TB303Step.accent` (bass / future hoover /
    /// future an1x).  `None` leaves the step's stored accent alone.
    pub accent_override: Option<f32>,
    /// When `Some`, replaces `TB303Step.slide`.  `None` leaves the
    /// step's stored slide alone.
    pub slide_override: Option<f32>,
}

impl PreechoApply {
    /// Sentinel used when preecho is inactive or the step is outside
    /// every lead-in window.  Every field is a no-op.
    pub const IDENTITY: Self = Self {
        velocity_mul: 1.0,
        ratchet_add: 0,
        probability_override: None,
        accent_override: None,
        slide_override: None,
    };
}

impl Default for PreechoApply {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Compute the effective lead-in length for the anchor at `anchor_idx`
/// within `cfg.anchors`.  In auto-length mode this is `gap_to_prev − 1`
/// (wrap-aware); otherwise the configured `length`.
fn effective_length(cfg: &PreechoConfig, anchor_idx: usize, pattern_len: usize) -> u8 {
    if !cfg.auto_length {
        return cfg.length;
    }
    let anchors = &cfg.anchors;
    if anchors.len() < 2 {
        // Single anchor + auto: fall back to user `length` or a sensible
        // default so the feature is never silent.
        return cfg.length.max(4);
    }
    let cur = anchors[anchor_idx] as usize;
    let prev_idx = if anchor_idx == 0 {
        anchors.len() - 1
    } else {
        anchor_idx - 1
    };
    let prev = anchors[prev_idx] as usize;
    let gap = if cur > prev {
        cur - prev
    } else {
        pattern_len + cur - prev
    };
    gap.saturating_sub(1).min(16) as u8
}

/// Resolve this step's pre-echo modulation against `cfg`.  Walks every
/// anchor, picks the closest one whose lead-in window contains `step`,
/// and returns a `PreechoApply` populated per the enabled ramp toggles.
/// Steps that sit on an anchor or outside every window get `IDENTITY`.
pub fn preecho_apply(step: usize, total_steps: usize, cfg: &PreechoConfig) -> PreechoApply {
    if !cfg.is_active() || total_steps == 0 {
        return PreechoApply::IDENTITY;
    }
    let mut best: Option<(usize, u8)> = None; // (distance, effective length)
    for (ai, &a) in cfg.anchors.iter().enumerate() {
        let a = a as usize;
        if a >= total_steps {
            continue;
        }
        let d = (a + total_steps - step) % total_steps;
        if d == 0 {
            return PreechoApply::IDENTITY;
        }
        let eff_len = effective_length(cfg, ai, total_steps);
        if eff_len == 0 || d > eff_len as usize {
            continue;
        }
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, eff_len));
        }
    }
    let Some((d, eff_len)) = best else {
        return PreechoApply::IDENTITY;
    };

    // d == 1 is the closest lead-in step (strongest); d == eff_len is
    // the earliest (weakest).  Position 0..=1 where 1 = at the anchor.
    let pos = 1.0 - ((d - 1) as f32 / eff_len.max(1) as f32);
    let curved = cfg.curve.apply(pos);

    let velocity_mul = if cfg.velocity_ramp {
        0.3 + 0.7 * curved
    } else {
        1.0
    };
    let ratchet_add = if cfg.ratchet_ramp {
        (curved * 3.0).round() as u8
    } else {
        0
    };
    let probability_override = if cfg.probability_ramp {
        Some((0.3 + 0.7 * curved).clamp(0.0, 1.0))
    } else {
        None
    };
    let accent_override = if cfg.accent_ramp {
        Some((0.3 + 0.7 * curved).clamp(0.0, 1.0))
    } else {
        None
    };
    let slide_override = if cfg.slide_cascade && d == 1 {
        Some(1.0)
    } else {
        None
    };

    PreechoApply {
        velocity_mul,
        ratchet_add,
        probability_override,
        accent_override,
        slide_override,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(anchors: &[u8], length: u8, vr: bool, rr: bool) -> PreechoConfig {
        PreechoConfig {
            enabled: true,
            anchors: anchors.to_vec(),
            length,
            auto_length: false,
            curve: RampCurve::Linear,
            velocity_ramp: vr,
            ratchet_ramp: rr,
            probability_ramp: false,
            accent_ramp: false,
            slide_cascade: false,
        }
    }

    fn cfg_melodic(anchors: &[u8], length: u8, ar: bool, sc: bool) -> PreechoConfig {
        PreechoConfig {
            enabled: true,
            anchors: anchors.to_vec(),
            length,
            auto_length: false,
            curve: RampCurve::Linear,
            velocity_ramp: false,
            ratchet_ramp: false,
            probability_ramp: false,
            accent_ramp: ar,
            slide_cascade: sc,
        }
    }

    #[test]
    fn inactive_returns_identity() {
        assert_eq!(
            preecho_apply(5, 16, &cfg(&[], 4, true, true)),
            PreechoApply::IDENTITY
        );
        assert_eq!(
            preecho_apply(5, 16, &cfg(&[0], 0, true, true)),
            PreechoApply::IDENTITY
        );
    }

    #[test]
    fn anchor_step_is_not_scaled() {
        let c = cfg(&[4], 3, true, true);
        assert_eq!(preecho_apply(4, 16, &c), PreechoApply::IDENTITY);
    }

    #[test]
    fn outside_leadin_is_identity() {
        let c = cfg(&[8], 3, true, true);
        assert_eq!(preecho_apply(4, 16, &c), PreechoApply::IDENTITY);
    }

    #[test]
    fn velocity_ramp_last_step_near_full() {
        let c = cfg(&[8], 4, true, false);
        let r = preecho_apply(7, 16, &c);
        assert!((r.velocity_mul - 1.0).abs() < 1e-3);
        assert_eq!(r.ratchet_add, 0);
    }

    #[test]
    fn velocity_ramp_earliest_step_is_weakest() {
        let c = cfg(&[8], 4, true, false);
        // d=4 → pos = 1 - 3/4 = 0.25 → vel = 0.3 + 0.7*0.25 = 0.475
        let r = preecho_apply(4, 16, &c);
        assert!(
            (r.velocity_mul - 0.475).abs() < 1e-3,
            "expected ~0.475, got {}",
            r.velocity_mul
        );
    }

    #[test]
    fn ratchet_ramp_builds() {
        let c = cfg(&[8], 4, false, true);
        assert_eq!(preecho_apply(7, 16, &c).ratchet_add, 3);
        assert_eq!(preecho_apply(4, 16, &c).ratchet_add, 1);
    }

    #[test]
    fn wraps_around_pattern_end() {
        let c = cfg(&[0], 4, true, false);
        let r = preecho_apply(14, 16, &c);
        assert!(r.velocity_mul > 0.3 && r.velocity_mul < 1.0);
    }

    #[test]
    fn nearest_anchor_wins_when_multiple() {
        let c = cfg(&[4, 12], 3, true, false);
        let r = preecho_apply(10, 16, &c);
        assert!(r.velocity_mul > 0.5);
        assert_eq!(preecho_apply(6, 16, &c), PreechoApply::IDENTITY);
    }

    // ── Melodic ──────────────────────────────────────────────────────────

    #[test]
    fn melodic_inactive_returns_none_none() {
        let c = cfg_melodic(&[], 4, true, true);
        let r = preecho_apply(5, 16, &c);
        assert_eq!(r.accent_override, None);
        assert_eq!(r.slide_override, None);
    }

    #[test]
    fn melodic_accent_ramp_last_step_is_full() {
        let c = cfg_melodic(&[8], 4, true, false);
        let r = preecho_apply(7, 16, &c);
        assert!((r.accent_override.unwrap() - 1.0).abs() < 1e-4);
        assert_eq!(r.slide_override, None);
    }

    #[test]
    fn melodic_slide_cascade_only_on_anchor_adjacent() {
        let c = cfg_melodic(&[8], 4, false, true);
        assert_eq!(preecho_apply(7, 16, &c).slide_override, Some(1.0));
        assert_eq!(preecho_apply(6, 16, &c).slide_override, None);
    }

    // ── v2: curves ───────────────────────────────────────────────────────

    #[test]
    fn ramp_curve_shapes_pos_input() {
        assert!((RampCurve::Linear.apply(0.5) - 0.5).abs() < 1e-4);
        assert!((RampCurve::Exp.apply(0.5) - 0.25).abs() < 1e-4);
        assert!((RampCurve::Log.apply(0.5) - 0.75).abs() < 1e-4);
        // smoothstep(0.5) = 0.5 by construction.
        assert!((RampCurve::Cosine.apply(0.5) - 0.5).abs() < 1e-4);
        // Endpoints are invariant under every curve.
        for c in [
            RampCurve::Linear,
            RampCurve::Exp,
            RampCurve::Log,
            RampCurve::Cosine,
        ] {
            assert!((c.apply(0.0) - 0.0).abs() < 1e-4);
            assert!((c.apply(1.0) - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn exp_curve_makes_early_leadin_quieter_than_linear() {
        let mut lin = cfg(&[8], 4, true, false);
        let mut exp = lin.clone();
        lin.curve = RampCurve::Linear;
        exp.curve = RampCurve::Exp;
        // d=4 → pos=0.25: linear vel = 0.475, exp pos² = 0.0625 → vel = 0.344.
        let lin_v = preecho_apply(4, 16, &lin).velocity_mul;
        let exp_v = preecho_apply(4, 16, &exp).velocity_mul;
        assert!(
            exp_v < lin_v,
            "exp curve should dip further at early lead-in (got exp={exp_v}, lin={lin_v})"
        );
    }

    // ── v2: probability ramp ─────────────────────────────────────────────

    #[test]
    fn probability_ramp_emits_override_across_window() {
        let mut c = cfg(&[8], 4, false, false);
        c.probability_ramp = true;
        // d=1 → pos=1 → prob = 1.0
        let p1 = preecho_apply(7, 16, &c).probability_override.unwrap();
        assert!((p1 - 1.0).abs() < 1e-4);
        // d=4 → pos=0.25 → prob = 0.475
        let p4 = preecho_apply(4, 16, &c).probability_override.unwrap();
        assert!((p4 - 0.475).abs() < 1e-4);
        // Outside lead-in: None.
        assert_eq!(preecho_apply(2, 16, &c).probability_override, None);
    }

    // ── v2: auto-length ──────────────────────────────────────────────────

    #[test]
    fn auto_length_fills_prior_anchor_gap() {
        // Anchors at 0 and 8.  With auto_length the lead-in into anchor 8
        // fills 7 steps (gap 8, minus 1 for the prior anchor slot).
        let mut c = cfg(&[0, 8], 0, true, false);
        c.auto_length = true;
        // Steps 1..=7 all sit inside the window before anchor 8.
        for step in 1..=7 {
            let v = preecho_apply(step, 16, &c).velocity_mul;
            assert!(v > 0.3 - 1e-4 && v <= 1.0 + 1e-4, "step {step} got {v}");
        }
        // Outside — anchor 8 itself is identity, and steps right after
        // anchor 8 fall inside the wrap window of anchor 0.
        assert_eq!(preecho_apply(8, 16, &c), PreechoApply::IDENTITY);
    }

    #[test]
    fn auto_length_different_gaps_pick_different_windows() {
        // Anchors at 0, 4, 12 — gaps 4, 8, 4 (wrapping 12 → 0 spans 4).
        let mut c = cfg(&[0, 4, 12], 0, true, false);
        c.auto_length = true;
        // Lead-in into anchor 12 has gap 8 → window 7 steps.
        // Step 5 sits inside (d=7), step 11 sits closest (d=1).
        let far = preecho_apply(5, 16, &c).velocity_mul;
        let near = preecho_apply(11, 16, &c).velocity_mul;
        assert!(
            near > far,
            "near lead-in (d=1) should be louder than far (d=7): near={near} far={far}"
        );
    }

    #[test]
    fn auto_length_single_anchor_falls_back() {
        // With one anchor there's no prior gap; auto-length falls back to
        // `length.max(4)` so the feature doesn't vanish silently.
        let mut c = cfg(&[8], 0, true, false);
        c.auto_length = true;
        let r = preecho_apply(7, 16, &c); // d=1 inside the length-4 fallback
        assert!(r.velocity_mul > 0.9);
    }
}
