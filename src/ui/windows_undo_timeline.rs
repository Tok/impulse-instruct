// ─── ui/windows_undo_timeline.rs ──────────────────────────────────────────────
// Undo / redo timeline scrubber — surfaces the existing `StateHistory`
// stack as a horizontal slider so users can A/B compare past states
// visually instead of mashing Ctrl-Z blind.
//
// V1 scope:
//   • Linear slider over `total_slots`, value = `current_index`.
//   • Drag the slider to scrub through past + future.
//   • Undo / Redo buttons for one-step nudges.
//   • Position readout (e.g. "12 / 30") + clear-future hint when
//     the user is mid-history (any new mutation would clear future).
//   • Toggle from the header view menu.
//
// Off by default — the keyboard shortcuts (Ctrl+Z / Ctrl+Shift+Z)
// stay primary; this window is for occasional comparison.

use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    pub(super) fn draw_undo_timeline_window(&mut self, ctx: &egui::Context) {
        if !self.show_undo_timeline {
            return;
        }
        let mut open = self.show_undo_timeline;
        egui::Window::new("Undo Timeline")
            .collapsible(true)
            .resizable(true)
            .default_width(420.0)
            .default_height(120.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let total = self.history.total_slots();
                let cur = self.history.current_index();
                if total <= 1 {
                    ui.label(
                        egui::RichText::new(
                            "No history yet — touch a knob or run an LLM turn to populate.",
                        )
                        .monospace()
                        .size(9.0)
                        .color(theme::IRON),
                    );
                    return;
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.history.can_undo(),
                            egui::Button::new(egui::RichText::new("↶ Undo").monospace().size(9.5)),
                        )
                        .clicked()
                    {
                        let now = self.state.read().clone();
                        if let Some(prev) = self.history.undo(now) {
                            *self.state.write() = prev;
                            self.session_dirty = true;
                        }
                    }
                    if ui
                        .add_enabled(
                            self.history.can_redo(),
                            egui::Button::new(egui::RichText::new("↷ Redo").monospace().size(9.5)),
                        )
                        .clicked()
                    {
                        let now = self.state.read().clone();
                        if let Some(next) = self.history.redo(now) {
                            *self.state.write() = next;
                            self.session_dirty = true;
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!("position {} / {}", cur, total - 1))
                            .monospace()
                            .size(8.5)
                            .color(theme::SMOKE),
                    );
                });
                ui.add_space(4.0);
                let mut target = cur as u32;
                let max_idx = (total - 1) as u32;
                if ui
                    .add(
                        egui::Slider::new(&mut target, 0..=max_idx)
                            .show_value(false)
                            .text("history"),
                    )
                    .changed()
                {
                    let now = self.state.read().clone();
                    if let Some(new_state) = self.history.scrub_to(target as usize, now) {
                        *self.state.write() = new_state;
                        self.session_dirty = true;
                    }
                }
                ui.add_space(2.0);
                if self.history.can_redo() {
                    ui.label(
                        egui::RichText::new(
                            "  Mid-history — any new edit clears the future entries.",
                        )
                        .monospace()
                        .size(8.0)
                        .color(theme::IRON),
                    );
                }
            });
        self.show_undo_timeline = open;
    }
}
