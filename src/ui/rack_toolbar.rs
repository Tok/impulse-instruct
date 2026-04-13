// ─── ui/rack_toolbar.rs ───────────────────────────────────────────────────────
// Rack toolbar: mode selector, flip toggle, collapse/expand, zone jumps.
// Extracted from rack_canvas.rs to stay under the line limit.

use egui::Color32;

use crate::ui::ImpulseApp;

pub fn draw_toolbar(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        // ── Prompt input (left side) ─────────────────────────────────
        app.draw_prompt_input(ui);
        ui.separator();

        // ── Mode + rack controls (right side) ───────────────────────
        ui.label(
            egui::RichText::new("MODE")
                .monospace()
                .size(8.5)
                .color(Color32::from_gray(80)),
        );
        for (label, mode_opt, tip) in [
            ("·", None, "Normal — drag knobs to change value"),
            (
                "U",
                Some(crate::state::ParamMode::UserOwned),
                "Lock mode — click a knob to lock it (user-owned)",
            ),
            (
                "F",
                Some(crate::state::ParamMode::LlmFocus),
                "Focus mode — click a knob to set LLM focus",
            ),
        ] {
            let active = app.touch_mode == mode_opt;
            let col = if active {
                Color32::from_gray(220)
            } else {
                Color32::from_gray(110)
            };
            let fill = if active {
                Color32::from_gray(55)
            } else {
                Color32::from_gray(22)
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(label).monospace().size(10.0).color(col))
                        .fill(fill)
                        .min_size(egui::vec2(22.0, 18.0)),
                )
                .on_hover_text(tip)
                .clicked()
            {
                app.touch_mode = mode_opt;
            }
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        let flip_col = if app.rack_flipped {
            Color32::from_gray(220)
        } else {
            Color32::from_gray(90)
        };
        let flip_fill = if app.rack_flipped {
            Color32::from_gray(55)
        } else {
            Color32::from_gray(22)
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(if app.rack_flipped { "FRONT" } else { "BACK" })
                        .monospace()
                        .size(8.5)
                        .color(flip_col),
                )
                .fill(flip_fill)
                .min_size(egui::vec2(42.0, 18.0)),
            )
            .on_hover_text("Flip rack  [Tab]  —  hold Alt to hide cables")
            .clicked()
        {
            app.toggle_rack_flip();
        }
        ui.separator();
        let tbtn = |ui: &mut egui::Ui, label: &str| -> bool {
            ui.add(
                egui::Button::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(7.5)
                        .color(Color32::from_gray(90)),
                )
                .fill(Color32::from_gray(22))
                .min_size(egui::vec2(16.0, 16.0)),
            )
            .clicked()
        };
        if tbtn(ui, "ARR") {
            app.state.write().rack.arrange_canonical();
            app.session_dirty = true;
        }
    });
}
