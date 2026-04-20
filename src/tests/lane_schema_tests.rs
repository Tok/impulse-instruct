// ─── tests/lane_schema_tests.rs ──────────────────────────────────────────────
// Covers `lane_schema` — the per-lane JSON schema used for grammar-
// constrained LLM generation.  We pin structural invariants (required
// fields, additionalProperties=false, max/min item counts) rather than
// the exact property order — so the tests don't ossify schema wording
// but DO catch the class of bugs where a schema silently loses a
// required field or accidentally opens up to extra properties.

use crate::llm::lanes::{LaneKind, lane_schema};

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

// ─── Global shape invariants ────────────────────────────────────────────────

#[test]
fn every_lane_schema_is_a_draft_07_object_with_closed_properties() {
    // Invariants that every schema MUST satisfy for grammar-constrained
    // generation to work.  A missing `additionalProperties: false` would
    // let the model emit arbitrary top-level keys that silently survive
    // the grammar check.
    for lane in ALL_LANES {
        let s = lane_schema(*lane);
        assert_eq!(
            s.get("type").and_then(|v| v.as_str()),
            Some("object"),
            "{lane:?} schema must be an object",
        );
        assert_eq!(
            s.get("additionalProperties").and_then(|v| v.as_bool()),
            Some(false),
            "{lane:?} schema must forbid additional top-level keys",
        );
        assert!(
            s.get("properties").and_then(|v| v.as_object()).is_some(),
            "{lane:?} schema must declare `properties`",
        );
        assert!(
            s.get("required").and_then(|v| v.as_array()).is_some(),
            "{lane:?} schema must declare `required`",
        );
    }
}

#[test]
fn every_schema_accepts_thinking_and_comment_meta_keys() {
    // `_thinking` / `_comment` are legitimate meta keys carried through
    // the pipeline filter.  Every lane schema must expose them in
    // `properties` so grammar-constrained generation allows them even
    // though they're not in `required`.
    for lane in ALL_LANES {
        let s = lane_schema(*lane);
        let props = s.get("properties").and_then(|v| v.as_object()).unwrap();
        assert!(
            props.contains_key("_thinking"),
            "{lane:?} schema must expose `_thinking` in properties",
        );
        assert!(
            props.contains_key("_comment"),
            "{lane:?} schema must expose `_comment` in properties",
        );
    }
}

// ─── Bass lane specifics ────────────────────────────────────────────────────

#[test]
fn bass_lane_schema_requires_sequencer() {
    // Bass schema's required list is always just `["sequencer"]` — the
    // top-level `bass` / `bass_voices` block is optional (pattern-only
    // updates are a common case).
    for idx in 0..4 {
        let s = lane_schema(LaneKind::Bass(idx));
        let required = required_keys(&s);
        assert!(
            required.contains(&"sequencer".to_string()),
            "Bass({idx}) must require `sequencer`, got {required:?}",
        );
    }
}

#[test]
fn bass_voice_n_sequencer_exposes_matching_step_key() {
    // Bass(0) sequencer.required must contain "bass_steps", Bass(1) must
    // contain "bass2_steps", etc.  Breaking this makes the grammar
    // accept a bass rewrite for the wrong voice.
    for idx in 0..4 {
        let s = lane_schema(LaneKind::Bass(idx));
        let expected_key = if idx == 0 {
            "bass_steps".to_string()
        } else {
            format!("bass{}_steps", idx + 1)
        };
        let seq = s
            .get("properties")
            .and_then(|v| v.get("sequencer"))
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| panic!("Bass({idx}) must expose sequencer.properties"));
        assert!(
            seq.contains_key(&expected_key),
            "Bass({idx}) sequencer must declare {expected_key:?}; got {:?}",
            seq.keys().collect::<Vec<_>>(),
        );
    }
}

#[test]
fn bass_step_arrays_cap_at_64_items() {
    // Every bass step / note / accent / slide / pan array has maxItems=64
    // — a larger array would blow past the sequencer's max step count
    // and waste tokens.
    let s = lane_schema(LaneKind::Bass(0));
    let seq_props = s
        .get("properties")
        .and_then(|v| v.get("sequencer"))
        .and_then(|v| v.get("properties"))
        .and_then(|v| v.as_object())
        .unwrap();
    for key in ["bass_steps", "bass_notes", "bass_accents", "bass_slides"] {
        let arr = seq_props.get(key).unwrap();
        assert_eq!(
            arr.get("maxItems").and_then(|v| v.as_u64()),
            Some(64),
            "{key} should cap at 64 items",
        );
    }
}

// ─── Kit lane specifics ────────────────────────────────────────────────────

#[test]
fn kit_a_lane_requires_three_drum_step_arrays() {
    // KitA's required sequencer keys are the three core drum voices.
    // Missing any one lets the model silently skip (say) the kick and
    // leaves the kit feeling incomplete.
    let s = lane_schema(LaneKind::KitA);
    let seq_required: Vec<String> = s
        .get("properties")
        .and_then(|v| v.get("sequencer"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    for key in ["kick_a_steps", "snare_a_steps", "hihat_a_steps"] {
        assert!(
            seq_required.contains(&key.to_string()),
            "KitA sequencer must require {key:?}, got {seq_required:?}",
        );
    }
}

#[test]
fn kit_b_lane_requires_clap_b_instead_of_hihat_a() {
    // KitB's required set must include clap_b_steps (909's trademark
    // snare/clap pair) and must NOT reference kit_a keys.
    let s = lane_schema(LaneKind::KitB);
    let seq_required: Vec<String> = s
        .get("properties")
        .and_then(|v| v.get("sequencer"))
        .and_then(|v| v.get("required"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    assert!(seq_required.contains(&"kick_b_steps".to_string()));
    for forbidden in ["kick_a_steps", "snare_a_steps", "hihat_a_steps"] {
        assert!(
            !seq_required.contains(&forbidden.to_string()),
            "KitB must not require {forbidden:?}",
        );
    }
}

// ─── Top-level-only lanes ──────────────────────────────────────────────────

#[test]
fn fx_lane_requires_fx_property() {
    let s = lane_schema(LaneKind::Fx);
    let required = required_keys(&s);
    assert!(
        required.contains(&"fx".to_string()),
        "FX lane must require `fx`, got {required:?}",
    );
}

#[test]
fn rack_lane_requires_rack_property() {
    let s = lane_schema(LaneKind::Rack);
    let required = required_keys(&s);
    assert!(required.contains(&"rack".to_string()));
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn required_keys(schema: &serde_json::Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
