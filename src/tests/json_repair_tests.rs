// ─── tests/json_repair_tests.rs ──────────────────────────────────────────────
// Covers corners of the LLM-output JSON pipeline that aren't already
// pinned down by `llm_plumbing_tests`: the bracket-closer's string-escape
// handling, sanitiser ordering (top-level wins over nested duplicates),
// and the spawn_agent / send_hint / dismiss branches of the action
// extractor (the plumbing file locks down heat/style/persona/jam_bars).

use crate::llm::LlmAction;
use crate::llm::json_repair::{extract_llm_actions, repair_json, sanitize_json_structure};

// ─── repair_json corner cases ───────────────────────────────────────────────

#[test]
fn repair_json_ignores_brackets_inside_strings() {
    // A `{` or `[` inside a string must not fake-open the depth counter
    // and trigger a phantom close.  This is the classic pitfall of a
    // naive bracket counter.
    let s = r#"{"note": "open { and [ here", "bass": {"cutoff": 0.4}}"#;
    let v = repair_json(s).expect("must parse clean");
    assert_eq!(
        v.get("note").and_then(|n| n.as_str()),
        Some("open { and [ here")
    );
}

#[test]
fn repair_json_trims_trailing_separators_before_closing() {
    // Model sometimes stops mid-comma — the trailing-strip pre-pass is
    // what makes bracket-closing on `{…},` produce valid JSON instead of
    // `{…},}`.
    let s = r#"{"bass": {"cutoff": 0.3},"#;
    let v = repair_json(s).expect("trailing comma should be stripped");
    assert!(v.get("bass").is_some());
}

// ─── sanitize_json_structure — ordering / overwrite rules ───────────────────

#[test]
fn sanitize_does_not_overwrite_existing_top_level_bass() {
    // `entry(...).or_insert` is the correct semantics: if bass already
    // exists at the top level, the nested-inside-sequencer copy must NOT
    // clobber it.  A past bug here had nested bass overwriting the real
    // top-level values.
    let v: serde_json::Value = serde_json::from_str(
        r#"{"bass": {"cutoff": 0.1}, "sequencer": {"bass": {"cutoff": 0.9}}}"#,
    )
    .unwrap();
    let out = sanitize_json_structure(v);
    assert_eq!(
        out.get("bass")
            .and_then(|b| b.get("cutoff"))
            .and_then(|c| c.as_f64()),
        Some(0.1),
        "top-level bass must win over nested copy",
    );
}

#[test]
fn sanitize_dot_notation_lfo_populates_correct_index_slots() {
    // Format A: `"lfo[N].field"` keys.  Must land in slot N specifically —
    // a past bug dropped the index and piled all keys into slot 0.
    let v: serde_json::Value = serde_json::from_str(
        r#"{"lfo": {"lfo[0].enabled": true, "lfo[0].rate": 0.5, "lfo[2].enabled": true}}"#,
    )
    .unwrap();
    let out = sanitize_json_structure(v);
    let arr = out
        .get("lfo")
        .and_then(|l| l.as_array())
        .expect("lfo should be array");
    assert_eq!(arr.len(), 4);
    assert_eq!(arr[0].get("enabled").and_then(|b| b.as_bool()), Some(true));
    assert_eq!(arr[0].get("rate").and_then(|r| r.as_f64()), Some(0.5));
    assert_eq!(arr[2].get("enabled").and_then(|b| b.as_bool()), Some(true));
    // Slot 1 and 3 must be present but empty so downstream indexing
    // never panics.
    assert!(arr[1].as_object().map(|o| o.is_empty()).unwrap_or(false));
    assert!(arr[3].as_object().map(|o| o.is_empty()).unwrap_or(false));
}

// ─── extract_llm_actions: spawn_agent / send_hint / dismiss ─────────────────

#[test]
fn extract_llm_actions_spawn_agent_fills_defaults() {
    let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"settings": {"spawn_agent": {"persona": "BASS", "scope": ["bass"]}}}"#,
    )
    .unwrap();
    let actions = extract_llm_actions(&mut obj);
    let spawn = actions.into_iter().find_map(|a| match a {
        LlmAction::SpawnAgent {
            persona,
            scope,
            model,
            mode,
            tts,
        } => Some((persona, scope, model, mode, tts)),
        _ => None,
    });
    let (persona, scope, model, mode, tts) = spawn.expect("must produce SpawnAgent");
    assert_eq!(persona, "BASS");
    assert_eq!(scope, vec!["bass".to_string()]);
    assert!(model.is_none());
    assert!(mode.is_none());
    assert!(!tts, "tts must default false when not requested");
}

#[test]
fn extract_llm_actions_mc_mode_implies_tts() {
    // mode=="mc" without explicit tts should still spawn with tts=true —
    // MC without a voice is meaningless.  The implicit inference is the
    // whole reason the heuristic exists.
    let mut obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"settings": {"spawn_agent": {"persona": "HOST", "mode": "mc"}}}"#)
            .unwrap();
    let actions = extract_llm_actions(&mut obj);
    let tts = actions.into_iter().find_map(|a| match a {
        LlmAction::SpawnAgent { tts, .. } => Some(tts),
        _ => None,
    });
    assert_eq!(tts, Some(true), "mc mode must imply tts=true");
}

#[test]
fn extract_llm_actions_explicit_tts_false_overrides_mc() {
    // If the model says mode=mc and tts=false explicitly, respect the
    // explicit value — the implicit mc→tts inference only fires when tts
    // is absent.
    let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"settings": {"spawn_agent": {"persona": "HOST", "mode": "mc", "tts": false}}}"#,
    )
    .unwrap();
    let actions = extract_llm_actions(&mut obj);
    let tts = actions.into_iter().find_map(|a| match a {
        LlmAction::SpawnAgent { tts, .. } => Some(tts),
        _ => None,
    });
    assert_eq!(
        tts,
        Some(false),
        "explicit tts=false must override mc inference",
    );
}

#[test]
fn extract_llm_actions_send_hint_requires_both_fields() {
    // to + hint both present → one SendHint action.
    let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"settings": {"send_hint": {"to": "BASS", "hint": "acid it up"}}}"#,
    )
    .unwrap();
    let actions = extract_llm_actions(&mut obj);
    assert!(actions.iter().any(
        |a| matches!(a, LlmAction::SendHint { to, hint } if to == "BASS" && hint == "acid it up"),
    ),);
    // Missing "hint" → no SendHint.
    let mut obj2: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"settings": {"send_hint": {"to": "BASS"}}}"#).unwrap();
    let actions2 = extract_llm_actions(&mut obj2);
    assert!(
        !actions2
            .iter()
            .any(|a| matches!(a, LlmAction::SendHint { .. })),
        "SendHint must require both `to` and `hint`",
    );
}

#[test]
fn extract_llm_actions_broadcast_hint_requires_scope_and_hint() {
    // scope + hint both present → one BroadcastHint action.
    let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"settings": {"broadcast_hint": {"scope": "bass", "hint": "half-time"}}}"#,
    )
    .unwrap();
    let actions = extract_llm_actions(&mut obj);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, LlmAction::BroadcastHint { scope, hint }
            if scope == "bass" && hint == "half-time")),
        "broadcast_hint with scope+hint must produce a BroadcastHint action"
    );

    // Missing scope → no BroadcastHint.
    let mut obj2: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"settings": {"broadcast_hint": {"hint": "go quiet"}}}"#).unwrap();
    assert!(
        !extract_llm_actions(&mut obj2)
            .iter()
            .any(|a| matches!(a, LlmAction::BroadcastHint { .. })),
        "broadcast_hint without scope must be rejected"
    );

    // Empty scope string → no BroadcastHint (broadcast-to-all is
    // admin-only and shouldn't be reachable from LLM JSON).
    let mut obj3: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"settings": {"broadcast_hint": {"scope": "", "hint": "go quiet"}}}"#,
    )
    .unwrap();
    assert!(
        !extract_llm_actions(&mut obj3)
            .iter()
            .any(|a| matches!(a, LlmAction::BroadcastHint { .. })),
        "broadcast_hint with empty scope must be rejected"
    );
}

#[test]
fn extract_llm_actions_dismiss_true_emits_action() {
    let mut obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"settings": {"dismiss": true}}"#).unwrap();
    let actions = extract_llm_actions(&mut obj);
    assert!(actions.iter().any(|a| matches!(a, LlmAction::DismissAgent)));
}

#[test]
fn extract_llm_actions_dismiss_false_emits_nothing() {
    let mut obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"settings": {"dismiss": false}}"#).unwrap();
    let actions = extract_llm_actions(&mut obj);
    assert!(
        !actions.iter().any(|a| matches!(a, LlmAction::DismissAgent)),
        "dismiss=false must NOT emit DismissAgent",
    );
}
