// ─── ui/undo.rs ──────────────────────────────────────────────────────────────
// Ring-buffer undo/redo stack for AppState snapshots.

const HISTORY_DEPTH: usize = 50;

pub(super) struct StateHistory {
    past: std::collections::VecDeque<crate::state::AppState>,
    future: Vec<crate::state::AppState>,
}

impl StateHistory {
    pub fn new() -> Self {
        Self {
            past: std::collections::VecDeque::with_capacity(HISTORY_DEPTH),
            future: Vec::new(),
        }
    }

    /// Record a snapshot before a mutation. Clears redo stack.
    pub fn push(&mut self, snapshot: crate::state::AppState) {
        if self.past.len() >= HISTORY_DEPTH {
            self.past.pop_front();
        }
        self.past.push_back(snapshot);
        self.future.clear();
    }

    /// Undo: restore previous state, push current to redo stack.
    pub fn undo(&mut self, current: crate::state::AppState) -> Option<crate::state::AppState> {
        let prev = self.past.pop_back()?;
        self.future.push(current);
        Some(prev)
    }

    /// Redo: re-apply a previously undone change.
    pub fn redo(&mut self, current: crate::state::AppState) -> Option<crate::state::AppState> {
        let next = self.future.pop()?;
        if self.past.len() >= HISTORY_DEPTH {
            self.past.pop_front();
        }
        self.past.push_back(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Index of the "current" state in the linear timeline.
    /// `past = [0..current_index]`, current = `current_index`, future
    /// = `(current_index+1) .. total_slots`.  Used by the timeline
    /// scrubber UI to render a position marker without exposing the
    /// internal past/future split.
    pub fn current_index(&self) -> usize {
        self.past.len()
    }

    /// Total slots in the linearised timeline — past + current + future.
    pub fn total_slots(&self) -> usize {
        self.past.len() + 1 + self.future.len()
    }

    /// Jump to an arbitrary point in the timeline.  `target` is the
    /// linearised index produced by `current_index` semantics:
    ///
    /// * `target == current_index()` is a no-op (returns `None`).
    /// * `target < current_index()` walks `target.distance` undos,
    ///   pushing the running state into the future stack each time.
    /// * `target > current_index()` walks the corresponding number
    ///   of redos, pulling from `future` and pushing onto `past`.
    /// * Out-of-range targets clamp to the timeline ends.
    ///
    /// Returns the new "current" state when a jump happened, or
    /// `None` when no movement was needed (target equalled current
    /// or both ends were already empty).
    pub fn scrub_to(
        &mut self,
        target: usize,
        current: crate::state::AppState,
    ) -> Option<crate::state::AppState> {
        let total = self.total_slots();
        if total == 0 {
            return None;
        }
        let target = target.min(total - 1);
        let cur = self.current_index();
        if target == cur {
            return None;
        }
        // Walk one step at a time so the past / future stacks
        // remain coherent (each undo / redo records the running
        // state on the opposite side).
        let mut running = current;
        if target < cur {
            for _ in 0..(cur - target) {
                if let Some(prev) = self.undo(running.clone()) {
                    running = prev;
                } else {
                    break;
                }
            }
        } else {
            for _ in 0..(target - cur) {
                if let Some(next) = self.redo(running.clone()) {
                    running = next;
                } else {
                    break;
                }
            }
        }
        Some(running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

    fn s_with_bpm(bpm: f32) -> AppState {
        let mut s = AppState::default();
        s.sequencer.bpm = bpm;
        s
    }

    #[test]
    fn empty_history_total_slots_is_one() {
        // No past, no future — current alone counts as one slot.
        let h = StateHistory::new();
        assert_eq!(h.total_slots(), 1);
        assert_eq!(h.current_index(), 0);
        assert!(!h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn current_index_walks_with_push_and_undo() {
        let mut h = StateHistory::new();
        h.push(s_with_bpm(100.0));
        h.push(s_with_bpm(110.0));
        h.push(s_with_bpm(120.0));
        // 3 past entries + current → index 3, total 4.
        assert_eq!(h.current_index(), 3);
        assert_eq!(h.total_slots(), 4);
        // Undo once → past=2, future=1.
        let now = s_with_bpm(130.0);
        let prev = h.undo(now).unwrap();
        assert!((prev.sequencer.bpm - 120.0).abs() < 1e-6);
        assert_eq!(h.current_index(), 2);
        assert_eq!(h.total_slots(), 4);
    }

    #[test]
    fn scrub_to_current_returns_none() {
        let mut h = StateHistory::new();
        h.push(s_with_bpm(100.0));
        let cur = h.current_index();
        let now = s_with_bpm(110.0);
        assert!(h.scrub_to(cur, now).is_none());
    }

    #[test]
    fn scrub_back_walks_undos_and_returns_target_state() {
        let mut h = StateHistory::new();
        h.push(s_with_bpm(100.0));
        h.push(s_with_bpm(110.0));
        h.push(s_with_bpm(120.0));
        let now = s_with_bpm(130.0);
        // Jump to index 0 — should land on bpm=100.
        let target = h.scrub_to(0, now).unwrap();
        assert!((target.sequencer.bpm - 100.0).abs() < 1e-6);
        assert_eq!(h.current_index(), 0);
        assert_eq!(h.total_slots(), 4); // future now full
        assert!(!h.can_undo());
        assert!(h.can_redo());
    }

    #[test]
    fn scrub_forward_walks_redos() {
        let mut h = StateHistory::new();
        h.push(s_with_bpm(100.0));
        h.push(s_with_bpm(110.0));
        h.push(s_with_bpm(120.0));
        let mut running = s_with_bpm(130.0);
        // Walk all the way back.
        running = h.scrub_to(0, running).unwrap();
        // Now scrub forward to the latest.
        let final_idx = h.total_slots() - 1;
        let target = h.scrub_to(final_idx, running).unwrap();
        assert!((target.sequencer.bpm - 130.0).abs() < 1e-6);
        assert_eq!(h.current_index(), final_idx);
        assert!(!h.can_redo());
    }

    #[test]
    fn scrub_clamps_out_of_range_target() {
        let mut h = StateHistory::new();
        h.push(s_with_bpm(100.0));
        h.push(s_with_bpm(110.0));
        let now = s_with_bpm(120.0);
        // Target way past the end clamps to the latest slot —
        // current is already there, so no movement needed.
        let total = h.total_slots(); // = 3
        let cur = h.current_index();
        assert_eq!(cur, total - 1);
        assert!(h.scrub_to(99999, now).is_none());
    }

    #[test]
    fn push_after_scrub_back_clears_future() {
        // Mid-history mutation must drop the redo stack — guards
        // against confusing "redo lost between an edit and the
        // expected next state" UX.
        let mut h = StateHistory::new();
        h.push(s_with_bpm(100.0));
        h.push(s_with_bpm(110.0));
        let mut running = s_with_bpm(120.0);
        running = h.scrub_to(0, running).unwrap();
        assert!(h.can_redo());
        h.push(running.clone());
        assert!(!h.can_redo());
    }
}
