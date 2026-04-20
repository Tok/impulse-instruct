// ─── ui/agent_card.rs ────────────────────────────────────────────────────────
// Per-agent card content — extracted from rack_content.rs to keep file sizes
// under the 1000-line cap.  Pure UI for the LLM agent module: persona, model,
// status, conv mode, scope, role, style, quick-command pills, instructions,
// system-prompt override.

use crate::ui::ImpulseApp;

// ─── LLM agent card content ──────────────────────────────────────────────────

pub(super) fn draw_llm_agent_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // Width is constrained by the grid; height auto-sizes to content.
    // All controls render in the card's natural top-down flow — the
    // clock sits inline with the persona/model row (right-justified,
    // scaled up ≈3× its old 26 px), and instructions / t/s / prompt
    // override span the full card width.
    draw_llm_agent_inner(app, ui, module_id);
}

fn draw_llm_agent_inner(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    use crate::ui::theme;

    let agent_idx = {
        let s = app.state.read();
        s.llm_agents.iter().position(|a| a.id == module_id)
    };
    let Some(idx) = agent_idx else { return };

    // Snapshot mutable fields for editing
    let (
        mut persona,
        mut temp,
        mut jam_bars,
        last_resp,
        tps,
        cycles,
        inferring,
        agent_model,
        conv_mode,
        active_style,
        enable_thinking,
        mut user_instructions,
        mut prompt_override,
        role,
        can_spawn,
        can_dismiss,
        mut seed,
        seed_locked,
    ) = {
        let s = app.state.read();
        let a = &s.llm_agents[idx];
        (
            a.persona_name.clone(),
            a.temperature,
            a.jam_bars,
            a.last_response.clone(),
            a.tokens_per_sec,
            a.jam_cycle_count,
            a.is_inferring,
            a.model_path.clone(),
            a.conversation_mode.clone(),
            a.active_style.clone(),
            a.enable_thinking,
            a.user_instructions.clone(),
            a.system_prompt_override.clone(),
            a.role,
            a.can_spawn,
            a.can_dismiss,
            a.seed,
            a.seed_locked,
        )
    };

    // ── Top block: [persona/model, progress, T/B, Scope] on the left
    // and a big round-robin clock on the right, spanning the full height
    // of the left column.  `horizontal_top` keeps the clock anchored at
    // the top-right while the left column flows downward.
    let clock_size: f32 = 80.0;
    let clock_gap: f32 = 6.0;
    let left_max_w = (ui.available_width() - clock_size - clock_gap).max(120.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_max_width(left_max_w);
            // ── Persona + model + status (single line) ────────────────────────────
            ui.horizontal(|ui| {
                // White LED indicator — pulses when this agent is inferring,
                // dim/idle otherwise.  Replaces the old text-bullet glyph.
                // Painted on a foreground layer so the LED's halo is never
                // covered by the persona TextEdit (or other widgets) drawn
                // afterwards in the same horizontal row.
                let led_size = egui::vec2(12.0, 12.0);
                let (led_rect, _) = ui.allocate_exact_size(led_size, egui::Sense::hover());
                let intensity = if inferring {
                    let t = ui.ctx().input(|i| i.time) as f32;
                    (t * 4.0 * std::f32::consts::PI).sin() * 0.25 + 0.75
                } else {
                    0.30
                };
                // Foreground layer so the LED halo lands on top of the
                // persona TextEdit + model combo, but clipped to the
                // CentralPanel's rect so the halo can't escape upward
                // into the header log panel when the agent card scrolls
                // past the rack's top edge.
                let mut overlay = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    ui.id().with("agent_led").with(module_id),
                ));
                overlay.set_clip_rect(ui.ctx().available_rect());
                theme::led(
                    &overlay,
                    led_rect.center(),
                    5.0,
                    egui::Color32::from_rgb(230, 230, 230),
                    intensity,
                );
                if inferring {
                    ui.ctx().request_repaint();
                }
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut persona)
                        .desired_width(80.0)
                        .font(egui::FontId::monospace(9.5))
                        .text_color(theme::FOG),
                );
                if resp.changed() {
                    app.state.write().llm_agents[idx].persona_name = persona;
                }
                // Model dropdown (inline)
                {
                    if app.available_models.is_empty() {
                        app.available_models = super::scan_models();
                    }
                    let display_name = match &agent_model {
                        Some(p) => std::path::Path::new(p)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(p)
                            .to_string(),
                        None => "(Def)".to_string(),
                    };
                    let combo_id = ui.id().with("agent_model").with(module_id);
                    egui::ComboBox::from_id_source(combo_id)
                        .selected_text(
                            egui::RichText::new(&display_name)
                                .color(theme::SMOKE)
                                .size(7.5)
                                .monospace(),
                        )
                        .width(ui.available_width().min(100.0))
                        .show_ui(ui, |ui| {
                            let is_default = agent_model.is_none();
                            if ui
                                .selectable_label(
                                    is_default,
                                    egui::RichText::new("(Default)")
                                        .monospace()
                                        .size(8.0)
                                        .color(if is_default {
                                            theme::CHALK
                                        } else {
                                            theme::SMOKE
                                        }),
                                )
                                .clicked()
                            {
                                // Optimistic UI: write the new value immediately so
                                // the dropdown reflects the click this frame, even
                                // if the LLM thread is mid-inference and won't drain
                                // its queue for tens of seconds.  The message
                                // carries the previous override so the LLM thread
                                // can release the old model from the pool.
                                let old = app.state.write().llm_agents[idx].model_path.take();
                                let _ =
                                    app.llm_tx.try_send(crate::llm::LlmInput::SwitchAgentModel {
                                        agent_id: module_id,
                                        old_path: old,
                                        new_path: None,
                                    });
                            }
                            for path in &app.available_models.clone() {
                                let short = std::path::Path::new(path)
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(path)
                                    .to_string();
                                let selected = agent_model.as_deref() == Some(path.as_str());
                                if ui
                                    .selectable_label(
                                        selected,
                                        egui::RichText::new(&short).monospace().size(8.0).color(
                                            if selected { theme::CHALK } else { theme::SMOKE },
                                        ),
                                    )
                                    .clicked()
                                {
                                    let old = {
                                        let mut s = app.state.write();
                                        let prev = s.llm_agents[idx].model_path.take();
                                        s.llm_agents[idx].model_path = Some(path.clone());
                                        prev
                                    };
                                    let _ = app.llm_tx.try_send(
                                        crate::llm::LlmInput::SwitchAgentModel {
                                            agent_id: module_id,
                                            old_path: old,
                                            new_path: Some(path.clone()),
                                        },
                                    );
                                }
                            }
                        });
                }
            });
            // Lane-progress sub-label — the big agent_clock shows the dial;
            // this keeps the per-lane "{done}/{total} {name}" tick visible
            // underneath the persona/model row.  Always rendered (falls
            // back to "idle" when no pipeline is live) so the rows below
            // don't shift up/down as inference starts and stops.  Matches
            // the LLM console's semantics so the two displays read alike.
            let agent_progress = app.state.read().llm_agents[idx].pipeline_progress.clone();
            let (label, label_color) = match &agent_progress {
                Some(p) => {
                    let mut s = match &p.current_lane {
                        Some(name) => format!("{}/{} {}", p.lanes_done + 1, p.total_lanes, name),
                        None if p.lanes_done >= p.total_lanes => {
                            format!("{}/{} done", p.total_lanes, p.total_lanes)
                        }
                        None => format!("{}/{} plan…", p.lanes_done, p.total_lanes),
                    };
                    if p.failed_count > 0 {
                        s = format!("{} · {}e", s, p.failed_count);
                    }
                    (s, theme::FOG)
                }
                None => ("idle".to_string(), theme::IRON),
            };
            ui.label(
                egui::RichText::new(label)
                    .color(label_color)
                    .monospace()
                    .size(7.5),
            );

            // ── Temp / Bars controls ───────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    egui::RichText::new("T")
                        .color(theme::ASH)
                        .monospace()
                        .size(8.0),
                );
                if ui
                    .add(
                        egui::DragValue::new(&mut temp)
                            .range(0.0..=2.0)
                            .speed(0.01)
                            .fixed_decimals(2),
                    )
                    .changed()
                {
                    app.state.write().llm_agents[idx].temperature = temp;
                }
                ui.label(
                    egui::RichText::new("B")
                        .color(theme::ASH)
                        .monospace()
                        .size(8.0),
                );
                if ui
                    .add(
                        egui::DragValue::new(&mut jam_bars)
                            .range(0.0..=16.0)
                            .speed(0.5)
                            .fixed_decimals(0),
                    )
                    .on_hover_text("Bars between jam cycles (0 = continuous)")
                    .changed()
                {
                    app.state.write().llm_agents[idx].jam_bars = jam_bars;
                }
            });

            // ── Scope (derived from control cables) ────────────────────────────
            {
                let cable_scope =
                    crate::state::scope_from_control_cables(&app.state.read().rack, module_id);
                let label = if cable_scope.is_empty() {
                    "SCOPE: ALL".to_string()
                } else {
                    format!("SCOPE: {}", cable_scope.join(" "))
                };
                ui.label(
                    egui::RichText::new(label)
                        .monospace()
                        .size(7.5)
                        .color(theme::IRON),
                );
            }
        }); // end left vertical
        // Push the clock to the right edge of the card so it stays flush
        // with the card border regardless of how much room the left
        // column actually consumes.
        let spacer = (ui.available_width() - clock_size).max(0.0);
        if spacer > 0.5 {
            ui.add_space(spacer);
        }
        let secs_to_next_fire =
            app.jam_next_fire
                .and_then(|(at, pending_agent)| match pending_agent {
                    Some(aid) if aid == module_id => Some(
                        at.saturating_duration_since(std::time::Instant::now())
                            .as_secs_f32(),
                    ),
                    _ => None,
                });
        let snapshot = app.state.read();
        super::widgets::agent_clock(ui, &snapshot, module_id, secs_to_next_fire, clock_size);
    }); // end horizontal_top
    // ── Conversation mode + thinking ───────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        use crate::state::ConversationMode;
        for (label, mode) in &[
            ("OFF", ConversationMode::Off),
            ("PRODUCER", ConversationMode::Producer),
            ("DJ", ConversationMode::Dj),
            ("MC", ConversationMode::Mc),
        ] {
            let active = conv_mode == *mode;
            let col = if active { theme::FOG } else { theme::SMOKE };
            let fill = if active {
                egui::Color32::from_gray(40)
            } else {
                theme::DEEP
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new(*label).monospace().size(7.5).color(col))
                        .fill(fill)
                        .min_size(egui::vec2(22.0, 14.0)),
                )
                .on_hover_text(match mode {
                    ConversationMode::Off => "No commentary",
                    ConversationMode::Producer => "Technical descriptions",
                    ConversationMode::Dj => "Hype DJ energy",
                    ConversationMode::Mc => "Jungle/rave MC",
                })
                .clicked()
            {
                app.state.write().llm_agents[idx].conversation_mode = mode.clone();
            }
        }
        ui.separator();
        // Thinking toggle
        let think_col = if enable_thinking {
            theme::FOG
        } else {
            theme::SMOKE
        };
        let think_fill = if enable_thinking {
            egui::Color32::from_gray(40)
        } else {
            theme::DEEP
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("THINK")
                        .monospace()
                        .size(7.5)
                        .color(think_col),
                )
                .fill(think_fill)
                .min_size(egui::vec2(22.0, 14.0)),
            )
            .on_hover_text(if enable_thinking {
                "Reasoning ON (/think)"
            } else {
                "Reasoning OFF (/no_think)"
            })
            .clicked()
        {
            app.state.write().llm_agents[idx].enable_thinking = !enable_thinking;
        }
    });
    // ── Role + autonomy permissions ─────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        use crate::state::AgentRole;
        let role_btn = |ui: &mut egui::Ui, label, r: &AgentRole| -> bool {
            let active = role == *r;
            let col = if active { theme::FOG } else { theme::SMOKE };
            let fill = egui::Color32::from_gray(if active { 40 } else { 18 });
            ui.add(
                egui::Button::new(egui::RichText::new(label).monospace().size(7.5).color(col))
                    .fill(fill)
                    .min_size(egui::vec2(22.0, 14.0)),
            )
            .clicked()
        };
        if role_btn(ui, "PRODUCER", &AgentRole::Producer) {
            app.state.write().llm_agents[idx].role = AgentRole::Producer;
        }
        if role_btn(ui, "MC", &AgentRole::Mc) {
            app.state.write().llm_agents[idx].role = AgentRole::Mc;
        }
        if role_btn(ui, "DJ", &AgentRole::Dj) {
            app.state.write().llm_agents[idx].role = AgentRole::Dj;
        }
        if role_btn(ui, "SPC", &AgentRole::Specialist) {
            app.state.write().llm_agents[idx].role = AgentRole::Specialist;
        }
        ui.separator();
        let perm_btn = |ui: &mut egui::Ui, label, on: bool, tip_on: &str, tip_off: &str| -> bool {
            let col = if on { theme::FOG } else { theme::IRON };
            let fill = egui::Color32::from_gray(if on { 35 } else { 18 });
            ui.add(
                egui::Button::new(egui::RichText::new(label).monospace().size(7.0).color(col))
                    .fill(fill)
                    .min_size(egui::vec2(24.0, 14.0)),
            )
            .on_hover_text(if on { tip_on } else { tip_off })
            .clicked()
        };
        if perm_btn(
            ui,
            "+AG",
            can_spawn,
            "Can spawn agents",
            "Cannot spawn agents",
        ) {
            app.state.write().llm_agents[idx].can_spawn = !can_spawn;
        }
        if perm_btn(ui, "BYE", can_dismiss, "Can sign off", "Cannot sign off") {
            app.state.write().llm_agents[idx].can_dismiss = !can_dismiss;
        }
    });
    // ── Seed (lock + value + random) ────────────────────────────────────
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let lock_label = if seed_locked { "SEED [L]" } else { "SEED" };
        let lock_col = if seed_locked { theme::FOG } else { theme::ASH };
        let lock_fill = egui::Color32::from_gray(if seed_locked { 35 } else { 18 });
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(lock_label)
                        .monospace()
                        .size(7.5)
                        .color(lock_col),
                )
                .fill(lock_fill)
                .min_size(egui::vec2(36.0, 14.0)),
            )
            .on_hover_text(if seed_locked {
                "Seed locked — global seed changes won't affect this agent. Click to unlock."
            } else {
                "Seed follows global. Click to lock independently."
            })
            .clicked()
        {
            app.state.write().llm_agents[idx].seed_locked = !seed_locked;
        }
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
            .on_hover_text("Per-agent seed (-1 = randomise every call)")
            .changed()
        {
            app.state.write().llm_agents[idx].seed = seed;
        }
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("RAND")
                        .monospace()
                        .size(7.0)
                        .color(theme::ASH),
                )
                .fill(egui::Color32::TRANSPARENT)
                .min_size(egui::vec2(24.0, 14.0)),
            )
            .on_hover_text("Reset to random (-1)")
            .clicked()
        {
            app.state.write().llm_agents[idx].seed = -1;
        }
    });
    // ── Style selector ──────────────────────────────────────────────────
    {
        use crate::llm::styles::StyleCatalog;
        let cat = StyleCatalog::get();
        let display = match &active_style {
            None => "No style".to_string(),
            Some(s) if s == "__free__" => "Free".to_string(),
            Some(s) if s == "__custom__" => "Custom".to_string(),
            Some(s) => cat
                .find_by_id(s)
                .map(|e| e.name.to_string())
                .unwrap_or_else(|| s.clone()),
        };
        let combo_id = ui.id().with("agent_style").with(module_id);
        egui::ComboBox::from_id_source(combo_id)
            .selected_text(
                egui::RichText::new(&display)
                    .color(theme::SMOKE)
                    .size(7.5)
                    .monospace(),
            )
            .width(ui.available_width().min(140.0))
            .show_ui(ui, |ui| {
                // No style
                if ui
                    .selectable_label(
                        active_style.is_none(),
                        egui::RichText::new("No style")
                            .monospace()
                            .size(8.0)
                            .color(theme::SMOKE),
                    )
                    .clicked()
                {
                    app.state.write().llm_agents[idx].active_style = None;
                }
                // Free
                if ui
                    .selectable_label(
                        active_style.as_deref() == Some("__free__"),
                        egui::RichText::new("Free")
                            .monospace()
                            .size(8.0)
                            .color(theme::SMOKE),
                    )
                    .clicked()
                {
                    app.state.write().llm_agents[idx].active_style = Some("__free__".to_string());
                }
                // Catalog entries
                for entry in cat.styles() {
                    let selected = active_style.as_deref() == Some(entry.id.as_str());
                    if ui
                        .selectable_label(
                            selected,
                            egui::RichText::new(&entry.name)
                                .monospace()
                                .size(8.0)
                                .color(if selected { theme::CHALK } else { theme::SMOKE }),
                        )
                        .clicked()
                    {
                        app.state.write().llm_agents[idx].active_style = Some(entry.id.to_string());
                    }
                }
            });
    }
    // ── Quick-command pills (one-click re-prompts) ──────────────────────
    super::agent_pills::draw_pills(app, ui, module_id);
    // ── User instructions (full width) + t/s badge directly underneath ──
    let instr_resp = ui.add(
        egui::TextEdit::multiline(&mut user_instructions)
            .desired_rows(2)
            .desired_width(ui.available_width())
            .font(egui::FontId::monospace(7.5))
            .text_color(theme::FOG)
            .hint_text("Instructions…"),
    );
    if instr_resp.changed() {
        app.state.write().llm_agents[idx].user_instructions = user_instructions;
    }
    ui.label(
        egui::RichText::new(format!("{:.1} t/s  #{}", tps, cycles))
            .color(theme::ASH)
            .monospace()
            .size(7.5),
    );
    // ── System prompt override ──────────────────────────────────────────
    let has_override = !prompt_override.is_empty();
    let ovr_header = if has_override {
        "▸ Prompt override (active)"
    } else {
        "▸ Prompt override"
    };
    let header_col = if has_override {
        theme::FOG
    } else {
        theme::IRON
    };
    let collapse_id = ui.id().with("prompt_ovr").with(module_id);
    egui::CollapsingHeader::new(
        egui::RichText::new(ovr_header)
            .monospace()
            .size(7.5)
            .color(header_col),
    )
    .id_source(collapse_id)
    .default_open(false)
    .show(ui, |ui| {
        let ovr_resp = ui.add(
            egui::TextEdit::multiline(&mut prompt_override)
                .desired_rows(3)
                .desired_width(ui.available_width())
                .font(egui::FontId::monospace(7.5))
                .text_color(theme::FOG)
                .hint_text("Leave empty for auto-generated prompt…"),
        );
        if ovr_resp.changed() {
            app.state.write().llm_agents[idx].system_prompt_override = prompt_override;
        }
    });
    super::rack_ai::draw_last_response_preview(ui, &last_resp);
}
