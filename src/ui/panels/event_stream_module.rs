// ─── ui/panels/event_stream_module.rs ────────────────────────────────────────
// Event stream as a rack module.  Mirrors the header `event_stream`
// widget — scrolling note / drum activity at tempo — but rendered
// inside an FX/Mod-zone module card so users can park it next to
// whichever voice / FX cluster they're editing.

use crate::ui::ImpulseApp;

pub fn draw_event_stream_module(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let avail_w = ui.available_width().max(80.0);
    let avail_h = ui.available_height().max(48.0);
    let now = ui.ctx().input(|i| i.time);
    let bpm = app.state.read().sequencer.bpm.max(20.0);
    let secs_per_step = 60.0 / (bpm as f64 * 4.0);
    let frac = if app.last_step_time > 0.0 {
        ((now - app.last_step_time) / secs_per_step).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let smooth_global = app.last_step_global as f64 + frac;
    let state = app.state.read();
    crate::ui::widgets::event_stream(
        ui,
        &state,
        smooth_global,
        &app.melodic_log,
        &app.drum_log,
        avail_w,
        avail_h,
    );
}
