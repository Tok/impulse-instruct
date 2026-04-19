// ─── tests/lane_method_tests.rs ──────────────────────────────────────────────
// Covers the four pure `LaneKind` methods that drive lane isolation
// (`label`, `output_keys`, `sequencer_subkeys`, `task_description`).
// The filter in `pipeline::filter_lane_output` runs on top of these
// lists, so a mistake here silently lets a Bass lane overwrite drum
// patterns / lift settings out of the Settings scope / etc.

use crate::llm::lanes::LaneKind;

const ALL_LANES: &[LaneKind] = &[
    LaneKind::Settings,
    LaneKind::Bass(0),
    LaneKind::Bass(1),
    LaneKind::Bass(2),
    LaneKind::Bass(3),
    LaneKind::KitA,
    LaneKind::KitB,
    LaneKind::Amen,
    LaneKind::Hoover,
    LaneKind::An1x,
    LaneKind::Fx,
    LaneKind::Modulation,
    LaneKind::Rack,
];

// ─── label ──────────────────────────────────────────────────────────────────

#[test]
fn lane_labels_are_non_empty_and_ascii_lowercase_ish() {
    // Labels feed log lines / telemetry — empty would break log parsing
    // and mixed-case would churn sorts.  All current labels are
    // lowercase identifiers.
    for lane in ALL_LANES {
        let l = lane.label();
        assert!(!l.is_empty(), "{lane:?} label is empty");
        assert!(
            l.chars()
                .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
            "{lane:?} label {l:?} should be lowercase snake_case",
        );
    }
}

#[test]
fn bass_labels_disambiguate_voices() {
    // Each bass voice has its own label so telemetry can distinguish
    // them.  A fallback "bassN" exists for indices ≥4 — it should NOT
    // be used for 0..=3.
    assert_eq!(LaneKind::Bass(0).label(), "bass1");
    assert_eq!(LaneKind::Bass(1).label(), "bass2");
    assert_eq!(LaneKind::Bass(2).label(), "bass3");
    assert_eq!(LaneKind::Bass(3).label(), "bass4");
    // Out-of-range indices fall into the generic bassN bucket.
    assert_eq!(LaneKind::Bass(99).label(), "bassN");
}

// ─── output_keys — scope isolation ──────────────────────────────────────────

#[test]
fn kit_lanes_only_expose_sequencer_output_key() {
    // KitA / KitB must not let the model emit anything at the top
    // level — drums belong inside `sequencer.*`.  If this drops and
    // `fx` or `bass` sneak in, the kit lane can overwrite other
    // voices.
    assert_eq!(LaneKind::KitA.output_keys(), &["sequencer"]);
    assert_eq!(LaneKind::KitB.output_keys(), &["sequencer"]);
}

#[test]
fn settings_lane_excludes_voice_keys() {
    // Settings owns globals (bpm/scale/ramps) — NOT bass/fx/etc.  A
    // leak would let the settings pass rewrite voice params.
    let keys = LaneKind::Settings.output_keys();
    for forbidden in ["bass", "fx", "hoover", "an1x", "amen", "lfo", "rack"] {
        assert!(
            !keys.contains(&forbidden),
            "Settings lane must not own {forbidden:?}: {keys:?}",
        );
    }
}

#[test]
fn fx_lane_owns_only_fx_top_level_key() {
    // No sequencer / voice / rack access — FX lane is purely about the
    // post-voice bus.
    assert_eq!(LaneKind::Fx.output_keys(), &["fx"]);
}

#[test]
fn modulation_lane_owns_only_lfo_and_free_eg() {
    // The mod lane drives LFO slots + free_eg; anything else is
    // out of scope.
    assert_eq!(LaneKind::Modulation.output_keys(), &["lfo", "free_eg"]);
}

#[test]
fn rack_lane_owns_only_rack_key() {
    // Rack routing lives at the top level under `rack`.  Must not
    // share keys with voice / FX lanes or a rack pass could rewrite
    // voice state.
    assert_eq!(LaneKind::Rack.output_keys(), &["rack"]);
}

// ─── sequencer_subkeys — per-voice isolation ────────────────────────────────

#[test]
fn bass_subkeys_include_expected_per_voice_prefix() {
    // Bass(1) must emit `bass2_*` keys (not `bass_*`), and vice versa.
    let v1 = LaneKind::Bass(0).sequencer_subkeys();
    let v2 = LaneKind::Bass(1).sequencer_subkeys();
    assert!(v1.iter().any(|k| k == "bass_steps"));
    assert!(v1.iter().any(|k| k == "bass_notes"));
    assert!(!v1.iter().any(|k| k.starts_with("bass2_")));
    assert!(v2.iter().any(|k| k == "bass2_steps"));
    assert!(v2.iter().any(|k| k == "bass2_notes"));
    assert!(
        !v2.iter()
            .any(|k| k.starts_with("bass_") && !k.starts_with("bass2"))
    );
}

#[test]
fn kit_a_and_kit_b_subkeys_are_strictly_disjoint() {
    // Drum patterns per kit must not share subkeys — a leak would let
    // kit_a rewrite kit_b.
    use std::collections::HashSet;
    let a: HashSet<String> = LaneKind::KitA.sequencer_subkeys().into_iter().collect();
    let b: HashSet<String> = LaneKind::KitB.sequencer_subkeys().into_iter().collect();
    // `drum_lengths` / `drum_ratchets` / `preecho` are shared meta
    // arrays indexed by voice — both kits DO emit them.  Everything
    // else must be disjoint.
    for k in a.intersection(&b) {
        assert!(
            matches!(k.as_str(), "drum_lengths" | "drum_ratchets" | "preecho"),
            "kit_a and kit_b share unexpected subkey: {k:?}",
        );
    }
    // Voice-specific keys are on the correct side.
    assert!(a.contains("kick_a_steps"));
    assert!(!a.contains("kick_b_steps"));
    assert!(b.contains("kick_b_steps"));
    assert!(!b.contains("kick_a_steps"));
}

#[test]
fn every_voice_lane_carries_preecho_subkey() {
    // Preecho config can be emitted by any melodic / drum lane — all
    // their subkey lists should include it so the pipeline filter
    // doesn't strip the ramp settings.
    for lane in [
        LaneKind::Bass(0),
        LaneKind::Bass(2),
        LaneKind::KitA,
        LaneKind::KitB,
        LaneKind::Amen,
        LaneKind::Hoover,
        LaneKind::An1x,
    ] {
        let subs = lane.sequencer_subkeys();
        assert!(
            subs.contains(&"preecho".to_string()),
            "{lane:?} sequencer_subkeys must include preecho, got {subs:?}",
        );
    }
}

#[test]
fn settings_subkeys_are_global_only() {
    // Settings lane owns bpm/steps/swing/time_sig_num/root_note/scale/
    // scale_snap — the transport globals.  Must NOT include any voice
    // pattern keys.
    let subs = LaneKind::Settings.sequencer_subkeys();
    assert!(subs.contains(&"bpm".to_string()));
    assert!(subs.contains(&"scale".to_string()));
    for forbidden in ["bass_steps", "kick_a_steps", "hoover_steps"] {
        assert!(
            !subs.contains(&forbidden.to_string()),
            "Settings lane must not own voice subkey {forbidden:?}",
        );
    }
}

#[test]
fn non_sequencer_lanes_have_empty_subkeys() {
    // FX / Modulation / Rack write at the top level — they have
    // no sequencer subkeys at all.  An accidental entry here would
    // let one of these lanes spill into sequencer state.
    for lane in [LaneKind::Fx, LaneKind::Modulation, LaneKind::Rack] {
        assert!(
            lane.sequencer_subkeys().is_empty(),
            "{lane:?} must have no sequencer subkeys",
        );
    }
}

// ─── task_description ───────────────────────────────────────────────────────

#[test]
fn every_lane_has_a_non_empty_task_description() {
    // The description drops into the per-lane prompt footer.  An empty
    // one would leave the model with no task framing.
    for lane in ALL_LANES {
        let desc = lane.task_description();
        assert!(
            !desc.is_empty(),
            "{lane:?} task description must not be empty"
        );
        assert!(
            desc.chars().any(|c| c.is_ascii_alphabetic()),
            "{lane:?} description should contain letters",
        );
    }
}
