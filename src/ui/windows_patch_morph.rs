// ─── ui/windows_patch_morph.rs ────────────────────────────────────────────────
// AI patch-morph dialog — small modal exposing the same `POST /api/morph`
// flow from the menu (deferred V2 follow-up: V1 was API-only).  User types
// a target prompt + picks bars / calls; on Start we route through
// `PatchMorphState::start` so the dialog and the API share one path.
//
// Live status: when a morph is already in progress, the dialog reads
// `state.patch_morph` and shows a progress line with a Cancel button
// instead of the input form.

use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    pub(super) fn draw_patch_morph_window(&mut self, ctx: &egui::Context) {
        if !self.show_patch_morph {
            return;
        }
        let in_progress = self.state.read().patch_morph.in_progress();
        egui::Window::new("AI Patch Morph")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                if in_progress {
                    self.draw_patch_morph_progress(ui);
                } else {
                    self.draw_patch_morph_form(ui);
                }
            });
    }

    fn draw_patch_morph_form(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(
                "Schedule a sequence of LLM nudges that evolve the patch \
                 toward the target prompt across N bars.",
            )
            .monospace()
            .size(9.0)
            .color(theme::ASH),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("PROMPT")
                    .monospace()
                    .size(9.0)
                    .color(theme::FOG),
            );
        });
        ui.add(
            egui::TextEdit::multiline(&mut self.patch_morph_input_prompt)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("evolve from cathedral to dystopia"),
        );
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("BARS")
                    .monospace()
                    .size(9.0)
                    .color(theme::FOG),
            );
            ui.add(
                egui::DragValue::new(&mut self.patch_morph_input_bars)
                    .range(1..=64)
                    .speed(1),
            );
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new("CALLS")
                    .monospace()
                    .size(9.0)
                    .color(theme::FOG),
            );
            // Soft-cap matches the API: 1..=bars*4.
            let max_calls = self.patch_morph_input_bars.saturating_mul(4).max(1);
            ui.add(
                egui::DragValue::new(&mut self.patch_morph_input_calls)
                    .range(1..=max_calls)
                    .speed(1),
            )
            .on_hover_text("Number of LLM nudges fired across the morph.");
        });

        // Keep `calls` inside the dynamic [1, bars*4] envelope when
        // `bars` shrinks — otherwise the DragValue would silently
        // hold a too-high value until the user clicked it.
        let max_calls = self.patch_morph_input_bars.saturating_mul(4).max(1);
        if self.patch_morph_input_calls > max_calls {
            self.patch_morph_input_calls = max_calls;
        }
        if self.patch_morph_input_calls < 1 {
            self.patch_morph_input_calls = 1;
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let prompt_filled = !self.patch_morph_input_prompt.trim().is_empty();
            if ui
                .add_enabled(prompt_filled, egui::Button::new("Start"))
                .on_hover_text("Begin the morph — nudges will fire on bar boundaries.")
                .clicked()
            {
                let prompt = self.patch_morph_input_prompt.trim().to_string();
                let bars = self.patch_morph_input_bars.clamp(1, 64);
                let total_calls = self
                    .patch_morph_input_calls
                    .clamp(1, bars.saturating_mul(4).max(1));
                let mut s = self.state.write();
                let step_division = s.sequencer.step_division;
                let now = s.global_step_count;
                s.patch_morph = crate::state::PatchMorphState::start(
                    prompt.clone(),
                    bars,
                    total_calls,
                    step_division,
                    now,
                );
                drop(s);
                log::info!("[ui] morph: \"{prompt}\" over {bars} bars × {total_calls} calls");
                self.show_patch_morph = false;
            }
            if ui.button("Cancel").clicked() {
                self.show_patch_morph = false;
            }
        });
    }

    fn draw_patch_morph_progress(&mut self, ui: &mut egui::Ui) {
        let (prompt, calls_done, total_calls) = {
            let s = self.state.read();
            (
                s.patch_morph.prompt.clone(),
                s.patch_morph.calls_done,
                s.patch_morph.total_calls,
            )
        };
        ui.label(
            egui::RichText::new("Morph in progress")
                .monospace()
                .size(11.0)
                .color(theme::CHALK),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("\"{}\"", prompt))
                .monospace()
                .size(9.0)
                .color(theme::SMOKE),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("step {} of {}", calls_done, total_calls))
                .monospace()
                .size(9.0)
                .color(theme::ASH),
        );
        // Visual bar — how many calls have completed.
        let frac = if total_calls == 0 {
            0.0
        } else {
            (calls_done as f32 / total_calls as f32).clamp(0.0, 1.0)
        };
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, egui::Rounding::same(1.0), theme::PIT);
        let mut fill = rect;
        fill.set_width(rect.width() * frac);
        painter.rect_filled(fill, egui::Rounding::same(1.0), theme::CHALK);

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .button("Stop")
                .on_hover_text("Cancel any remaining nudges — the patch keeps its current state.")
                .clicked()
            {
                self.state.write().patch_morph.active = false;
                self.show_patch_morph = false;
            }
            if ui.button("Hide").clicked() {
                self.show_patch_morph = false;
            }
        });
    }
}
