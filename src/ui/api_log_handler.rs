// ─── ui/api_log_handler.rs ── drain API log messages into the UI ─────────────
// Extracted from mod.rs to stay under the 1000-line limit.

use super::{ActivityAction, ActivityEntry, ImpulseApp};

impl ImpulseApp {
    /// Drain log messages pushed by the HTTP API and display them in the UI log.
    pub(crate) fn drain_api_log(&mut self) {
        let messages: Vec<String> = {
            let mut s = self.state.write();
            s.api_log.drain(..).collect()
        };
        for msg in messages {
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
