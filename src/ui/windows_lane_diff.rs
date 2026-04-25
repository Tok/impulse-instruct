// ─── ui/windows_lane_diff.rs ─────────────────────────────────────────────────
// Lane Diff window — surfaces `LlmState.recent_lane_applies` so the user
// can see "what changed this turn" per lane.  Each row shows the lane
// label, wall-time of the inference, and a collapsible JSON dump of the
// payload that was applied.  The pipeline filter has already narrowed
// the JSON to keys the lane actually wrote, so each row is effectively
// a writeback diff for that lane.
//
// Toggled from the header view menu (`show_lane_diff`); off by default
// to keep the chrome clean for users who don't care about the diff.

use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    pub(super) fn draw_lane_diff_window(&mut self, ctx: &egui::Context) {
        if !self.show_lane_diff {
            return;
        }
        let mut open = self.show_lane_diff;
        egui::Window::new("Lane Diff")
            .collapsible(true)
            .resizable(true)
            .default_width(420.0)
            .default_height(360.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let records: Vec<crate::state::LaneApplyRecord> = self
                    .state
                    .read()
                    .llm
                    .recent_lane_applies
                    .iter()
                    .rev()
                    .cloned()
                    .collect();
                if records.is_empty() {
                    ui.label(
                        egui::RichText::new("No lane applies recorded yet.")
                            .monospace()
                            .size(9.0)
                            .color(theme::IRON),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Trigger an LLM turn or jam cycle — each successful lane apply will land here.",
                        )
                        .monospace()
                        .size(8.0)
                        .color(theme::IRON),
                    );
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{} entries (newest first)", records.len()))
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                    );
                    if ui
                        .small_button(egui::RichText::new("clear").monospace().size(8.5))
                        .clicked()
                    {
                        self.state.write().llm.recent_lane_applies.clear();
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for (i, rec) in records.iter().enumerate() {
                            let header = format!(
                                "{}  cycle {}  ·  {} ms  ·  {} keys",
                                rec.lane_label,
                                rec.cycle,
                                rec.ms,
                                count_diff_keys(&rec.update),
                            );
                            egui::CollapsingHeader::new(
                                egui::RichText::new(header)
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            )
                            .id_source(("lane_diff_row", i, rec.cycle))
                            .default_open(i == 0)
                            .show(ui, |ui| {
                                let pretty = serde_json::to_string_pretty(&rec.update)
                                    .unwrap_or_else(|_| rec.update.to_string());
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(pretty)
                                            .monospace()
                                            .size(8.5)
                                            .color(theme::SMOKE),
                                    )
                                    .wrap()
                                    .selectable(true),
                                );
                            });
                        }
                    });
            });
        self.show_lane_diff = open;
    }
}

/// Count the leaf-ish keys in a writeback payload — used for the row
/// header to give a one-glance sense of how much each lane changed.
/// Top-level keys count as 1; nested objects (e.g. `sequencer.bpm` +
/// `sequencer.bass_steps`) count each subkey.
fn count_diff_keys(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::Object(map) => map
            .values()
            .map(|sub| match sub {
                serde_json::Value::Object(inner) => inner.len(),
                _ => 1,
            })
            .sum(),
        _ => 1,
    }
}
