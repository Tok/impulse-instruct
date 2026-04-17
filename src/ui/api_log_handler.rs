// ─── ui/api_log_handler.rs ── drain API log messages into the UI ─────────────
// Uses a lock-free crossbeam channel (no write lock on AppState).
// Deduplicates consecutive identical messages with a repeat counter.

use super::ImpulseApp;

/// Soft cap on the structured activity-log ring — when exceeded, the oldest
/// `ACTIVITY_LOG_DRAIN` entries are dropped.  Cap/drain together give a ~4×
/// churn cushion so we don't Vec::drain on every push.
pub(crate) const ACTIVITY_LOG_CAP: usize = 500;
pub(crate) const ACTIVITY_LOG_DRAIN: usize = 100;

/// A single entry in the structured activity log.
#[derive(Clone)]
pub(crate) struct ActivityEntry {
    pub timestamp: std::time::Instant,
    pub persona: String,
    pub action: ActivityAction,
    pub detail: String,
}
#[derive(Clone, PartialEq)]
#[allow(dead_code)] // variants populated incrementally as more log sources are wired
pub(crate) enum ActivityAction {
    Response,    // normal LLM response
    Thinking,    // chain-of-thought
    ParamUpdate, // parameter change applied
    Spawn,       // agent spawned
    Dismiss,     // agent dismissed
    UserPrompt,  // user typed a prompt
    System,      // system message (startup, error)
}

impl ImpulseApp {
    /// Drain log messages from the API→UI channel and display them in the log.
    /// Consecutive identical messages are collapsed into "msg (xN)".
    pub(crate) fn drain_api_log(&mut self) {
        while let Ok(msg) = self.api_log_rx.try_recv() {
            log::info!("{}", msg);
            if msg == self.last_log_line {
                // Same message again — update the repeat counter in-place
                self.log_repeat_count += 1;
                // Remove the previous line from log_text and replace it
                if let Some(last_nl) = self.log_text.rfind('\n') {
                    // Find the second-to-last newline (start of previous line)
                    let prev_nl = self.log_text[..last_nl].rfind('\n');
                    let start = prev_nl.map(|p| p + 1).unwrap_or(0);
                    self.log_text.truncate(start);
                }
                self.log_text
                    .push_str(&format!("{} (x{})\n", msg, self.log_repeat_count));
            } else {
                // New message
                self.last_log_line = msg.clone();
                self.log_repeat_count = 1;
                self.log_text.push_str(&msg);
                self.log_text.push('\n');
            }
            self.activity_log.push(ActivityEntry {
                timestamp: std::time::Instant::now(),
                persona: "API".to_string(),
                action: ActivityAction::ParamUpdate,
                detail: msg,
            });
        }
        self.trim_activity_log();
    }

    /// Drop the oldest `ACTIVITY_LOG_DRAIN` entries once the log grows past
    /// `ACTIVITY_LOG_CAP`.  Kept as a method rather than inlined so both
    /// producers (API log + LLM drain) stay in sync on the policy.
    pub(crate) fn trim_activity_log(&mut self) {
        if self.activity_log.len() > ACTIVITY_LOG_CAP {
            self.activity_log.drain(..ACTIVITY_LOG_DRAIN);
        }
    }
}
