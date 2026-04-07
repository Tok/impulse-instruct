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
}
