// ─── ui/agent_pills.rs ──────────────────────────────────────────────────────
// One-click re-prompts for the LLM agent card.  Each pill fires a one-shot
// LlmInput::Infer scoped to the agent (so `agent_id = Some(module_id)`),
// saving the user from typing the same handful of refinement requests.

use crate::ui::{ImpulseApp, theme};

const PILLS: &[(&str, &str, &str)] = &[
    (
        "REWRITE",
        "rewrite both melody and rhythm — same vibe, fresh take",
        "Rewrite both melody + rhythm",
    ),
    (
        "VARI",
        "give me a variation on the current pattern",
        "Variation",
    ),
    (
        "FILL",
        "do a quick fill / break before the next bar resets",
        "Fill",
    ),
    ("SPARSE", "sparser — fewer hits, more space", "Sparser"),
    ("BUSY", "busier — more hits, denser pattern", "Busier"),
    (
        "BRIGHT",
        "brighter — open the filter, push the highs",
        "Brighter",
    ),
    (
        "DARK",
        "darker — close the filter, drop the highs",
        "Darker",
    ),
];

/// Render the row of quick-command pills for the LLM agent identified by
/// `module_id`.  Pill clicks send a one-shot LlmInput::Infer scoped to that
/// agent — its existing scope (rack control cables) is honoured by the LLM
/// loop, so the prompt naturally lands inside its sandbox.
pub fn draw_pills(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::new(2.0, 2.0);
        for (label, prompt, hover) in PILLS {
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(*label)
                            .monospace()
                            .size(7.5)
                            .color(theme::SMOKE),
                    )
                    .fill(egui::Color32::from_gray(22))
                    .min_size(egui::vec2(36.0, 14.0)),
                )
                .on_hover_text(*hover)
                .clicked()
            {
                app.send_llm_infer((*prompt).to_string(), true, Some(module_id));
            }
        }
    });
}
