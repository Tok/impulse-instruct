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

/// How a lead-in step's note is chosen to resolve into the anchor note.
/// The caller owns the key / scale context and resolves the shift returned
/// by `preecho_apply`; this enum just picks the shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NoteApproach {
    /// No note rewriting — lead-in steps play their stored notes.  v1
    /// behaviour.  Default so existing configs read identically.
    #[default]
    Off,
    /// Descending chromatic approach: the step at distance `d` from the
    /// anchor plays `anchor_note - d` semitones.  Simple, genre-neutral
    /// — a classic tension-into-resolution lead-in.
    Chromatic,
    /// Descending scale-step approach: walks `d` scale-degrees below the
    /// anchor note, snapped to the project's active scale.  More musical
    /// than chromatic when the scale is known.
    Scale,
    /// Ascending arpeggio approach: cycles through root/3rd/5th below the
    /// anchor.  `d=1` lands on a 3rd below, `d=2` on a 5th below, `d=3`
    /// on the root below — repeating.  Gives a lead-in that feels like a
    /// chord voicing unfurling into the downbeat.
    Arp,
}

/// How to shift a lead-in step's note relative to the anchor.  Returned
/// by `preecho_apply` as part of the optional note override; the caller
/// resolves `ScaleSteps` against the active scale and clamps to MIDI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteShift {
    /// Add this many semitones to the anchor note (negative = descending).
    Semitones(i16),
    /// Walk this many scale-steps from the anchor note (negative = down).
    ScaleSteps(i16),
}

/// Resolved note rewrite for a lead-in step.  Carries the anchor step
/// index (so the caller can look up the anchor's stored note from the
/// voice's pattern array) plus the shift to apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteOverride {
    /// Step index in the pattern of the anchor this lead-in targets.
    pub anchor_step: u8,
    /// How the anchor note should be shifted to produce this step's note.
    pub shift: NoteShift,
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
    /// v2 melodic add-on: rewrite lead-in step notes to resolve into the
    /// anchor note (chromatic, scale-step, or arpeggio).  `Off` leaves
    /// the stored notes alone — backwards compatible with v1 configs.
    #[serde(default)]
    pub note_approach: NoteApproach,
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
            note_approach: NoteApproach::Off,
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
    /// When `Some`, rewrites the step's note so the lead-in resolves into
    /// the anchor.  Carries the anchor step index so the caller can look
    /// up the anchor's stored note plus the shift to apply.  `None` on
    /// anchor steps, outside every window, or when `note_approach` is
    /// `Off`.
    pub note_override: Option<NoteOverride>,
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
        note_override: None,
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
    // (distance, effective length, anchor step index)
    let mut best: Option<(usize, u8, u8)> = None;
    for (ai, &a) in cfg.anchors.iter().enumerate() {
        let anchor_step = a;
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
        if best.map(|(bd, _, _)| d < bd).unwrap_or(true) {
            best = Some((d, eff_len, anchor_step));
        }
    }
    let Some((d, eff_len, anchor_step)) = best else {
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
    // Note-approach shift.  Chromatic walks semitone-by-semitone; Scale
    // walks scale-step-by-scale-step; Arp walks scale-steps in twos so
    // the lead-in outlines a triad (−2 → 3rd, −4 → 5th, −6 → root, ...).
    // Sign is negative so every mode resolves upward into the anchor.
    let note_override = match cfg.note_approach {
        NoteApproach::Off => None,
        NoteApproach::Chromatic => Some(NoteOverride {
            anchor_step,
            shift: NoteShift::Semitones(-(d as i16)),
        }),
        NoteApproach::Scale => Some(NoteOverride {
            anchor_step,
            shift: NoteShift::ScaleSteps(-(d as i16)),
        }),
        NoteApproach::Arp => Some(NoteOverride {
            anchor_step,
            shift: NoteShift::ScaleSteps(-2 * d as i16),
        }),
    };

    PreechoApply {
        velocity_mul,
        ratchet_add,
        probability_override,
        accent_override,
        slide_override,
        note_override,
    }
}

/// Resolve a `NoteShift` against a concrete anchor note + scale context
/// into the final MIDI note for a lead-in step.  Pure, allocation-free
/// (no `scale_notes()` call), safe to use from the audio thread.
/// Clamps to the valid MIDI range (0..=127).
pub fn resolve_note_shift(
    anchor_note: u8,
    shift: NoteShift,
    root: u8,
    scale: crate::state::Scale,
) -> u8 {
    match shift {
        NoteShift::Semitones(n) => (anchor_note as i16 + n).clamp(0, 127) as u8,
        NoteShift::ScaleSteps(n) => walk_scale_steps(anchor_note, n, root, scale),
    }
}

/// Walk `steps` scale-degrees from `note` under `(root, scale)`.  Positive
/// `steps` walks up, negative walks down.  If `note` isn't in the scale,
/// snap to the nearest scale degree at or below `note` first so the walk
/// stays inside the scale.
fn walk_scale_steps(note: u8, steps: i16, root: u8, scale: crate::state::Scale) -> u8 {
    if steps == 0 {
        return note;
    }
    if scale == crate::state::Scale::Chromatic {
        return (note as i16 + steps).clamp(0, 127) as u8;
    }
    let intervals = scale.intervals();
    let n = intervals.len() as i16;
    let root = root % 12;
    // Pitch class of `note` relative to the root (0..=11).
    let pc = (note as i16 - root as i16).rem_euclid(12);
    // Find the largest scale interval <= pc — that's our degree anchor.
    let mut degree = 0i16;
    for (i, &iv) in intervals.iter().enumerate() {
        if iv as i16 <= pc {
            degree = i as i16;
        }
    }
    // Integer octave of `note` relative to the root+degree combo.
    let base_semis_from_root = note as i16 - root as i16;
    let octave = (base_semis_from_root - intervals[degree as usize] as i16).div_euclid(12);
    // Walk the degree + fold octave shifts back in.
    let raw = degree + steps;
    let oct_shift = raw.div_euclid(n);
    let new_degree = raw.rem_euclid(n);
    let new_semis = root as i16 + intervals[new_degree as usize] as i16 + (octave + oct_shift) * 12;
    new_semis.clamp(0, 127) as u8
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
            note_approach: NoteApproach::Off,
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
            note_approach: NoteApproach::Off,
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

    // ── v2: note approach ────────────────────────────────────────────────

    fn cfg_note_approach(anchors: &[u8], length: u8, mode: NoteApproach) -> PreechoConfig {
        PreechoConfig {
            enabled: true,
            anchors: anchors.to_vec(),
            length,
            auto_length: false,
            curve: RampCurve::Linear,
            velocity_ramp: false,
            ratchet_ramp: false,
            probability_ramp: false,
            accent_ramp: false,
            slide_cascade: false,
            note_approach: mode,
        }
    }

    #[test]
    fn note_approach_off_leaves_note_override_none() {
        let c = cfg_note_approach(&[8], 4, NoteApproach::Off);
        assert_eq!(preecho_apply(7, 16, &c).note_override, None);
    }

    #[test]
    fn chromatic_approach_shifts_by_distance_semitones() {
        let c = cfg_note_approach(&[8], 4, NoteApproach::Chromatic);
        let o1 = preecho_apply(7, 16, &c).note_override.unwrap();
        assert_eq!(o1.anchor_step, 8);
        assert_eq!(o1.shift, NoteShift::Semitones(-1));
        let o3 = preecho_apply(5, 16, &c).note_override.unwrap();
        assert_eq!(o3.shift, NoteShift::Semitones(-3));
    }

    #[test]
    fn scale_approach_shifts_by_distance_scale_steps() {
        let c = cfg_note_approach(&[8], 4, NoteApproach::Scale);
        let o1 = preecho_apply(7, 16, &c).note_override.unwrap();
        assert_eq!(o1.shift, NoteShift::ScaleSteps(-1));
        let o3 = preecho_apply(5, 16, &c).note_override.unwrap();
        assert_eq!(o3.shift, NoteShift::ScaleSteps(-3));
    }

    #[test]
    fn arp_approach_doubles_scale_steps() {
        let c = cfg_note_approach(&[8], 4, NoteApproach::Arp);
        let o1 = preecho_apply(7, 16, &c).note_override.unwrap();
        assert_eq!(o1.shift, NoteShift::ScaleSteps(-2));
        let o2 = preecho_apply(6, 16, &c).note_override.unwrap();
        assert_eq!(o2.shift, NoteShift::ScaleSteps(-4));
        let o3 = preecho_apply(5, 16, &c).note_override.unwrap();
        assert_eq!(o3.shift, NoteShift::ScaleSteps(-6));
    }

    #[test]
    fn note_approach_carries_nearest_anchor_step() {
        // Two anchors; closest one wins + its step index survives into the
        // override so the caller knows which anchor note to look up.
        let c = cfg_note_approach(&[4, 12], 3, NoteApproach::Chromatic);
        assert_eq!(
            preecho_apply(11, 16, &c).note_override.unwrap().anchor_step,
            12
        );
        assert_eq!(
            preecho_apply(3, 16, &c).note_override.unwrap().anchor_step,
            4
        );
    }

    #[test]
    fn note_approach_is_none_at_anchor_and_outside_window() {
        let c = cfg_note_approach(&[8], 3, NoteApproach::Chromatic);
        // Anchor itself.
        assert_eq!(preecho_apply(8, 16, &c).note_override, None);
        // Far outside window.
        assert_eq!(preecho_apply(2, 16, &c).note_override, None);
    }

    // ── v2: resolve_note_shift ──────────────────────────────────────────
    use crate::state::Scale;

    #[test]
    fn resolve_semitones_walks_linearly() {
        assert_eq!(
            resolve_note_shift(60, NoteShift::Semitones(-1), 0, Scale::NaturalMinor),
            59
        );
        assert_eq!(
            resolve_note_shift(60, NoteShift::Semitones(-3), 0, Scale::NaturalMinor),
            57
        );
    }

    #[test]
    fn resolve_semitones_clamps_to_midi_range() {
        assert_eq!(
            resolve_note_shift(2, NoteShift::Semitones(-10), 0, Scale::NaturalMinor),
            0
        );
        assert_eq!(
            resolve_note_shift(120, NoteShift::Semitones(20), 0, Scale::NaturalMinor),
            127
        );
    }

    #[test]
    fn resolve_scale_steps_walks_minor_scale() {
        // A natural minor from A (root=9): A B C D E F G → MIDI 57 59 60 62 64 65 67.
        // Anchor A4 = 69 (scale-degree 0 in A minor, octave above).
        // Step -1 → G4 (67), -2 → F4 (65), -3 → E4 (64).
        assert_eq!(
            resolve_note_shift(69, NoteShift::ScaleSteps(-1), 9, Scale::NaturalMinor),
            67
        );
        assert_eq!(
            resolve_note_shift(69, NoteShift::ScaleSteps(-2), 9, Scale::NaturalMinor),
            65
        );
        assert_eq!(
            resolve_note_shift(69, NoteShift::ScaleSteps(-3), 9, Scale::NaturalMinor),
            64
        );
    }

    #[test]
    fn resolve_scale_steps_chromatic_mode_is_semitone_walk() {
        // Chromatic scale behaves like semitone stepping (every step = 1 semi).
        assert_eq!(
            resolve_note_shift(60, NoteShift::ScaleSteps(-3), 0, Scale::Chromatic),
            57
        );
    }

    #[test]
    fn resolve_scale_steps_crosses_octave_correctly() {
        // A minor pentatonic has 5 tones → walking -5 steps drops exactly
        // one octave; -6 steps drops 1 octave and 1 penta-tone.
        // A minor penta = A C D E G. Anchor A4 = 69. -5 → A3 = 57.
        assert_eq!(
            resolve_note_shift(69, NoteShift::ScaleSteps(-5), 9, Scale::Pentatonic),
            57
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
