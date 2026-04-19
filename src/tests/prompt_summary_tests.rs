// ─── tests/prompt_summary_tests.rs ───────────────────────────────────────────
// `prompt_summary.rs` has inline tests for three of its four helpers
// (voices/groove/rack-coverage).  This file fills in the missing
// `bass_active_steps_summary` coverage and pins a few edge cases that
// the inline tests don't assert — specifically the "silent" sentinel
// and the multi-voice directive's per-voice key spelling.

use crate::llm::prompt_summary::{
    bass_active_steps_summary, bass_groove_summary, bass_voices_summary,
};
use crate::state::AppState;

// ─── bass_active_steps_summary ──────────────────────────────────────────────

#[test]
fn active_steps_summary_says_none_silent_when_empty() {
    // Default state has no active bass steps → "none (silent)" exact
    // sentinel.  The LLM prompt reads this literal string to decide
    // whether to generate a pattern from scratch.
    let s = AppState::default();
    assert_eq!(bass_active_steps_summary(&s), "none (silent)");
}

#[test]
fn active_steps_summary_lists_indices_in_order() {
    // Indices must appear in ascending order (walk the pattern L→R) so
    // the prompt reads consistently across saves.
    let mut s = AppState::default();
    s.sequencer.bass_pattern[0].active = true;
    s.sequencer.bass_pattern[7].active = true;
    s.sequencer.bass_pattern[4].active = true;
    let out = bass_active_steps_summary(&s);
    assert_eq!(out, "0, 4, 7");
}

#[test]
fn active_steps_summary_single_hit_has_no_comma() {
    // One active step → just the number, no trailing comma.
    let mut s = AppState::default();
    s.sequencer.bass_pattern[3].active = true;
    assert_eq!(bass_active_steps_summary(&s), "3");
}

// ─── bass_voices_summary — edge cases ───────────────────────────────────────

#[test]
fn voices_summary_reports_zero_active_cleanly() {
    // All voices disabled should still produce a readable line with
    // count (0 of N) and an empty bracket — not a panic, not an empty
    // string.
    let mut s = AppState::default();
    for v in &mut s.bass_voices {
        v.enabled = false;
    }
    let out = bass_voices_summary(&s);
    assert!(out.starts_with("Active bass voices (0 of"));
    assert!(out.contains("[]"));
    // No multi-voice rule because count < 2.
    assert!(!out.contains("MULTI-VOICE"));
}

#[test]
fn voices_summary_three_voices_lists_all_three_key_pairs() {
    // Each active voice N (>0) contributes a `bassN+1_steps+bassN+1_notes`
    // key pair — "AND"-joined in the directive.  Voice 0 keeps the
    // legacy unnumbered form.
    let mut s = AppState::default();
    s.bass_voices[0].enabled = true;
    s.bass_voices[1].enabled = true;
    s.bass_voices[2].enabled = true;
    let out = bass_voices_summary(&s);
    assert!(out.contains("bass_steps+bass_notes"));
    assert!(out.contains("bass2_steps+bass2_notes"));
    assert!(out.contains("bass3_steps+bass3_notes"));
    assert!(out.contains("DISTINCT pattern"));
}

// ─── bass_groove_summary — active-only filtering ────────────────────────────

#[test]
fn groove_summary_ignores_accent_on_inactive_steps() {
    // An accent / slide value on an INACTIVE step is meaningless (voice
    // doesn't fire), so it must not appear in the summary.  This guards
    // the filter predicate `s.active && s.accent > 0.0`.
    let mut s = AppState::default();
    s.sequencer.bass_pattern[0].active = true;
    s.sequencer.bass_pattern[0].accent = 1.0;
    // Step 5: has an accent BUT is inactive — shouldn't show up.
    s.sequencer.bass_pattern[5].active = false;
    s.sequencer.bass_pattern[5].accent = 1.0;
    let out = bass_groove_summary(&s);
    assert!(out.contains("Accent steps: 0"));
    assert!(
        !out.contains(", 5") && !out.contains("5,"),
        "accent on inactive step should not leak into summary: {out}",
    );
}
