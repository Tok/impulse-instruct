// ─── ui/windows.rs ────────────────────────────────────────────────────────────
// Dispatcher for floating overlay windows.  Each window owns its own file:
// `windows_prefs.rs`, `windows_about.rs`, `windows_sysinfo.rs`, `wizard.rs`.
use crate::ui::ImpulseApp;

impl ImpulseApp {
    /// Draw all floating overlay windows (prefs, about, sysinfo, wizard).
    pub(super) fn draw_windows(&mut self, ctx: &egui::Context) {
        self.draw_prefs_window(ctx);
        self.draw_about_window(ctx);
        self.draw_sysinfo_window(ctx);
        self.draw_wizard_window(ctx);
        self.draw_lane_diff_window(ctx);
        self.draw_undo_timeline_window(ctx);
    }
}
