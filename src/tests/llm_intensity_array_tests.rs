// ─── tests/llm_intensity_array_tests.rs ──────────────────────────────────────
// Covers `apply_llm_intensity_array` — the bass_accents / bass_slides wire
// format decoder.  Three wire formats are accepted; this file boxes each
// one in so a future tweak to the "looks like an index list" heuristic
// can't silently change how the LLM's output maps onto the sequencer.

use crate::state::TB303Step;
use crate::state::llm_apply::apply_llm_intensity_array;

fn blank_pattern(n: usize) -> Vec<TB303Step> {
    (0..n).map(|_| TB303Step::default()).collect()
}

fn set_accent(step: &mut TB303Step, v: f32) {
    step.accent = v;
}

#[test]
fn empty_array_clears_every_step() {
    let mut pat = blank_pattern(16);
    // Populate so we can verify the empty-array path actually writes 0.0
    // rather than just leaving whatever was there.
    for step in &mut pat {
        step.accent = 0.7;
    }
    apply_llm_intensity_array(&[], &mut pat, set_accent);
    assert!(pat.iter().all(|s| s.accent == 0.0));
}

#[test]
fn short_index_list_sets_listed_steps_to_one() {
    let mut pat = blank_pattern(16);
    // First fill everything so the clear-before-set phase is visible:
    // listed indices become 1.0, everything else becomes 0.0 (not
    // whatever they were).
    for step in &mut pat {
        step.accent = 0.5;
    }
    let arr = vec![
        serde_json::json!(0),
        serde_json::json!(4),
        serde_json::json!(8),
        serde_json::json!(12),
    ];
    apply_llm_intensity_array(&arr, &mut pat, set_accent);
    for (i, step) in pat.iter().enumerate() {
        let expected = if [0, 4, 8, 12].contains(&i) { 1.0 } else { 0.0 };
        assert_eq!(step.accent, expected, "step {i}");
    }
}

#[test]
fn out_of_range_index_is_dropped() {
    let mut pat = blank_pattern(8);
    let arr = vec![serde_json::json!(0), serde_json::json!(99)];
    // Index 99 is past the pattern length; must not panic and must not
    // set anything.  Index 0 still lands.
    apply_llm_intensity_array(&arr, &mut pat, set_accent);
    assert_eq!(pat[0].accent, 1.0);
    for step in &pat[1..] {
        assert_eq!(step.accent, 0.0);
    }
}

#[test]
fn inline_sixteen_element_array_writes_per_step_floats() {
    let mut pat = blank_pattern(16);
    // 16 floats → inline mode (≥ 16 triggers the inline path even for
    // all-integer values).  Mixed values + out-of-range floats clamp.
    let arr = vec![
        serde_json::json!(0.0),
        serde_json::json!(0.25),
        serde_json::json!(0.5),
        serde_json::json!(0.75),
        serde_json::json!(1.0),
        serde_json::json!(1.5),  // clamps to 1.0
        serde_json::json!(-0.5), // clamps to 0.0
        serde_json::json!(0.3),
        serde_json::json!(0.4),
        serde_json::json!(0.5),
        serde_json::json!(0.6),
        serde_json::json!(0.7),
        serde_json::json!(0.8),
        serde_json::json!(0.9),
        serde_json::json!(1.0),
        serde_json::json!(0.0),
    ];
    apply_llm_intensity_array(&arr, &mut pat, set_accent);
    assert_eq!(pat[0].accent, 0.0);
    assert_eq!(pat[4].accent, 1.0);
    assert_eq!(pat[5].accent, 1.0, "1.5 should clamp down");
    assert_eq!(pat[6].accent, 0.0, "-0.5 should clamp up");
    assert!((pat[2].accent - 0.5).abs() < 1e-4);
}

#[test]
fn inline_bools_translate_to_zero_and_one() {
    let mut pat = blank_pattern(16);
    let mut arr: Vec<serde_json::Value> = (0..16).map(|i| serde_json::json!(i % 2 == 0)).collect();
    // Flip one value to int to confirm both branches of the inline type
    // dispatch fire — bool → 0/1 and u64 → 0/1.
    arr[1] = serde_json::json!(1);
    apply_llm_intensity_array(&arr, &mut pat, set_accent);
    assert_eq!(pat[0].accent, 1.0);
    assert_eq!(pat[1].accent, 1.0);
    assert_eq!(pat[2].accent, 1.0);
    assert_eq!(pat[3].accent, 0.0);
}
