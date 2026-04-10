// ─── ui/api_log_handler.rs ── drain API log messages into the UI ─────────────
// Uses a lock-free crossbeam channel (no write lock on AppState).

use super::{ActivityAction, ActivityEntry, ImpulseApp};

impl ImpulseApp {
    /// Drain log messages from the API→UI channel and display them in the log.
    pub(crate) fn drain_api_log(&mut self) {
        while let Ok(msg) = self.api_log_rx.try_recv() {
            log::info!("{}", msg);
            self.log_text.push_str(&msg);
            self.log_text.push('\n');
            self.activity_log.push(ActivityEntry {
                timestamp: std::time::Instant::now(),
                persona: "API".to_string(),
                action: ActivityAction::ParamUpdate,
                detail: msg,
            });
        }
        if self.activity_log.len() > 500 {
            self.activity_log.drain(..100);
        }
    }
}
