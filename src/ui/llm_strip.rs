// ─── ui/llm_strip.rs ──────────────────────────────────────────────────────────
// LLM interaction strip: style selector, instructions, prompt input, log.
//
// Layout (resizable panel, drag the bottom border):
//   TOP row  — LEFT: style + instructions  |  RIGHT: log (fills height)
//   BOTTOM row — full-width prompt input + ASK button (vertically centred)

use crate::llm::LlmInput;
use crate::llm::styles::StyleCatalog;
use crate::state::apply_llm_update;
use crate::ui::{ImpulseApp, theme};
use egui::{Frame, TopBottomPanel};

impl ImpulseApp {
    /// Style selector, prompt input, log, and thinking display.
    pub(super) fn draw_llm_strip(&mut self, ctx: &egui::Context) {
        // Compact default: style row ~22px + instructions ~22px + prompt ~34px + top margin 4px ≈ 82px.
        // User can drag the bottom border down to reveal more log lines.
        TopBottomPanel::top("llm_strip")
            .frame(Frame::none().fill(theme::PIT).inner_margin(egui::Margin { left: 8.0, right: 8.0, top: 4.0, bottom: 0.0 }))
            .resizable(true)
            .min_height(70.0)
            .default_height(95.0)
            .show(ctx, |ui| {
                // ── TOP row: style + instructions | log ───────────────────────
                ui.horizontal(|ui| {
                    // ── LEFT column: style + instructions ─────────────────────
                    let left_w = (ui.available_width() * 0.36).clamp(270.0, 420.0);

                    ui.scope(|ui| {
                        ui.set_min_width(left_w);
                        ui.set_max_width(left_w);

                        // Style selector row
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("STYLE")
                                    .monospace()
                                    .size(9.0)
                                    .color(theme::ASH),
                            );
                            ui.add_space(4.0);

                            let cur_style = self.state.read().llm.active_style.clone();
                            let catalog = StyleCatalog::get();
                            let cur_name = match cur_style.as_deref() {
                                None               => "None",
                                Some("__free__")   => "Free",
                                Some("__custom__") => "Custom",
                                Some(id) => catalog.find_by_id(id)
                                    .map(|s| s.name.as_str())
                                    .unwrap_or("None"),
                            };

                            let mut new_style_selection: Option<Option<String>> = None;

                            egui::ComboBox::from_id_source("style_selector")
                                .selected_text(
                                    egui::RichText::new(cur_name).monospace().size(9.5),
                                )
                                .width(140.0)
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
                                        let active =
                                            cur_style.as_deref() == Some(style.id.as_str());
                                        if ui.selectable_label(
                                            active,
                                            egui::RichText::new(&style.name)
                                                .monospace()
                                                .size(9.5),
                                        ).clicked() {
                                            new_style_selection = Some(Some(style.id.clone()));
                                        }
                                    }
                                });

                            // Inline custom brief when __custom__ is active
                            if cur_style.as_deref() == Some("__custom__") {
                                let mut custom_text =
                                    self.state.read().llm.custom_style_text.clone();
                                let r = ui.add(
                                    egui::TextEdit::singleline(&mut custom_text)
                                        .hint_text("style brief…")
                                        .desired_width(ui.available_width() - 4.0)
                                        .font(egui::FontId::monospace(9.5)),
                                );
                                if r.changed() {
                                    self.state.write().llm.custom_style_text = custom_text.clone();
                                }
                                if r.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    && !custom_text.trim().is_empty()
                                {
                                    self.log_text.push_str("Custom style brief updated\n");
                                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                                        prompt: "apply the active style brief — update sound and rhythm accordingly".to_string(),
                                        one_shot: true,
                                    });
                                }
                            }

                            if let Some(maybe_id) = new_style_selection {
                                match maybe_id {
                                    None => {
                                        self.state.write().llm.active_style = None;
                                        self.log_text.push_str("Style cleared\n");
                                    }
                                    Some(ref id) if id == "__free__" => {
                                        self.state.write().llm.active_style = Some(id.clone());
                                        self.log_text.push_str("Style → Free\n");
                                        let _ = self.llm_tx.try_send(LlmInput::Infer {
                                            prompt: "we're going free — be creative and unpredictable, surprise me".to_string(),
                                            one_shot: true,
                                        });
                                    }
                                    Some(ref id) if id == "__custom__" => {
                                        self.state.write().llm.active_style = Some(id.clone());
                                        self.log_text.push_str("Style → Custom\n");
                                    }
                                    Some(id) => {
                                        let (name, baseline) = catalog.find_by_id(&id)
                                            .map(|s| (s.name.clone(), s.baseline_params.clone()))
                                            .unwrap_or_default();
                                        if let Some(ref bp) = baseline {
                                            let current = self.state.read().clone();
                                            *self.state.write() = apply_llm_update(current, bp);
                                        }
                                        self.state.write().llm.active_style = Some(id);
                                        let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                                        let prompt = format!(
                                            "FULL RESET to {} — generate all parameters from scratch.",
                                            name
                                        );
                                        self.log_text
                                            .push_str(&format!("Style → {} (reset)\n", name));
                                        let _ = self.llm_tx.try_send(LlmInput::Infer {
                                            prompt,
                                            one_shot: true,
                                        });
                                    }
                                }
                            }
                        });

                        // Persistent instructions
                        {
                            let mut instr = self.state.read().llm.user_instructions.clone();
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut instr)
                                    .hint_text("persistent instructions…")
                                    .desired_width(left_w - 4.0)
                                    .font(egui::FontId::monospace(9.5)),
                            );
                            if r.changed() {
                                self.state.write().llm.user_instructions = instr;
                            }
                        }
                    }); // end left scope

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // ── RIGHT column: log + thinking ──────────────────────────
                    ui.vertical(|ui| {
                        // Thinking toggle (compact, one line)
                        if let Some(ref thinking) = self.last_thinking.clone() {
                            let label =
                                if self.show_thinking { "▾ think" } else { "▸ think" };
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new(label)
                                            .color(theme::IRON)
                                            .monospace()
                                            .size(8.5),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.show_thinking = !self.show_thinking;
                            }
                            if self.show_thinking {
                                egui::ScrollArea::vertical()
                                    .id_source("thinking_scroll")
                                    .max_height(30.0)
                                    .auto_shrink([false; 2])
                                    .show(ui, |ui| {
                                        let mut t = thinking.clone();
                                        ui.add(
                                            egui::TextEdit::multiline(&mut t)
                                                .desired_width(f32::INFINITY)
                                                .font(egui::FontId::monospace(8.5))
                                                .text_color(theme::ASH)
                                                .frame(false)
                                                .interactive(false),
                                        );
                                    });
                            }
                        }

                        // Log fills remaining height
                        egui::ScrollArea::vertical()
                            .id_source("log_scroll")
                            .stick_to_bottom(true)
                            .auto_shrink([false; 2])
                            .show(ui, |ui: &mut egui::Ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.log_text)
                                        .desired_width(f32::INFINITY)
                                        .font(egui::FontId::monospace(9.0))
                                        .text_color(theme::FOG)
                                        .frame(false)
                                        .interactive(true),
                                );
                            });
                    });
                }); // end top row

                // ── LISTEN bar: capture + analysis display ────────────────────
                ui.horizontal(|ui| {
                    // Listen button — drains capture ring buffer, runs analysis
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new("LISTEN")
                                .monospace()
                                .size(8.5)
                                .color(theme::ASH),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 14.0)),
                    ).clicked() {
                        self.trigger_listen();
                    }

                    // AUTO toggle — re-triggers LISTEN every 4 jam cycles
                    let auto_color = if self.auto_listen { theme::FOG } else { theme::SMOKE };
                    if ui.add(
                        egui::Button::new(
                            egui::RichText::new("AUTO")
                                .monospace()
                                .size(8.0)
                                .color(auto_color),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 14.0)),
                    ).clicked() {
                        self.auto_listen = !self.auto_listen;
                        self.auto_listen_counter = 0;
                    }

                    // Show snapshot stats if available
                    // Per-voice level bars derived from volume params in state
                    {
                        let s = self.state.read();
                        let levels: &[(&str, f32)] = &[
                            ("BAS", s.bass.volume),
                            ("K-A", s.kit_a.kick.volume),
                            ("S-A", s.kit_a.snare.volume),
                            ("HH", s.kit_a.hihat_closed.volume),
                            ("K-B", s.kit_b.kick.volume),
                            ("S-B", s.kit_b.snare.volume),
                            ("CLP", s.kit_b.clap.volume),
                            ("AMN", s.amen.volume),
                        ];
                        ui.add_space(6.0);
                        for (label, vol) in levels {
                            let bar_h = 10.0_f32;
                            let bar_w = 3.0_f32;
                            let fill_h = (vol * bar_h).clamp(0.0, bar_h);
                            let (resp, painter) = ui.allocate_painter(
                                egui::vec2(bar_w + 1.0, bar_h),
                                egui::Sense::hover(),
                            );
                            let rect = resp.rect;
                            // Background
                            painter.rect_filled(rect, 0.0, theme::VOID);
                            // Fill from bottom
                            let fill_rect = egui::Rect::from_min_max(
                                egui::pos2(rect.min.x, rect.max.y - fill_h),
                                rect.max,
                            );
                            painter.rect_filled(fill_rect, 0.0, theme::IRON);
                            resp.on_hover_text(format!("{} {:.0}%", label, vol * 100.0));
                        }
                    }

                    if let Some(ref a) = self.audio_analysis {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "sub {:+.0}  low {:+.0}  mid {:+.0}  hi {:+.0}  pk {:+.1}  crest {:.0}dB  ~{:.0}tr/bar",
                                a.sub_rms_db, a.low_rms_db, a.mid_rms_db, a.high_rms_db,
                                a.peak_db, a.crest_db, a.transients_per_bar
                            ))
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                    }
                });

                // ── BOTTOM row: full-width prompt + Enter (vertically centred) ──
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                    let avail = ui.available_width();
                    let prompt_w = avail - 50.0;
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.prompt_input)
                            .hint_text("prompt the model…")
                            .desired_width(prompt_w)
                            .desired_rows(2)
                            .font(egui::FontId::monospace(11.5)),
                    );
                    let submit = ui
                        .add_sized(
                            [44.0, response.rect.height()],
                            egui::Button::new(
                                egui::RichText::new("↵").monospace().size(14.0),
                            ),
                        )
                        .clicked();

                    // Enter (without Shift) submits; trim the trailing newline first.
                    let enter_pressed = response.has_focus()
                        && ctx.input(|i| {
                            i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                        });
                    if enter_pressed && self.prompt_input.ends_with('\n') {
                        self.prompt_input.pop();
                    }

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
                                    format!("do something fresh in the {} style", name)
                                }
                                None => "do something interesting — evolve the pattern and sound".to_string(),
                            };
                            (p, "YOU → ✦\n".to_string())
                        } else {
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
            });
    }
}
