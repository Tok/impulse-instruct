// ─── ui/llm_strip.rs ──────────────────────────────────────────────────────────
// LLM interaction strip: style selector, instructions, prompt input, log.
//
// Layout (resizable panel, drag the bottom border):
//   TOP row  — LEFT: style + instructions  |  RIGHT: log (fills height)
//   BOTTOM row — full-width prompt input + ASK button (vertically centred)
//
// The Huth note colorizer for the log pane lives in `ui/llm_log_color.rs`.

use crate::llm::LlmInput;
use crate::llm::styles::StyleCatalog;
use crate::state::apply_llm_update;
use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    /// Drain the audio capture buffer, run analysis, and fire a one-shot LLM
    /// prompt with the results. No-op if no audio has been captured yet.
    pub(crate) fn trigger_listen(&mut self) {
        use crate::audio::analysis::{analyse_audio, format_snapshot};
        use crate::audio::{SAMPLE_RATE, SAMPLE_RATE_HZ};
        let mut captured: Vec<f32> = Vec::with_capacity(SAMPLE_RATE_HZ as usize * 10);
        while let Ok(s) = self.capture_rx.pop() {
            captured.push(s);
        }
        if !captured.is_empty() {
            let analysis = analyse_audio(&captured, SAMPLE_RATE);
            let snapshot = format_snapshot(&analysis);
            let prompt = format!(
                "{}\nYou are listening to the audio you just produced. React — correct any mix or arrangement issues. Respond in JSON.",
                snapshot
            );
            self.log_text.push_str("LISTEN → analysing…\n");
            let _ = self.llm_tx.try_send(LlmInput::Infer {
                prompt,
                one_shot: true,
                agent_id: None,
            });
            self.audio_analysis = Some(analysis);
            self.listen_pending = true;
        } else {
            self.log_text.push_str("LISTEN → no audio captured yet\n");
        }
    }

    /// LLM console content — rendered inside a rackable module card.
    /// Contains: style selector, instructions, log, JAM timing, LISTEN, prompt input.
    fn apply_style_rack_modules(&mut self, names: &[String]) {
        super::style_rack::apply(self, names);
    }

    fn apply_style_selection(&mut self, maybe_id: Option<String>) {
        match maybe_id {
            None => {
                self.state.write().llm.active_style = None;
                self.log_text.push_str("Style cleared\n");
            }
            Some(ref id) if id == "__free__" => {
                self.state.write().llm.active_style = Some(id.clone());
                self.log_text.push_str("Style → Free\n");
                let _ = self.llm_tx.try_send(LlmInput::Infer {
                    prompt: "we're going free — be creative and unpredictable, surprise me".into(),
                    one_shot: true,
                    agent_id: None,
                });
            }
            Some(ref id) if id == "__custom__" => {
                self.state.write().llm.active_style = Some(id.clone());
                self.log_text.push_str("Style → Custom\n");
            }
            Some(id) => {
                let catalog = StyleCatalog::get();
                let (name, baseline, rack_modules) = catalog
                    .find_by_id(&id)
                    .map(|s| {
                        (
                            s.name.clone(),
                            s.baseline_params.clone(),
                            s.rack_modules.clone(),
                        )
                    })
                    .unwrap_or_default();
                if let Some(ref bp) = baseline {
                    let current = self.state.read().clone();
                    let (ramped, remainder) =
                        crate::state::jam_tools::schedule_baseline_ramps(current, bp, 8.0);
                    let next = if remainder.as_object().is_some_and(|o| !o.is_empty()) {
                        apply_llm_update(ramped, &remainder, &[])
                    } else {
                        ramped
                    };
                    *self.state.write() = next;
                }
                self.apply_style_rack_modules(&rack_modules);
                self.state.write().llm.active_style = Some(id);
                let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                self.log_text
                    .push_str(&format!("Style → {} (reset)\n", name));
                let _ = self.llm_tx.try_send(LlmInput::Infer {
                    prompt: format!(
                        "FULL RESET to {} — generate all parameters from scratch.",
                        name
                    ),
                    one_shot: true,
                    agent_id: None,
                });
            }
        }
        // Propagate style to all sub-agents that don't have style_locked
        let style_id = self.state.read().llm.active_style.clone();
        if let Some(sid) = style_id {
            let snapshot = self.state.read().clone();
            *self.state.write() = crate::state::propagate_style(snapshot, &sid);
        }
    }

    pub(super) fn draw_llm_console_content(&mut self, ui: &mut egui::Ui) {
        // Override the centered layout from module_card — console needs full-width
        // text fields, log area, and prompt input that fill the card.
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            self.draw_llm_console_inner(ui);
        });
    }

    fn draw_llm_console_inner(&mut self, ui: &mut egui::Ui) {
        // ── Model selector + context bar ─────────────────────────────
        ui.horizontal(|ui| {
            // Model dropdown (global default)
            if self.available_models.is_empty() {
                self.available_models = super::scan_models();
            }
            let (cur_model, ctx_used, ctx_max, is_mock, is_initializing) = {
                let s = self.state.read();
                (
                    s.llm.model_path.clone(),
                    s.llm.context_used,
                    s.llm.context_max,
                    s.llm.is_mock,
                    s.llm.llm_initializing,
                )
            };
            let cur_short = std::path::Path::new(&cur_model)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            ui.label(
                egui::RichText::new("MODEL")
                    .monospace()
                    .size(8.0)
                    .color(theme::ASH),
            );
            egui::ComboBox::from_id_source("console_model")
                .selected_text(
                    egui::RichText::new(&cur_short)
                        .monospace()
                        .size(8.5)
                        .color(theme::SMOKE),
                )
                .width(180.0)
                .show_ui(ui, |ui| {
                    for path in &self.available_models.clone() {
                        let short = std::path::Path::new(path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(path)
                            .to_string();
                        let selected = *path == cur_model;
                        if ui
                            .selectable_label(
                                selected,
                                egui::RichText::new(&short)
                                    .monospace()
                                    .size(9.0)
                                    .color(if selected { theme::CHALK } else { theme::FOG }),
                            )
                            .clicked()
                            && !selected
                        {
                            let _ = self.llm_tx.try_send(LlmInput::SwitchModel(path.clone()));
                            self.state.write().llm.llm_initializing = true;
                        }
                    }
                });
            ui.separator();
            // Context bar
            let pct = if ctx_max > 0 {
                ctx_used as f32 / ctx_max as f32
            } else {
                0.0
            };
            ui.label(
                egui::RichText::new(format!("CONTEXT {:.0}%", pct * 100.0))
                    .monospace()
                    .size(8.0)
                    .color(if pct > 0.85 { theme::FOG } else { theme::ASH }),
            );
            let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(80.0, 6.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(bar_rect, 1.0, egui::Color32::from_gray(38));
            let fill_w = (bar_rect.width() * pct.clamp(0.0, 1.0)).max(0.0);
            if fill_w > 0.0 {
                p.rect_filled(
                    egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, bar_rect.height())),
                    1.0,
                    egui::Color32::from_gray(if pct > 0.85 { 160 } else { 90 }),
                );
            }
            // Reset button
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("RESET")
                            .monospace()
                            .size(7.5)
                            .color(theme::IRON),
                    )
                    .fill(egui::Color32::TRANSPARENT),
                )
                .on_hover_text("Reset context (restart server)")
                .clicked()
            {
                let _ = self.llm_tx.try_send(LlmInput::ResetContext);
            }
            if is_mock {
                ui.label(
                    egui::RichText::new("MOCK")
                        .monospace()
                        .size(8.0)
                        .color(theme::IRON),
                );
            }
            if is_initializing {
                let t = ui.ctx().input(|i| i.time) as f32;
                let dots = match ((t * 2.0) as usize) % 4 {
                    0 => "   ",
                    1 => ".  ",
                    2 => ".. ",
                    _ => "...",
                };
                let pulse = (t * 3.0 * std::f32::consts::TAU).sin() * 0.3 + 0.7;
                let g = (140.0 * pulse) as u8;
                ui.label(
                    egui::RichText::new(format!("Loading{}", dots))
                        .monospace()
                        .size(8.0)
                        .color(egui::Color32::from_gray(g)),
                );
                ui.ctx().request_repaint();
            }

            ui.separator();

            // ── JAM timing (same line as model/ctx) ─────────────────
            ui.label(
                egui::RichText::new("JAM")
                    .monospace()
                    .size(8.0)
                    .color(theme::ASH),
            );
            let (jam_bars, cycle_count, is_inferring, active_ramps, tps) = {
                let s = self.state.read();
                (
                    s.llm.jam_bars,
                    s.llm.jam_cycle_count,
                    s.llm.is_inferring,
                    s.llm.active_ramps.len(),
                    s.llm.tokens_per_sec,
                )
            };
            for (label, bars) in &[
                ("C", 0.0f32),
                ("1", 1.0),
                ("2", 2.0),
                ("4", 4.0),
                ("8", 8.0),
            ] {
                let active = (jam_bars - bars).abs() < 0.01;
                let col = if active { theme::FOG } else { theme::SMOKE };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(*label).monospace().size(7.5).color(col),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 12.0)),
                    )
                    .clicked()
                {
                    self.state.write().llm.jam_bars = *bars;
                }
            }
            ui.label(
                egui::RichText::new(format!("#{}", cycle_count))
                    .monospace()
                    .size(7.0)
                    .color(theme::IRON),
            );
            if is_inferring {
                ui.label(egui::RichText::new("▶").size(7.5).color(theme::FOG))
                    .on_hover_text(format!("{:.1} tok/s", tps));
            } else if tps > 0.0 {
                ui.label(
                    egui::RichText::new(format!("{:.0}t/s", tps))
                        .monospace()
                        .size(7.0)
                        .color(theme::IRON),
                );
            }
            if active_ramps > 0 {
                ui.label(
                    egui::RichText::new(format!("~{}", active_ramps))
                        .monospace()
                        .size(7.0)
                        .color(theme::IRON),
                );
            }
            if let Some((fire_at, _)) = self.jam_next_fire {
                let remaining = fire_at.duration_since(std::time::Instant::now());
                ui.label(
                    egui::RichText::new(format!("{:.1}s", remaining.as_secs_f32()))
                        .monospace()
                        .size(7.0)
                        .color(theme::ASH),
                );
            }

            // ── Agent round-robin (right-justified on same line) ────
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let s = self.state.read();
                let agents = &s.llm_agents;
                let enabled: Vec<_> = agents
                    .iter()
                    .filter(|a| s.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
                    .collect();
                if !enabled.is_empty() {
                    let time = ui.ctx().input(|i| i.time) as f32;
                    // Right-to-left: draw in reverse so they appear left-to-right
                    for a in enabled.iter().rev() {
                        let scope_str = a.scope.join(",");
                        if !scope_str.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("[{}]", scope_str))
                                    .color(theme::IRON)
                                    .monospace()
                                    .size(6.5),
                            );
                        }
                        let name_col = if a.is_inferring {
                            theme::CHALK
                        } else {
                            theme::IRON
                        };
                        ui.label(
                            egui::RichText::new(&a.persona_name)
                                .color(name_col)
                                .monospace()
                                .size(7.5),
                        );
                        let dot_col = if a.is_inferring {
                            let p = (time * 4.0 * std::f32::consts::TAU).sin() * 0.3 + 0.7;
                            egui::Color32::from_gray((220.0 * p) as u8)
                        } else {
                            theme::IRON
                        };
                        ui.label(egui::RichText::new("●").color(dot_col).size(7.5));
                    }
                    if agents.iter().any(|a| a.is_inferring) {
                        ui.ctx().request_repaint();
                    }
                }
            });
        });

        // ── Style selector + instructions ────────────────────────────
        ui.horizontal(|ui| {
            let style_locked = self.state.read().llm.style_lock;
            let lock_label = if style_locked { "STYLE [L]" } else { "STYLE" };
            let lock_col = if style_locked { theme::FOG } else { theme::ASH };
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(lock_label)
                            .monospace()
                            .size(9.0)
                            .color(lock_col),
                    )
                    .fill(egui::Color32::TRANSPARENT),
                )
                .on_hover_text(if style_locked {
                    "Style locked — agents cannot change it. Click to unlock."
                } else {
                    "Style unlocked — agents may change it. Click to lock."
                })
                .clicked()
            {
                self.state.write().llm.style_lock = !style_locked;
            }
            ui.add_space(4.0);
            let cur_style = self.state.read().llm.active_style.clone();
            let catalog = StyleCatalog::get();
            let cur_name = match cur_style.as_deref() {
                None => "None",
                Some("__free__") => "Free",
                Some("__custom__") => "Custom",
                Some(id) => catalog
                    .find_by_id(id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("None"),
            };
            let mut new_sel: Option<Option<String>> = None;
            egui::ComboBox::from_id_source(ui.id().with("console_style"))
                .selected_text(egui::RichText::new(cur_name).monospace().size(9.5))
                .width(140.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            cur_style.is_none(),
                            egui::RichText::new("None").monospace().size(9.5),
                        )
                        .clicked()
                    {
                        new_sel = Some(None);
                    }
                    if ui
                        .selectable_label(
                            cur_style.as_deref() == Some("__free__"),
                            egui::RichText::new("Free").monospace().size(9.5),
                        )
                        .clicked()
                    {
                        new_sel = Some(Some("__free__".into()));
                    }
                    if ui
                        .selectable_label(
                            cur_style.as_deref() == Some("__custom__"),
                            egui::RichText::new("Custom...").monospace().size(9.5),
                        )
                        .clicked()
                    {
                        new_sel = Some(Some("__custom__".into()));
                    }
                    ui.separator();
                    for s in catalog.styles() {
                        if ui
                            .selectable_label(
                                cur_style.as_deref() == Some(s.id.as_str()),
                                egui::RichText::new(&s.name).monospace().size(9.5),
                            )
                            .clicked()
                        {
                            new_sel = Some(Some(s.id.clone()));
                        }
                    }
                });
            if let Some(maybe_id) = new_sel {
                self.apply_style_selection(maybe_id);
            }
            ui.separator();
            // Instructions
            let mut instr = self.state.read().llm.user_instructions.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut instr)
                        .hint_text("persistent instructions…")
                        .desired_width(ui.available_width())
                        .font(egui::FontId::monospace(9.5)),
                )
                .changed()
            {
                self.state.write().llm.user_instructions = instr;
            }
        });

        // Prompt input moved to the rack toolbar (draw_prompt_input).
    }

    /// Prompt input + submit — drawn in the rack toolbar, not the LLM console.
    pub(super) fn draw_prompt_input(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_width();
        // Leave space for the toolbar buttons on the right
        let prompt_w = (avail - 300.0).max(200.0);

        let response = ui.add(
            egui::TextEdit::singleline(&mut self.prompt_input)
                .hint_text("prompt…")
                .desired_width(prompt_w)
                .min_size(egui::vec2(0.0, 18.0))
                .font(egui::FontId::monospace(10.0)),
        );
        let submit = ui
            .button(egui::RichText::new("↵").monospace().size(10.0))
            .clicked();

        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter_pressed {
            if self.prompt_input.ends_with('\n') {
                self.prompt_input.pop();
            }
            response.request_focus();
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
                (p, "YOU → [evolve]\n".to_string())
            } else {
                let lower = typed.to_lowercase();
                let catalog = StyleCatalog::get();
                if let Some(matched) = catalog.styles().iter().find(|s| {
                    s.keywords
                        .iter()
                        .any(|kw| lower.contains(&kw.to_lowercase()))
                }) {
                    self.state.write().llm.active_style = Some(matched.id.clone());
                    self.log_text
                        .push_str(&format!("Style → {}\n", matched.name));
                }
                (typed.clone(), format!("YOU → {}\n", typed))
            };
            self.log_text.push_str(&log_line);
            let enabled_agents: Vec<u32> = {
                let s = self.state.read();
                s.llm_agents
                    .iter()
                    .filter(|a| s.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
                    .map(|a| a.id)
                    .collect()
            };
            if enabled_agents.is_empty() {
                let _ = self.llm_tx.try_send(LlmInput::Infer {
                    prompt,
                    one_shot: true,
                    agent_id: None,
                });
            } else {
                for aid in enabled_agents {
                    let _ = self.llm_tx.try_send(LlmInput::Infer {
                        prompt: prompt.clone(),
                        one_shot: true,
                        agent_id: Some(aid),
                    });
                }
            }
            self.prompt_input.clear();
        }
    }
}
