// ─── tests/planner_schema_tests.rs ───────────────────────────────────────────
// Covers `planner_schema` + `build_planner_prompt` — the two halves of
// the planner LLM's contract.  The schema's `enum` on lane labels must
// mirror the lane list in the prompt, or the grammar will reject valid
// plans.  The prompt must mention the active voices (the planner uses
// them to gate which lanes it may emit).
//
// Tests focus on INVARIANTS — enum coverage, required fields, lane /
// state sync — not the exact wording of the prompt, so the tests don't
// ossify phrasing.

use crate::llm::planner::{build_planner_prompt, planner_schema};
use crate::state::AppState;

// ─── planner_schema ─────────────────────────────────────────────────────────

#[test]
fn planner_schema_has_closed_top_level_and_required_lanes() {
    let s = planner_schema();
    assert_eq!(
        s.get("type").and_then(|v| v.as_str()),
        Some("object"),
        "planner schema must be an object",
    );
    assert_eq!(
        s.get("additionalProperties").and_then(|v| v.as_bool()),
        Some(false),
        "planner schema must forbid stray keys",
    );
    let required: Vec<String> = s
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        required.contains(&"lanes".to_string()),
        "planner schema must require `lanes`, got {required:?}",
    );
}

#[test]
fn planner_lanes_enum_matches_dispatchable_lane_set() {
    // The planner schema's `lanes.items.enum` enumerates the exact set
    // of strings the planner may emit.  This must stay in sync with
    // `lane_from_label` — if a new lane is added without updating the
    // schema, grammar-constrained generation rejects valid plans; if
    // a lane is removed without updating the schema, the filter drops
    // the stale value silently.
    let expected: std::collections::HashSet<&'static str> = [
        "settings", "bass1", "bass2", "bass3", "bass4", "kit_a", "kit_b", "amen", "hoover", "an1x",
        "fx", "mod", "rack",
    ]
    .into_iter()
    .collect();
    let schema = planner_schema();
    let enum_vals: std::collections::HashSet<String> = schema
        .get("properties")
        .and_then(|v| v.get("lanes"))
        .and_then(|v| v.get("items"))
        .and_then(|v| v.get("enum"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let enum_strs: std::collections::HashSet<&str> = enum_vals.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        enum_strs, expected,
        "planner schema lane enum must match the dispatchable lane label set",
    );
}

#[test]
fn planner_lanes_array_has_min_one_and_max_twelve() {
    // minItems=1: an empty plan is meaningless (the fallback path is
    // `default_plan`, not the planner).  maxItems=12: matches the max
    // lane count — a 13+ plan is suspicious and should be rejected.
    let s = planner_schema();
    let lanes = s.get("properties").and_then(|v| v.get("lanes")).unwrap();
    assert_eq!(lanes.get("minItems").and_then(|v| v.as_u64()), Some(1));
    assert_eq!(lanes.get("maxItems").and_then(|v| v.as_u64()), Some(12));
}

#[test]
fn planner_rationale_has_length_cap() {
    // The rationale field is shown in telemetry / log; cap it at a
    // reasonable length so a runaway model can't eat token budget on
    // a sprawling explanation.
    let s = planner_schema();
    let rationale = s
        .get("properties")
        .and_then(|v| v.get("rationale"))
        .unwrap();
    assert_eq!(
        rationale.get("maxLength").and_then(|v| v.as_u64()),
        Some(200),
        "rationale must have maxLength=200 — any higher is a sprawl risk",
    );
}

// ─── build_planner_prompt ───────────────────────────────────────────────────

#[test]
fn planner_prompt_includes_every_available_lane_label() {
    // The model reads the AVAILABLE LANES bullet list to know which
    // labels it may emit.  If a label goes missing from the prompt,
    // the model won't know to pick it even though the grammar allows.
    let s = AppState::default();
    let prompt = build_planner_prompt(&s);
    for label in [
        "settings", "bass1", "bass2", "bass3", "bass4", "kit_a", "kit_b", "amen", "hoover", "an1x",
        "fx", "mod", "rack",
    ] {
        assert!(
            prompt.contains(label),
            "planner prompt must mention lane label {label:?}",
        );
    }
}

#[test]
fn planner_prompt_reports_style_when_active() {
    // Active style name must appear in STATE line so the planner can
    // reason about it — missing would mean the narrow-command check
    // silently ignores style-specific cues.
    let mut s = AppState::default();
    s.llm.active_style = Some("acid_house".to_string());
    let prompt = build_planner_prompt(&s);
    assert!(
        prompt.contains("style=acid_house"),
        "planner prompt must surface the active style id",
    );
}

#[test]
fn planner_prompt_style_none_renders_as_none_literal() {
    // No active style → STATE shows `style=none`, not `style=` or a
    // default.  The planner heuristic keys on that exact token.
    let s = AppState::default();
    let prompt = build_planner_prompt(&s);
    assert!(
        prompt.contains("style=none"),
        "planner prompt must say style=none when no style is active",
    );
}

#[test]
fn planner_prompt_emits_user_owned_style_warning() {
    // The planner must NEVER fire a lane that changes the active style.
    // The prompt relies on a specific cautionary phrase to anchor this
    // rule — if that phrase drifts, the model may start emitting
    // settings lanes that rewrite the style silently.
    let s = AppState::default();
    let prompt = build_planner_prompt(&s);
    assert!(
        prompt.to_lowercase().contains("user-owned")
            || prompt.to_lowercase().contains("user owned"),
        "planner prompt must flag style as user-owned",
    );
}
