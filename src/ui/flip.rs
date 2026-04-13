// ─── ui/flip.rs ──────────────────────────────────────────────────────────────
// Rack flip helper — alternates scroll target (master/agent) on flip-to-back
// so the user sees audio cables first, then control cables on next flip.

use crate::ui::ImpulseApp;

impl ImpulseApp {
    /// Flip rack; on flip-to-back, alternate scroll (master/agent) to show cables.
    pub(crate) fn toggle_rack_flip(&mut self) {
        self.rack_flipped = !self.rack_flipped;
        self.session_dirty = true;
        if self.rack_flipped {
            let t = if self.flip_to_back_count.is_multiple_of(2) {
                "master"
            } else {
                "agent"
            };
            self.state.write().scroll_target = Some(t.to_string());
            self.flip_to_back_count = self.flip_to_back_count.wrapping_add(1);
        }
    }
}
