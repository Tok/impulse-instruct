// ─── ui/panels/scope_module.rs ───────────────────────────────────────────────
// Bar oscilloscope as a rack module.  Mirrors the header
// `draw_scope_colored` widget — same phosphor-trail rendering — but
// lives inside an FX/Mod-zone module card so the user can park a
// scope anywhere in the rack instead of the always-on header slot.

use crate::ui::ImpulseApp;

pub fn draw_scope_module(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let avail_w = ui.available_width().max(80.0);
    let avail_h = ui.available_height().max(48.0);
    crate::ui::scope_footer::draw_scope_colored(
        ui,
        &app.scope_buf,
        &app.scope_history,
        avail_w,
        avail_h,
        None,
    );
}
