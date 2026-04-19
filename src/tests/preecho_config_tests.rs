// ─── tests/preecho_config_tests.rs ───────────────────────────────────────────
// Covers `PreechoConfig::is_active` — the 3-way gate the lead-in
// modulator consults before emitting any per-step override.  If this
// returns the wrong answer the whole preecho feature silently drops out
// (all ramps return IDENTITY) or silently fires with no window (div-by-
// zero / empty-range math).

use crate::sequencer::preecho::{NoteApproach, PreechoConfig, RampCurve};

fn cfg_with(enabled: bool, anchors: Vec<u8>, length: u8, auto_length: bool) -> PreechoConfig {
    PreechoConfig {
        enabled,
        anchors,
        length,
        auto_length,
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
fn is_active_false_when_disabled() {
    // Master switch overrides everything else — even with anchors and
    // length set, disabled = inactive.  The whole point of the master
    // toggle.
    let c = cfg_with(false, vec![8], 4, false);
    assert!(!c.is_active());
}

#[test]
fn is_active_false_when_no_anchors() {
    // Without anchors there's nothing to lead into — the per-step
    // walker would have no reference to measure distance from.
    let c = cfg_with(true, Vec::new(), 4, false);
    assert!(!c.is_active());
}

#[test]
fn is_active_false_when_length_zero_and_not_auto() {
    // `length == 0 && !auto_length` is "no lead-in window" — every
    // step would fall outside every window so preecho would no-op
    // anyway.  Better to short-circuit in the gate.
    let c = cfg_with(true, vec![8], 0, false);
    assert!(!c.is_active());
}

#[test]
fn is_active_true_with_explicit_length() {
    // The classic config: enabled, at least one anchor, length > 0.
    let c = cfg_with(true, vec![8], 4, false);
    assert!(c.is_active());
}

#[test]
fn is_active_true_with_auto_length_and_zero_length() {
    // auto_length bypasses the `length > 0` requirement — the effective
    // window comes from gap-to-previous-anchor rather than the field.
    let c = cfg_with(true, vec![4, 8, 12], 0, true);
    assert!(c.is_active());
}

#[test]
fn is_active_true_with_both_length_and_auto() {
    // auto_length alongside a non-zero length is redundant but not
    // incorrect — the apply layer ignores the explicit length when
    // auto is on.  Must still be active.
    let c = cfg_with(true, vec![4, 8, 12], 4, true);
    assert!(c.is_active());
}

#[test]
fn is_active_default_is_inactive() {
    // A freshly-constructed PreechoConfig must be silent — no accidental
    // feature activation on new sessions.
    assert!(!PreechoConfig::default().is_active());
}
