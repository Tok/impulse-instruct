// ─── tests/planner_output_tests.rs ───────────────────────────────────────────
// Covers the two pure planner helpers in `llm/planner.rs` that
// `planner_tests.rs` doesn't already test:
//   • `parse_planner_output` — parses the planner LLM's JSON envelope,
//     filters to live lanes, and auto-expands bass-containing plans to
//     cover every active bass voice (avoiding the "bass1 gets rewritten
//     while bass2 stays silent" failure mode).
//   • `default_plan` — deterministic fallback when the planner fails;
//     must honor rack content (skip absent voices) and active-voice set.

use crate::llm::lanes::LaneKind;
use crate::llm::planner::{default_plan, parse_planner_output};
use crate::state::AppState;

// ─── parse_planner_output ───────────────────────────────────────────────────

#[test]
fn parse_returns_none_for_empty_lanes_array() {
    let s = AppState::default();
    let json = serde_json::json!({"lanes": [], "rationale": ""});
    assert!(parse_planner_output(&s, &json).is_none());
}

#[test]
fn parse_returns_none_when_all_labels_unknown() {
    // Garbage labels get filtered; nothing left → None so the caller
    // falls back to `default_plan`.
    let s = AppState::default();
    let json = serde_json::json!({"lanes": ["xyzzy", "fooooo"]});
    assert!(parse_planner_output(&s, &json).is_none());
}

#[test]
fn parse_preserves_rationale_and_order() {
    let s = AppState::default();
    let json = serde_json::json!({
        "lanes": ["settings", "bass1", "fx"],
        "rationale": "narrow bass tweak",
    });
    let plan = parse_planner_output(&s, &json).unwrap();
    assert_eq!(plan.rationale, "narrow bass tweak");
    // Settings should come before Bass which should come before Fx in
    // the order the planner picked.
    let pos = |lane: LaneKind| plan.lanes.iter().position(|l| *l == lane).unwrap();
    assert!(pos(LaneKind::Settings) < pos(LaneKind::Bass(0)));
    assert!(pos(LaneKind::Bass(0)) < pos(LaneKind::Fx));
}

#[test]
fn parse_deduplicates_repeated_labels() {
    // Model occasionally emits the same lane twice.  Duplicates must be
    // silently collapsed so the pipeline doesn't run the same inference
    // twice (wastes tokens, risks double-apply state).
    let s = AppState::default();
    let json = serde_json::json!({"lanes": ["bass1", "bass1", "fx"]});
    let plan = parse_planner_output(&s, &json).unwrap();
    let bass_count = plan
        .lanes
        .iter()
        .filter(|l| matches!(l, LaneKind::Bass(0)))
        .count();
    assert_eq!(bass_count, 1, "duplicate bass1 must be collapsed");
}

#[test]
fn parse_auto_expands_broad_plan_with_multiple_lanes_besides_bass() {
    // Heuristic: a plan is treated as "narrow" only when the raw lane
    // list is ≤2 entries with exactly one bass label.  A broader plan
    // (≥3 lanes, or multiple bass labels) triggers auto-expand so every
    // active bass voice gets its own write — avoiding the "bass2/3
    // silently skipped" failure mode.
    let mut s = AppState::default();
    s.bass_voices[0].enabled = true;
    s.bass_voices[1].enabled = true;
    s.bass_voices[2].enabled = true;
    // ≥3 lanes + single bass label → broad.
    let json = serde_json::json!({
        "lanes": ["settings", "kit_a", "bass1", "fx"],
    });
    let plan = parse_planner_output(&s, &json).unwrap();
    for idx in 0..3 {
        assert!(
            plan.lanes.contains(&LaneKind::Bass(idx)),
            "Bass({idx}) should be auto-expanded into the broad plan; got {:?}",
            plan.lanes,
        );
    }
    // Fx comes AFTER all bass lanes (order preserved by the expand logic).
    let fx_pos = plan.lanes.iter().position(|l| *l == LaneKind::Fx).unwrap();
    let last_bass = plan
        .lanes
        .iter()
        .rposition(|l| matches!(l, LaneKind::Bass(_)))
        .unwrap();
    assert!(last_bass < fx_pos, "fx must come after all bass lanes");
}

#[test]
fn parse_keeps_two_lane_plan_narrow_even_on_multi_voice_rack() {
    // The flipside of the broad-expand test: `["bass1", "fx"]` is still
    // treated as narrow because raw_labels.len() <= 2 AND named_bass_count
    // == 1.  bass2 / bass3 stay out of the plan even though they're
    // enabled — so "add reverb to bass 1" doesn't become a full bass
    // rewrite.
    let mut s = AppState::default();
    s.bass_voices[0].enabled = true;
    s.bass_voices[1].enabled = true;
    s.bass_voices[2].enabled = true;
    let json = serde_json::json!({"lanes": ["bass1", "fx"]});
    let plan = parse_planner_output(&s, &json).unwrap();
    assert!(plan.lanes.contains(&LaneKind::Bass(0)));
    assert!(!plan.lanes.contains(&LaneKind::Bass(1)));
    assert!(!plan.lanes.contains(&LaneKind::Bass(2)));
}

#[test]
fn parse_preserves_narrow_single_voice_command() {
    // User said "rewrite bass 2" → planner emits just `["bass2"]`.  The
    // auto-expand heuristic must NOT pull in bass1 / bass3 (that would
    // turn a narrow command into a full rewrite).
    let mut s = AppState::default();
    s.bass_voices[0].enabled = true;
    s.bass_voices[1].enabled = true;
    s.bass_voices[2].enabled = true;
    let json = serde_json::json!({"lanes": ["bass2"]});
    let plan = parse_planner_output(&s, &json).unwrap();
    assert_eq!(plan.lanes, vec![LaneKind::Bass(1)]);
}

#[test]
fn parse_drops_unknown_labels_but_keeps_known_ones() {
    let s = AppState::default();
    let json = serde_json::json!({"lanes": ["fx", "notalane", "settings"]});
    let plan = parse_planner_output(&s, &json).unwrap();
    assert!(plan.lanes.contains(&LaneKind::Fx));
    assert!(plan.lanes.contains(&LaneKind::Settings));
    assert_eq!(plan.lanes.len(), 2, "unknown labels must be dropped");
}

// ─── default_plan ───────────────────────────────────────────────────────────

#[test]
fn default_plan_always_starts_with_settings() {
    // Settings is ordered first so style / bpm land before any voice
    // lane writes its steps.  Non-negotiable.
    let s = AppState::default();
    let plan = default_plan(&s);
    assert_eq!(plan.lanes.first(), Some(&LaneKind::Settings));
}

#[test]
fn default_plan_always_ends_with_fx() {
    // Fx closes out the default pass so voice writes land before the
    // send / master mix dials get tuned.
    let s = AppState::default();
    let plan = default_plan(&s);
    assert_eq!(plan.lanes.last(), Some(&LaneKind::Fx));
}

#[test]
fn default_plan_orders_drums_before_bass_before_hoover() {
    // Canonical order from the default plan's doc comment:
    //   Settings → KitA → KitB → Amen → Bass(…) → Hoover → An1x → Fx.
    // Build a rack that has all four and check adjacency-free ordering.
    let s = AppState::default();
    let plan = default_plan(&s);
    let pos = |lane: LaneKind| plan.lanes.iter().position(|l| *l == lane);
    if let (Some(a), Some(b)) = (pos(LaneKind::KitA), pos(LaneKind::Bass(0))) {
        assert!(a < b, "KitA must come before Bass in default_plan");
    }
    if let (Some(a), Some(b)) = (pos(LaneKind::Bass(0)), pos(LaneKind::Hoover)) {
        assert!(a < b, "Bass must come before Hoover in default_plan");
    }
    if let (Some(a), Some(b)) = (pos(LaneKind::Hoover), pos(LaneKind::An1x)) {
        assert!(a < b, "Hoover must come before An1x in default_plan");
    }
}

#[test]
fn default_plan_skips_disabled_bass_voices() {
    // Only enabled bass voices should appear in the default plan —
    // otherwise each cycle burns tokens on silent voices.
    let mut s = AppState::default();
    // Default rack has AcidBass, so Bass lanes will be emitted.
    s.bass_voices[0].enabled = true;
    s.bass_voices[1].enabled = false;
    s.bass_voices[2].enabled = true;
    s.bass_voices[3].enabled = false;
    let plan = default_plan(&s);
    assert!(plan.lanes.contains(&LaneKind::Bass(0)));
    assert!(!plan.lanes.contains(&LaneKind::Bass(1)));
    assert!(plan.lanes.contains(&LaneKind::Bass(2)));
    assert!(!plan.lanes.contains(&LaneKind::Bass(3)));
}

#[test]
fn default_plan_rationale_is_non_empty() {
    // The rationale string feeds UI telemetry — empty would show as
    // "(no reason)" in the planner log.  Keep this contract.
    let s = AppState::default();
    let plan = default_plan(&s);
    assert!(!plan.rationale.is_empty());
    assert!(plan.rationale.to_lowercase().contains("default"));
}
