// ─── tests/preecho_apply_tests.rs ────────────────────────────────────────────
// Covers `preecho_apply` — the per-step modulator that ramps velocity,
// accent, probability, ratchet and note-shift across the lead-in window
// before each anchor.  Tests here lock down the "which anchor wins",
// "ramp math hits the endpoints", and "step-on-anchor returns identity"
// rules so a refactor of the curve / endpoint math can't silently drift.

use crate::sequencer::preecho::{
    NoteApproach, NoteShift, PreechoApply, PreechoConfig, RampCurve, preecho_apply,
};

fn base_cfg() -> PreechoConfig {
    PreechoConfig {
        enabled: true,
        anchors: vec![8],
        length: 4,
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

#[test]
fn disabled_returns_identity() {
    let mut cfg = base_cfg();
    cfg.enabled = false;
    cfg.velocity_ramp = true;
    // Disabled master switch overrides every ramp toggle.
    assert_eq!(preecho_apply(7, 16, &cfg), PreechoApply::IDENTITY);
}

#[test]
fn empty_anchors_returns_identity() {
    let mut cfg = base_cfg();
    cfg.anchors.clear();
    cfg.velocity_ramp = true;
    assert_eq!(preecho_apply(5, 16, &cfg), PreechoApply::IDENTITY);
}

#[test]
fn step_on_anchor_returns_identity() {
    let mut cfg = base_cfg();
    cfg.velocity_ramp = true;
    cfg.accent_ramp = true;
    // Anchor step itself — whole point of the feature is that the anchor
    // plays at user intensity, not the ramped lead-in intensity.
    assert_eq!(preecho_apply(8, 16, &cfg), PreechoApply::IDENTITY);
}

#[test]
fn outside_window_returns_identity() {
    let mut cfg = base_cfg();
    cfg.length = 2;
    cfg.velocity_ramp = true;
    // d = (8 + 16 - 3) % 16 = 5 > length=2 → outside every window.
    assert_eq!(preecho_apply(3, 16, &cfg), PreechoApply::IDENTITY);
}

#[test]
fn velocity_ramp_climbs_toward_anchor_under_linear_curve() {
    let mut cfg = base_cfg();
    cfg.velocity_ramp = true;
    // Step positions: anchor=8, length=4 → lead-in covers steps 4..=7.
    //   pos = 1 − (d−1)/length;  velocity_mul = 0.3 + 0.7 * pos.
    //   d=4 (step 4): pos = 0.25 → velocity = 0.475.
    //   d=1 (step 7): pos = 1.00 → velocity = 1.000.
    let earliest = preecho_apply(4, 16, &cfg).velocity_mul;
    let closest = preecho_apply(7, 16, &cfg).velocity_mul;
    assert!(
        (earliest - 0.475).abs() < 1e-3,
        "earliest lead-in should be 0.475, got {earliest}",
    );
    assert!(
        (closest - 1.0).abs() < 1e-3,
        "closest lead-in should be 1.0, got {closest}",
    );
    // Ramp must be strictly monotonic across the window.
    let mid_early = preecho_apply(5, 16, &cfg).velocity_mul;
    let mid_late = preecho_apply(6, 16, &cfg).velocity_mul;
    assert!(earliest < mid_early && mid_early < mid_late && mid_late < closest);
}

#[test]
fn ratchet_ramp_tops_out_at_3_near_anchor() {
    let mut cfg = base_cfg();
    cfg.ratchet_ramp = true;
    // pos=1.0 → round(3.0) = 3.  This is the "stronger near the anchor"
    // invariant — lead-in closest to the anchor must ratchet harder than
    // lead-in farther away.
    assert_eq!(preecho_apply(7, 16, &cfg).ratchet_add, 3);
    assert!(preecho_apply(4, 16, &cfg).ratchet_add <= 1);
}

#[test]
fn slide_cascade_fires_only_on_last_lead_in_step() {
    let mut cfg = base_cfg();
    cfg.slide_cascade = true;
    // d == 1 — slide should be 1.0.
    assert_eq!(preecho_apply(7, 16, &cfg).slide_override, Some(1.0));
    // d == 2 — no slide.
    assert_eq!(preecho_apply(6, 16, &cfg).slide_override, None);
    // d == 4 (earliest) — no slide.
    assert_eq!(preecho_apply(4, 16, &cfg).slide_override, None);
}

#[test]
fn note_approach_arp_walks_double_scale_steps() {
    let mut cfg = base_cfg();
    cfg.note_approach = NoteApproach::Arp;
    let out = preecho_apply(6, 16, &cfg); // d = 2
    let note = out.note_override.expect("arp must produce note override");
    assert_eq!(note.anchor_step, 8);
    assert_eq!(note.shift, NoteShift::ScaleSteps(-4));
}

#[test]
fn closest_anchor_wins_when_multiple_windows_overlap() {
    // Anchors at 4 and 12 with length 6 would both pull step 8 into range
    // (d=−4 wraps to 12 for anchor 4; d=4 for anchor 12).  Closest wins;
    // both are tied at d=4, so order of anchors determines the pick.
    // Simpler test: anchors at 8 and 12, length 4 — step 10 is d=2 from 12
    // and d=14 (no match) from 8 → anchor 12 wins.
    let mut cfg = base_cfg();
    cfg.anchors = vec![8, 12];
    cfg.length = 4;
    cfg.note_approach = NoteApproach::Chromatic;
    let out = preecho_apply(10, 16, &cfg);
    let note = out.note_override.expect("note must resolve");
    assert_eq!(
        note.anchor_step, 12,
        "closer anchor (12) should win for step 10"
    );
    // d = 2 chromatic → shift = -2.
    assert_eq!(note.shift, NoteShift::Semitones(-2));
}

#[test]
fn no_ramps_enabled_returns_identity() {
    // Cfg is valid (enabled + anchors + length) but no ramp toggles, no
    // note approach.  Every returned field should be the identity
    // equivalent even though the step IS in the lead-in window.
    let cfg = base_cfg();
    let out = preecho_apply(7, 16, &cfg);
    assert_eq!(out.velocity_mul, 1.0);
    assert_eq!(out.ratchet_add, 0);
    assert_eq!(out.probability_override, None);
    assert_eq!(out.accent_override, None);
    assert_eq!(out.slide_override, None);
    assert_eq!(out.note_override, None);
}
