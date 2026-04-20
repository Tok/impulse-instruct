// ─── tests/param_mode_tests.rs ───────────────────────────────────────────────
// Covers the three pure ParamMode helpers in `state/mod.rs`:
//   • `param_mode(path, locked, focused)` — derive the view mode from the
//     two per-param sets.  Locked wins over Focused; both absent = Free.
//   • `cycle_param_mode(state, path)` — Free → UserOwned → LlmFocus → Free.
//   • `set_param_mode(state, path, mode)` — ensures the two sets stay
//     mutually exclusive (no path can simultaneously be Locked + Focused).
//
// These drive the UI's knob-mode toggle (click cycles; right-click sets a
// specific mode) and the LLM's "which params may I touch" gate — getting
// this wrong silently lets the LLM overwrite user-owned knobs or skip
// user-focused ones.

use crate::state::{AppState, ParamMode, cycle_param_mode, param_mode, set_param_mode};
use std::collections::HashSet;

// ─── param_mode ─────────────────────────────────────────────────────────────

#[test]
fn param_mode_defaults_to_free_when_neither_set() {
    let locked = HashSet::new();
    let focused = HashSet::new();
    assert_eq!(
        param_mode("bass.cutoff", &locked, &focused),
        ParamMode::Free
    );
}

#[test]
fn param_mode_locked_wins_over_focused() {
    // The two sets shouldn't normally overlap, but if they do (e.g. a
    // buggy save), `param_mode` must prefer UserOwned so the LLM can't
    // silently overwrite a locked param.
    let mut locked = HashSet::new();
    locked.insert("bass.cutoff".to_string());
    let mut focused = HashSet::new();
    focused.insert("bass.cutoff".to_string());
    assert_eq!(
        param_mode("bass.cutoff", &locked, &focused),
        ParamMode::UserOwned,
    );
}

#[test]
fn param_mode_focused_only_returns_llm_focus() {
    let locked = HashSet::new();
    let mut focused = HashSet::new();
    focused.insert("fx.reverb_mix".to_string());
    assert_eq!(
        param_mode("fx.reverb_mix", &locked, &focused),
        ParamMode::LlmFocus,
    );
}

// ─── cycle_param_mode ───────────────────────────────────────────────────────

#[test]
fn cycle_walks_free_to_locked_to_focused_back_to_free() {
    let s = AppState::default();
    // Starting state: Free.
    assert!(!s.llm.locked_params.contains("bass.cutoff"));
    assert!(!s.llm.focused_params.contains("bass.cutoff"));

    // First click: Free → UserOwned (locked).
    let s = cycle_param_mode(s, "bass.cutoff");
    assert!(s.llm.locked_params.contains("bass.cutoff"));
    assert!(!s.llm.focused_params.contains("bass.cutoff"));

    // Second click: UserOwned → LlmFocus.  Must move out of locked
    // AND into focused in the same step — leaving it in both would
    // let `param_mode` report UserOwned forever.
    let s = cycle_param_mode(s, "bass.cutoff");
    assert!(!s.llm.locked_params.contains("bass.cutoff"));
    assert!(s.llm.focused_params.contains("bass.cutoff"));

    // Third click: LlmFocus → Free.
    let s = cycle_param_mode(s, "bass.cutoff");
    assert!(!s.llm.locked_params.contains("bass.cutoff"));
    assert!(!s.llm.focused_params.contains("bass.cutoff"));
}

#[test]
fn cycle_independent_paths_do_not_interfere() {
    // Cycling bass.cutoff must not touch fx.reverb_mix state.
    let s = AppState::default();
    let s = cycle_param_mode(s, "bass.cutoff"); // → locked
    let s = cycle_param_mode(s, "fx.reverb_mix"); // → locked
    let s = cycle_param_mode(s, "bass.cutoff"); // → focused
    assert!(s.llm.focused_params.contains("bass.cutoff"));
    assert!(!s.llm.focused_params.contains("fx.reverb_mix"));
    assert!(s.llm.locked_params.contains("fx.reverb_mix"));
}

// ─── set_param_mode ─────────────────────────────────────────────────────────

#[test]
fn set_free_clears_both_sets() {
    // set(Free) must remove the path from BOTH locked and focused — any
    // residue would leave param_mode reporting the wrong view.
    let s = AppState::default();
    let s = cycle_param_mode(s, "bass.cutoff"); // locked
    let s = cycle_param_mode(s, "bass.cutoff"); // focused
    // Force both (shouldn't happen normally, but regression guard):
    let mut s = s;
    s.llm.locked_params.insert("bass.cutoff".to_string());
    let s = set_param_mode(s, "bass.cutoff", ParamMode::Free);
    assert!(!s.llm.locked_params.contains("bass.cutoff"));
    assert!(!s.llm.focused_params.contains("bass.cutoff"));
}

#[test]
fn set_user_owned_removes_focus_if_present() {
    // Invariant: Locked ∩ Focused = ∅.  If a path was focused,
    // set(UserOwned) must move it (not duplicate).
    let s = AppState::default();
    let s = set_param_mode(s, "bass.cutoff", ParamMode::LlmFocus);
    assert!(s.llm.focused_params.contains("bass.cutoff"));

    let s = set_param_mode(s, "bass.cutoff", ParamMode::UserOwned);
    assert!(s.llm.locked_params.contains("bass.cutoff"));
    assert!(
        !s.llm.focused_params.contains("bass.cutoff"),
        "set(UserOwned) must clear Focus to preserve mutual exclusion",
    );
}

#[test]
fn set_llm_focus_removes_lock_if_present() {
    // Mirror of the UserOwned test — set(LlmFocus) must clear Locked.
    let s = AppState::default();
    let s = set_param_mode(s, "bass.cutoff", ParamMode::UserOwned);
    assert!(s.llm.locked_params.contains("bass.cutoff"));

    let s = set_param_mode(s, "bass.cutoff", ParamMode::LlmFocus);
    assert!(s.llm.focused_params.contains("bass.cutoff"));
    assert!(
        !s.llm.locked_params.contains("bass.cutoff"),
        "set(LlmFocus) must clear Lock to preserve mutual exclusion",
    );
}

#[test]
fn set_idempotent_for_already_matching_mode() {
    // Setting a path to its current mode is a no-op — no duplicate
    // inserts, no spurious removals of the OTHER set.
    let s = AppState::default();
    let s = set_param_mode(s, "bass.cutoff", ParamMode::UserOwned);
    let s = set_param_mode(s, "bass.cutoff", ParamMode::UserOwned);
    assert_eq!(s.llm.locked_params.len(), 1);
    assert!(s.llm.locked_params.contains("bass.cutoff"));
}
