// ─── ui/panels/timeline.rs ────────────────────────────────────────────────────
// LLM activity timeline — structured, scrollable log of agent actions.

use crate::ui::{ActivityAction, ImpulseApp, theme};

pub fn draw_timeline(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let start = app.activity_log.first().map(|e| e.timestamp);

    if app.activity_log.is_empty() {
        ui.label(
            egui::RichText::new("No activity yet")
                .color(theme::IRON)
                .monospace()
                .size(8.5),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(200.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for entry in &app.activity_log {
                let elapsed = start
                    .map(|s| entry.timestamp.duration_since(s).as_secs_f32())
                    .unwrap_or(0.0);
                let tag = match entry.action {
                    ActivityAction::Response => "RSP",
                    ActivityAction::Thinking => "THK",
                    ActivityAction::ParamUpdate => "UPD",
                    ActivityAction::Spawn => "NEW",
                    ActivityAction::Dismiss => "DEL",
                    ActivityAction::UserPrompt => "YOU",
                    ActivityAction::System => "SYS",
                };
                let tag_col = match entry.action {
                    ActivityAction::ParamUpdate => theme::FOG,
                    ActivityAction::UserPrompt => theme::CHALK,
                    ActivityAction::System => theme::IRON,
                    _ => theme::ASH,
                };
                // Compact one-liner: [time] TAG persona: detail
                let text = format!(
                    "[{:>6.1}s] {} {} {}",
                    elapsed,
                    tag,
                    entry.persona,
                    if entry.detail.len() > 80 {
                        format!("{}...", &entry.detail[..77])
                    } else {
                        entry.detail.clone()
                    }
                );
                ui.label(
                    egui::RichText::new(text)
                        .monospace()
                        .size(8.0)
                        .color(tag_col),
                );
            }
        });

    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} entries", app.activity_log.len()))
                .color(theme::IRON)
                .monospace()
                .size(7.5),
        );
        if ui
            .small_button(egui::RichText::new("Clear").monospace().size(7.5))
            .clicked()
        {
            app.activity_log.clear();
        }
    });
}
