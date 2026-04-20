// ─── tests/pipeline_filter_tests.rs ──────────────────────────────────────────
// Covers two pure helpers in `llm/pipeline.rs`:
//   • `lane_apply_scope(lane)` — the scope strings `apply_llm_update` sees
//     when a lane's output is written back to AppState.  Tight invariant:
//     a Bass lane must NEVER include `"kit_a"` or `"kit_b"` in its scope,
//     or the model could overwrite drum patterns from a bass-only
//     inference.  Same goes for the reverse direction.
//   • `filter_lane_output(lane, raw)` — belt-and-suspenders JSON filter
//     that drops top-level keys outside the lane's allow-list and, inside
//     `sequencer`, any subkey outside the lane's permitted set.  Also
//     drops empty pattern arrays that would otherwise silently clear a
//     voice.

use crate::llm::lanes::LaneKind;
use crate::llm::pipeline::{filter_lane_output, lane_apply_scope};

// ─── lane_apply_scope ───────────────────────────────────────────────────────

#[test]
fn bass_scope_never_contains_kit_or_fx_keys() {
    // A bass lane must write into bass.* and sequencer.* only — allowing
    // "kit_a" / "kit_b" here would let a bass-only inference overwrite
    // drum patterns.  Same for "fx" / "hoover" / "an1x".
    let scope = lane_apply_scope(LaneKind::Bass(0));
    assert!(scope.contains(&"bass".to_string()));
    assert!(scope.contains(&"sequencer".to_string()));
    for forbidden in ["kit_a", "kit_b", "fx", "hoover", "an1x", "amen"] {
        assert!(
            !scope.contains(&forbidden.to_string()),
            "Bass scope must not contain {forbidden:?}",
        );
    }
}

#[test]
fn kit_scopes_are_strictly_disjoint() {
    // KitA and KitB scopes must not overlap on the per-voice key — a
    // kit_a lane should NOT be able to write kit_b.*.
    let a = lane_apply_scope(LaneKind::KitA);
    let b = lane_apply_scope(LaneKind::KitB);
    assert!(a.contains(&"kit_a".to_string()));
    assert!(!a.contains(&"kit_b".to_string()));
    assert!(b.contains(&"kit_b".to_string()));
    assert!(!b.contains(&"kit_a".to_string()));
}

#[test]
fn fx_mod_rack_use_top_level_scope() {
    // FX / Modulation / Rack write at the top level of AppState, not
    // under `sequencer` — so the scope is empty (== "touch anything the
    // schema permits").  Tested as a group because a future refactor
    // might split them; the empty-scope contract is what matters.
    for lane in [LaneKind::Fx, LaneKind::Modulation, LaneKind::Rack] {
        assert!(
            lane_apply_scope(lane).is_empty(),
            "{lane:?} must use top-level scope (empty vec)",
        );
    }
}

#[test]
fn every_voice_scope_includes_sequencer() {
    // Every voice lane shares the `sequencer` scope key so it can write
    // its own step array.  If this ever drops, voice patterns would
    // silently stop updating.
    for lane in [
        LaneKind::Bass(0),
        LaneKind::Bass(2),
        LaneKind::KitA,
        LaneKind::KitB,
        LaneKind::Amen,
        LaneKind::Hoover,
        LaneKind::An1x,
    ] {
        assert!(
            lane_apply_scope(lane).contains(&"sequencer".to_string()),
            "{lane:?} must carry `sequencer` in its scope",
        );
    }
}

// ─── filter_lane_output ─────────────────────────────────────────────────────

#[test]
fn filter_drops_top_level_keys_outside_lane_allowlist() {
    // Bass lane outputs shouldn't touch `fx.*` — even if the model
    // emits it, the filter must strip before apply.
    let raw = serde_json::json!({
        "bass": {"cutoff": 0.5},
        "fx": {"reverb_mix": 0.7},
    });
    let out = filter_lane_output(LaneKind::Bass(0), raw);
    assert!(out.get("bass").is_some(), "in-scope keys must survive");
    assert!(out.get("fx").is_none(), "out-of-scope keys must be dropped");
}

#[test]
fn filter_carries_over_thinking_and_comment_keys() {
    // `_thinking` / `_comment` / `mc_line` are meta — they pass through
    // every lane's filter so the log / TTS pipeline can read them.
    let raw = serde_json::json!({
        "_thinking": "reasoning",
        "_comment": "notes",
        "mc_line": "bass drop incoming",
        "bass": {"cutoff": 0.4},
    });
    let out = filter_lane_output(LaneKind::Bass(0), raw);
    assert_eq!(
        out.get("_thinking").and_then(|v| v.as_str()),
        Some("reasoning"),
    );
    assert_eq!(out.get("_comment").and_then(|v| v.as_str()), Some("notes"));
    assert_eq!(
        out.get("mc_line").and_then(|v| v.as_str()),
        Some("bass drop incoming"),
    );
}

#[test]
fn filter_strips_sequencer_subkeys_outside_lane_scope() {
    // Bass lane must not write kick_a_steps even via `sequencer.kick_a_steps`.
    let raw = serde_json::json!({
        "sequencer": {
            "bass_steps": [0, 4, 8, 12],
            "kick_a_steps": [0, 4, 8, 12],
        },
    });
    let out = filter_lane_output(LaneKind::Bass(0), raw);
    let seq = out.get("sequencer").and_then(|s| s.as_object()).unwrap();
    assert!(
        seq.contains_key("bass_steps"),
        "in-scope sequencer subkey must survive",
    );
    assert!(
        !seq.contains_key("kick_a_steps"),
        "out-of-scope sequencer subkey must be dropped",
    );
}

#[test]
fn filter_drops_empty_destructive_pattern_arrays() {
    // Empty `bass_steps: []` would be interpreted by apply_llm_update as
    // "clear every step" — a destructive silent-failure.  The filter
    // drops it so the prior pattern survives.
    let raw = serde_json::json!({
        "sequencer": {
            "bass_steps": [],
            "bass_notes": [0, 4, 8],
        },
    });
    let out = filter_lane_output(LaneKind::Bass(0), raw);
    let seq = out.get("sequencer").and_then(|s| s.as_object()).unwrap();
    assert!(
        !seq.contains_key("bass_steps"),
        "empty bass_steps must be dropped",
    );
    assert!(
        seq.contains_key("bass_notes"),
        "non-empty destructive-eligible keys must survive",
    );
}

#[test]
fn filter_drops_sequencer_entirely_when_all_subkeys_filtered() {
    // If every sequencer subkey is out-of-scope, the resulting empty
    // sequencer object must NOT appear in the output (would cause a
    // no-op-but-noisy apply).
    let raw = serde_json::json!({
        "sequencer": {"kick_a_steps": [1, 0, 1, 0]},
    });
    let out = filter_lane_output(LaneKind::Bass(0), raw);
    assert!(
        out.get("sequencer").is_none(),
        "sequencer with all subkeys filtered must be dropped, got {out:?}",
    );
}

#[test]
fn filter_non_object_passes_through_unchanged() {
    // Callers give the filter whatever comes out of json_repair — a
    // null / array / string must pass through untouched instead of
    // panicking.
    assert_eq!(
        filter_lane_output(LaneKind::Bass(0), serde_json::Value::Null),
        serde_json::Value::Null
    );
    let arr = serde_json::json!([1, 2, 3]);
    assert_eq!(filter_lane_output(LaneKind::Fx, arr.clone()), arr);
}
