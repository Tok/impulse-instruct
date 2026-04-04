// ─── ui/llm_strip.rs ──────────────────────────────────────────────────────────
// LLM interaction strip: style selector, instructions, prompt input, log.

use crate::llm::LlmInput;
use crate::llm::styles::StyleCatalog;
use crate::state::apply_llm_update;
use crate::ui::{ImpulseApp, theme};
use egui::{Frame, TopBottomPanel};

impl ImpulseApp {
    /// Style selector, prompt input, log, and thinking display.
    pub(super) fn draw_llm_strip(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("llm_strip")
            .frame(Frame::none().fill(theme::PIT).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .min_height(80.0)
            .max_height(140.0)
            .show(ctx, |ui| {
                // ── Style selector ────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("STYLE").monospace().size(9.0).color(theme::IRON));
                    ui.add_space(4.0);

                    let cur_style = self.state.read().llm.active_style.clone();
                    let catalog = StyleCatalog::get();
                    let cur_name = match cur_style.as_deref() {
                        None               => "None",
                        Some("__free__")   => "Free",
                        Some("__custom__") => "Custom",
                        Some(id) => catalog.find_by_id(id).map(|s| s.name.as_str()).unwrap_or("None"),
                    };

                    let mut new_style_selection: Option<Option<String>> = None;

                    egui::ComboBox::from_id_source("style_selector")
                        .selected_text(egui::RichText::new(cur_name).monospace().size(9.5))
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(
                                cur_style.is_none(),
                                egui::RichText::new("None").monospace().size(9.5),
                            ).clicked() {
                                new_style_selection = Some(None);
                            }
                            if ui.selectable_label(
                                cur_style.as_deref() == Some("__free__"),
                                egui::RichText::new("Free").monospace().size(9.5),
                            ).clicked() {
                                new_style_selection = Some(Some("__free__".to_string()));
                            }
                            if ui.selectable_label(
                                cur_style.as_deref() == Some("__custom__"),
                                egui::RichText::new("Custom...").monospace().size(9.5),
                            ).clicked() {
                                new_style_selection = Some(Some("__custom__".to_string()));
                            }
                            ui.separator();
                            for style in catalog.styles() {
                                let active = cur_style.as_deref() == Some(style.id.as_str());
                                if ui.selectable_label(
                                    active,
                                    egui::RichText::new(&style.name).monospace().size(9.5),
                                ).clicked() {
                                    new_style_selection = Some(Some(style.id.clone()));
                                }
                            }
                        });

                    if let Some(maybe_id) = new_style_selection {
                        match maybe_id {
                            None => {
                                self.state.write().llm.active_style = None;
                                self.log_text.push_str("Style cleared\n");
                            }
                            Some(ref id) if id == "__free__" => {
                                self.state.write().llm.active_style = Some(id.clone());
                                self.log_text.push_str("Style → Free (no constraints)\n");
                                let _ = self.llm_tx.try_send(LlmInput::Infer {
                                    prompt: "we're going free — be creative and unpredictable, surprise me".to_string(),
                                    one_shot: true,
                                });
                            }
                            Some(ref id) if id == "__custom__" => {
                                self.state.write().llm.active_style = Some(id.clone());
                                self.log_text.push_str("Style → Custom (edit brief below)\n");
                            }
                            Some(id) => {
                                let (name, baseline) = catalog.find_by_id(&id)
                                    .map(|s| (s.name.clone(), s.baseline_params.clone()))
                                    .unwrap_or_default();
                                // Apply baseline params immediately before LLM fires
                                if let Some(ref bp) = baseline {
                                    let current = self.state.read().clone();
                                    *self.state.write() = apply_llm_update(current, bp);
                                }
                                self.state.write().llm.active_style = Some(id);
                                // Reset LLM context so model has no memory of previous style
                                let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                                let prompt = format!(
                                    "FULL RESET to {} — generate all parameters from scratch for this genre. BPM, patterns, synth, FX — everything should match the style brief.",
                                    name
                                );
                                self.log_text.push_str(&format!("Style → {} (context reset)\n", name));
                                let _ = self.llm_tx.try_send(LlmInput::Infer { prompt, one_shot: true });
                            }
                        }
                    }
                });

                // ── Custom style brief input ───────────────────────────────────
                if self.state.read().llm.active_style.as_deref() == Some("__custom__") {
                    ui.horizontal(|ui| {
                        ui.add_space(40.0);
                        let mut custom_text = self.state.read().llm.custom_style_text.clone();
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut custom_text)
                                .hint_text("describe your style brief for the AI…")
                                .desired_width(ui.available_width() - 60.0)
                                .font(egui::FontId::monospace(10.0))
                        );
                        if r.changed() {
                            self.state.write().llm.custom_style_text = custom_text.clone();
                        }
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !custom_text.trim().is_empty()
                        {
                            self.log_text.push_str("Custom style brief updated\n");
                            let _ = self.llm_tx.try_send(LlmInput::Infer {
                                prompt: "apply the active style brief — update sound and rhythm accordingly".to_string(),
                                one_shot: true,
                            });
                        }
                    });
                }

                ui.add_space(2.0);

                // ── Persistent user instructions ──────────────────────────────
                {
                    let mut instr = self.state.read().llm.user_instructions.clone();
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut instr)
                            .hint_text("persistent instructions injected into every prompt…")
                            .desired_width(ui.available_width())
                            .font(egui::FontId::monospace(10.0))
                    );
                    if r.changed() {
                        self.state.write().llm.user_instructions = instr;
                    }
                }

                ui.add_space(2.0);

                // ── Prompt input ──────────────────────────────────────────────
                ui.horizontal(|ui| {
                    let text_width = ui.available_width() - 52.0;
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.prompt_input)
                            .hint_text("prompt the model…")
                            .desired_width(text_width)
                            .font(egui::FontId::monospace(10.5))
                    );
                    let submit = ui.button(
                        egui::RichText::new("ASK").monospace().size(10.0)
                    ).clicked();

                    let enter_pressed = response.lost_focus()
                        && ctx.input(|i| i.key_pressed(egui::Key::Enter));

                    if submit || enter_pressed {
                        let typed = self.prompt_input.trim().to_string();
                        let (prompt, log_line) = if typed.is_empty() {
                            let active_style = self.state.read().llm.active_style.clone();
                            let p = match active_style.as_deref() {
                                Some(id) => {
                                    let name = StyleCatalog::get()
                                        .find_by_id(id)
                                        .map(|s| s.name.as_str())
                                        .unwrap_or(id);
                                    format!("do something fresh in the {} style — surprise me", name)
                                }
                                None => "do something interesting — evolve the pattern and sound however you feel".to_string(),
                            };
                            (p, "YOU → ✦\n".to_string())
                        } else {
                            // Detect style keywords in the typed prompt and update dropdown.
                            let lower = typed.to_lowercase();
                            let catalog = StyleCatalog::get();
                            if let Some(matched) = catalog.styles().iter().find(|s| {
                                s.keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
                            }) {
                                self.state.write().llm.active_style = Some(matched.id.clone());
                                self.log_text.push_str(&format!("Style → {}\n", matched.name));
                            }
                            (typed.clone(), format!("YOU → {}\n", typed))
                        };
                        self.log_text.push_str(&log_line);
                        let _ = self.llm_tx.try_send(LlmInput::Infer { prompt, one_shot: true });
                        self.prompt_input.clear();
                    }
                });

                ui.add_space(4.0);

                // ── Log ───────────────────────────────────────────────────────
                egui::ScrollArea::vertical()
                    .id_source("log_scroll")
                    .stick_to_bottom(true)
                    .max_height(90.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui: &mut egui::Ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.log_text)
                                .desired_width(f32::INFINITY)
                                .font(egui::FontId::monospace(9.0))
                                .text_color(theme::SMOKE)
                                .frame(false)
                                .interactive(true)
                        );
                    });

                // ── Thinking tokens (collapsible) ─────────────────────────────
                if let Some(ref thinking) = self.last_thinking.clone() {
                    ui.horizontal(|ui| {
                        let label = if self.show_thinking { "▾ thinking" } else { "▸ thinking" };
                        if ui.small_button(egui::RichText::new(label).color(theme::IRON).size(9.0)).clicked() {
                            self.show_thinking = !self.show_thinking;
                        }
                    });
                    if self.show_thinking {
                        egui::ScrollArea::vertical()
                            .id_source("thinking_scroll")
                            .max_height(60.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let mut t = thinking.clone();
                                ui.add(
                                    egui::TextEdit::multiline(&mut t)
                                        .desired_width(f32::INFINITY)
                                        .font(egui::FontId::monospace(9.0))
                                        .text_color(theme::IRON)
                                        .frame(false)
                                        .interactive(false)
                                );
                            });
                    }
                }
            });
    }
}
