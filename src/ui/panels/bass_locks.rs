// ─── ui/panels/bass_locks.rs ─────────────────────────────────────────────────
// LOCKED params strip for the bass panel — lets the user remove individual
// `bass.*` locks or unlock all of them at once.
// Extracted from bass.rs to stay under the line limit.

use crate::ui::{ImpulseApp, theme};

pub(super) fn draw_locked_params(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let locked_bass: Vec<String> = {
        let s = app.state.read();
        s.llm
            .locked_params
            .iter()
            .filter(|p| p.starts_with("bass"))
            .cloned()
            .collect()
    };

    // `horizontal_wrapped` so a long lock list wraps inside the module
    // instead of stretching the panel (a locked-down scenario can pin
    // 30+ params and overflow the bass module otherwise).  Cap width
    // explicitly in case a parent layout doesn't constrain us.
    let max_w = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(max_w, 0.0),
        egui::Layout::left_to_right(egui::Align::TOP),
        |ui| {
            ui.set_max_width(max_w);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("LOCKED:")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(8.5),
                );
                if locked_bass.is_empty() {
                    ui.label(
                        egui::RichText::new("none (LLM controls all)")
                            .color(theme::IRON)
                            .monospace()
                            .size(8.5),
                    );
                } else {
                    let mut to_remove: Option<String> = None;
                    for p in &locked_bass {
                        let short = p.replace("bass.", "");
                        if ui
                            .small_button(
                                egui::RichText::new(format!("× {}", short))
                                    .monospace()
                                    .size(8.0),
                            )
                            .clicked()
                        {
                            to_remove = Some(p.clone());
                        }
                    }
                    if let Some(p) = to_remove {
                        let next = crate::state::unlock_param(app.state.read().clone(), &p);
                        *app.state.write() = next;
                    }
                }
                if ui
                    .small_button(egui::RichText::new("UNLOCK ALL").monospace().size(8.0))
                    .clicked()
                {
                    let mut next = app.state.read().clone();
                    next.llm.locked_params.retain(|p| !p.starts_with("bass"));
                    *app.state.write() = next;
                }
            });
        },
    );
}
