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
    /// Send an `LlmInput::Infer` to the LLM thread and update the UI-side
    /// queue shadow that drives the LLM console's cycle viz.  Wraps every
    /// in-UI Infer dispatch so the visualiser stays in sync with what the
    /// LLM thread will see.  Direct API / OSC sends bypass this — they're
    /// not represented in the viz for v1.
    pub(crate) fn send_llm_infer(&mut self, prompt: String, one_shot: bool, agent_id: Option<u32>) {
        self.llm_queue.note_send(agent_id);
        let _ = self.llm_tx.try_send(crate::llm::LlmInput::Infer {
            prompt,
            one_shot,
            agent_id,
        });
    }

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
            self.send_llm_infer(prompt, true, None);
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
                self.send_llm_infer(
                    "we're going free — be creative and unpredictable, surprise me".into(),
                    true,
                    None,
                );
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
                self.send_llm_infer(
                    format!(
                        "FULL RESET to {} — generate all parameters from scratch.",
                        name
                    ),
                    true,
                    None,
                );
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
        // Split the card into LEFT = round-robin cycle viz, RIGHT =
        // existing console rows.  Cycle is square (width = height)
        // and takes the full panel height — same idiom as the ring
        // oscilloscope reserves a square chip on its panel side.
        // Capped so the right side always has room for prompt + log.
        let total_w = ui.available_width();
        let total_h = ui.available_height();
        let cycle_size = total_h.max(96.0).min((total_w * 0.35).max(96.0));
        let gap = 6.0;
        let right_w = (total_w - cycle_size - gap).max(220.0);

        let cursor = ui.cursor().min;
        let cycle_rect = egui::Rect::from_min_size(cursor, egui::vec2(cycle_size, total_h));
        let right_rect = egui::Rect::from_min_size(
            egui::pos2(cursor.x + cycle_size + gap, cursor.y),
            egui::vec2(right_w, total_h),
        );

        ui.allocate_ui_at_rect(cycle_rect, |ui| {
            let secs_to_next_fire = self.jam_next_fire.map(|(at, _)| {
                at.saturating_duration_since(std::time::Instant::now())
                    .as_secs_f32()
            });
            let s = self.state.read();
            super::widgets::llm_cycle(
                ui,
                &s,
                &self.llm_queue,
                self.jam_next_agent,
                secs_to_next_fire,
                cycle_size,
            );
        });
        ui.allocate_ui_at_rect(right_rect, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                self.draw_llm_console_inner(ui);
            });
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
                            // Optimistic UI: reset every agent override to None
                            // immediately so the agent dropdowns flip to (Default)
                            // this frame, without waiting for the LLM thread to
                            // process the SwitchModel message.  The LLM thread's
                            // SwitchModel handler GCs the pool via shutdown_all_except,
                            // which is robust to this state already being None.
                            {
                                let mut s = self.state.write();
                                for a in s.llm_agents.iter_mut() {
                                    a.model_path = None;
                                }
                                s.llm.llm_initializing = true;
                            }
                            let _ = self.llm_tx.try_send(LlmInput::SwitchModel(path.clone()));
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

            // ── Pipeline progress (always visible — empty when idle) ───
            // Two stacked bars, no red tint.  Top = lane-completion
            // fraction; bottom = error-count fraction (same denominator,
            // visible only when failures > 0).  Errors render in a
            // dimmer gray, not red.
            let progress = self.state.read().llm.pipeline_progress.clone();

            ui.label(
                egui::RichText::new("PIPE")
                    .monospace()
                    .size(8.0)
                    .color(theme::ASH),
            );
            let bar_w = 180.0_f32;
            let bar_h = 3.0_f32;
            let bar_gap = 1.0_f32;
            let group_h = bar_h * 2.0 + bar_gap;
            let (group_rect, _) =
                ui.allocate_exact_size(egui::vec2(bar_w, group_h), egui::Sense::hover());
            let pa = ui.painter();
            let progress_rect = egui::Rect::from_min_size(group_rect.min, egui::vec2(bar_w, bar_h));
            let error_rect = egui::Rect::from_min_size(
                egui::pos2(group_rect.min.x, group_rect.min.y + bar_h + bar_gap),
                egui::vec2(bar_w, bar_h),
            );
            pa.rect_filled(progress_rect, 1.0, egui::Color32::from_gray(38));
            pa.rect_filled(error_rect, 1.0, egui::Color32::from_gray(38));

            let (frac, err_frac, total_failed) = match &progress {
                Some(p) if p.total_lanes > 0 => (
                    p.lanes_done as f32 / p.total_lanes as f32,
                    p.failed_count as f32 / p.total_lanes as f32,
                    p.failed_count,
                ),
                _ => (0.0, 0.0, 0),
            };
            let progress_w = (bar_w * frac.clamp(0.0, 1.0)).max(0.0);
            if progress_w > 0.0 {
                pa.rect_filled(
                    egui::Rect::from_min_size(progress_rect.min, egui::vec2(progress_w, bar_h)),
                    1.0,
                    egui::Color32::from_gray(140),
                );
            }
            let error_w = (bar_w * err_frac.clamp(0.0, 1.0)).max(0.0);
            if error_w > 0.0 {
                pa.rect_filled(
                    egui::Rect::from_min_size(error_rect.min, egui::vec2(error_w, bar_h)),
                    1.0,
                    egui::Color32::from_gray(95),
                );
            }

            let label = match &progress {
                Some(p) => match &p.current_lane {
                    Some(name) => format!("{}/{} {}", p.lanes_done + 1, p.total_lanes, name),
                    None if p.lanes_done >= p.total_lanes => {
                        format!("{}/{} done", p.total_lanes, p.total_lanes)
                    }
                    None => format!("{}/{} planning…", p.lanes_done, p.total_lanes),
                },
                None => "idle".to_string(),
            };
            let label = if total_failed > 0 {
                format!("{} · {} err", label, total_failed)
            } else {
                label
            };
            ui.label(egui::RichText::new(label).monospace().size(8.0).color(
                if progress.is_some() {
                    theme::FOG
                } else {
                    theme::IRON
                },
            ));
            // Keep animating while pipeline is live.
            if progress.is_some() {
                ui.ctx().request_repaint();
            }

            // Phase-1 lane scores: tiny per-lane summary so the user can
            // see the evaluator picking up signal in real time.  Order
            // matches the typical jam pipeline; only renders lanes that
            // have been scored at least once this session.
            {
                let scores = self.state.read().llm.lane_scores.clone();
                if !scores.is_empty() {
                    let order = [
                        "settings", "kit_a", "kit_b", "amen", "bass1", "bass2", "hoover", "an1x",
                        "fx",
                    ];
                    for label in order {
                        if let Some(s) = scores.get(label) {
                            // Map score → gray ramp so a glance reads
                            // brighter = better, dimmer = worse.
                            let g = (90.0 + s.score * 130.0) as u8;
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}{:.0}",
                                    &label[..label.len().min(2)],
                                    s.score * 100.0
                                ))
                                .monospace()
                                .size(7.0)
                                .color(egui::Color32::from_gray(g)),
                            );
                        }
                    }
                }
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
            // Round-robin cycle viz now lives in the right-side panel
            // (set up by `draw_llm_console_content`).
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

        // ── Seed row (mirrors STYLE: lock + editable value + random reset) ──
        ui.horizontal(|ui| {
            // Reuse `style_lock` as a "global broadcast lock" toggle?  No —
            // seed has its own lock semantics: when locked, agents won't
            // auto-pick up global seed changes.  Stored on each agent;
            // here the global "[L]" suffix is informational, indicating
            // whether changes propagate (i.e. NOT locked across all agents).
            let any_agent_locked = self
                .state
                .read()
                .llm_agents
                .iter()
                .any(|a| a.seed_locked);
            let lock_label = if any_agent_locked {
                "SEED [L]"
            } else {
                "SEED"
            };
            let lock_col = if any_agent_locked {
                theme::FOG
            } else {
                theme::ASH
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(lock_label)
                        .monospace()
                        .size(9.0)
                        .color(lock_col),
                )
                .selectable(false),
            )
            .on_hover_text(if any_agent_locked {
                "At least one agent has its own locked seed. Use the agent card to clear locks."
            } else {
                "Seed change propagates to every agent."
            });
            ui.add_space(4.0);
            let mut seed = self.state.read().llm.seed;
            if ui
                .add(
                    egui::DragValue::new(&mut seed)
                        .range(-1..=i64::MAX)
                        .speed(1)
                        .custom_formatter(|n, _| {
                            if (n as i64) < 0 {
                                "random".to_string()
                            } else {
                                format!("{}", n as i64)
                            }
                        }),
                )
                .on_hover_text("Random seed (-1 = randomise on every call). Propagates to all unlocked agents.")
                .changed()
            {
                let snap = self.state.read().clone();
                let new_state = crate::state::propagate_seed(snap, seed);
                *self.state.write() = new_state;
            }
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("RANDOM")
                            .monospace()
                            .size(8.0)
                            .color(theme::ASH),
                    )
                    .fill(egui::Color32::TRANSPARENT),
                )
                .on_hover_text("Reset to random (seed = -1)")
                .clicked()
            {
                let snap = self.state.read().clone();
                let new_state = crate::state::propagate_seed(snap, -1);
                *self.state.write() = new_state;
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
                self.send_llm_infer(prompt, true, None);
            } else {
                for aid in enabled_agents {
                    self.send_llm_infer(prompt.clone(), true, Some(aid));
                }
            }
            self.prompt_input.clear();
        }
    }
}
