// ─── state/patch_morph.rs ─────────────────────────────────────────────────────
// AI patch morph — schedule a sequence of LLM "nudge" prompts that
// evolve the FX chain (or any other LLM-controlled state) along a
// textual prompt across N bars.  Absurd-queue feature #4.
//
// The state lives here; the actual scheduling lives in
// `ui::patch_morph_handler`, which polls `global_step_count` once per
// UI tick and fires the next nudge when the bar-step interval has
// passed.  This struct is `#[serde(skip)]` on AppState because morph
// progress is ephemeral — reloading a session shouldn't resurrect a
// half-finished morph.
//
// Distinct from `ChainMorph` (sibling `morph.rs`), which crossfades
// step *patterns* on the audio thread.  This one nudges *parameters*
// from the UI thread via the LLM — different mechanism, different
// surface, intentionally separate file so the two can evolve without
// stepping on each other.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PatchMorphState {
    /// Is a morph currently in progress?  When false every other
    /// field is irrelevant — the UI tick early-exits.
    pub active: bool,
    /// The user-provided morph prompt.  The scheduler appends a
    /// progress line ("step 3 of 8") when sending each nudge to the
    /// LLM, so the model knows where in the arc it is.
    pub prompt: String,
    /// Total number of LLM nudges to fire across the morph
    /// (typically equals `bars` — one nudge per bar).
    pub total_calls: u32,
    /// Calls already fired since `start`.
    pub calls_done: u32,
    /// `global_step_count` snapshot at morph start.
    pub start_global_step: u64,
    /// `global_step_count` value when the most recent nudge fired.
    /// Next nudge fires when `global_step_count >=
    /// last_step_fired + step_interval`.
    pub last_step_fired: u64,
    /// Number of `global_step_count` ticks between consecutive
    /// LLM nudges — pre-computed at morph start so the per-tick
    /// poll doesn't need to recompute against the current
    /// `step_division`.
    pub step_interval: u64,
}

impl PatchMorphState {
    /// True when the morph has work left to do — `active` is true
    /// and at least one more call is still queued.  `tick_patch_morph`
    /// uses this to decide whether to fire / decrement / deactivate.
    pub fn in_progress(&self) -> bool {
        self.active && self.calls_done < self.total_calls
    }

    /// Format the next morph nudge prompt with progress context so
    /// the LLM knows where in the arc it is.  Pure — same inputs
    /// always produce the same output, makes the formatting
    /// trivially testable.  Includes both the absolute step
    /// number and the original prompt so the model has both
    /// "where am I" and "what am I doing".
    pub fn next_nudge_prompt(&self) -> String {
        // calls_done is incremented *after* sending, so the call
        // we're about to send is `calls_done + 1` of `total_calls`.
        let n = self.calls_done + 1;
        let total = self.total_calls.max(1);
        format!(
            "Patch-morph step {n} of {total}: nudge the FX / params \
             one small step toward the target — \"{}\".  Make the \
             change incremental, not the final destination.",
            self.prompt
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_progress_requires_active_and_remaining_calls() {
        let mut m = PatchMorphState::default();
        assert!(!m.in_progress(), "default is inert");
        m.active = true;
        m.total_calls = 4;
        m.calls_done = 0;
        assert!(m.in_progress(), "active + remaining → in progress");
        m.calls_done = 4;
        assert!(
            !m.in_progress(),
            "done with all calls → no longer in progress"
        );
        m.calls_done = 5;
        assert!(!m.in_progress(), "over-done is still done, not progressing");
        m.calls_done = 0;
        m.active = false;
        assert!(!m.in_progress(), "inactive ignores remaining calls");
    }

    #[test]
    fn next_nudge_prompt_includes_progress_and_user_prompt() {
        let m = PatchMorphState {
            active: true,
            prompt: "evolve from cathedral to dystopia".into(),
            total_calls: 8,
            calls_done: 2,
            ..Default::default()
        };
        let s = m.next_nudge_prompt();
        // calls_done=2 → next is call 3 of 8.
        assert!(s.contains("step 3 of 8"), "missing progress: {s}");
        assert!(
            s.contains("evolve from cathedral to dystopia"),
            "missing user prompt: {s}"
        );
        assert!(s.contains("incremental"), "missing nudge instruction: {s}");
    }

    #[test]
    fn next_nudge_prompt_clamps_zero_total_calls() {
        // Defensive: a state with total_calls = 0 (caller bug)
        // shouldn't divide by zero or print "step 1 of 0".
        let m = PatchMorphState {
            total_calls: 0,
            calls_done: 0,
            ..Default::default()
        };
        let s = m.next_nudge_prompt();
        assert!(s.contains("step 1 of 1"), "got: {s}");
    }
}
