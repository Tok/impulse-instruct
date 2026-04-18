// ─── ui/agent_card.rs ────────────────────────────────────────────────────────
// Per-agent card content — extracted from rack_content.rs to keep file sizes
// under the 1000-line cap.  Pure UI for the LLM agent module: persona, model,
// status, conv mode, scope, role, style, quick-command pills, instructions,
// system-prompt override.

use crate::ui::ImpulseApp;

// ─── LLM agent card content ──────────────────────────────────────────────────

pub(super) fn draw_llm_agent_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // Width is constrained by the grid; height auto-sizes to content.
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

    // ── Persona + model + status (single line) ────────────────────────────
    ui.horizontal(|ui| {
        if inferring {
            let t = ui.ctx().input(|i| i.time) as f32;
            let pulse = (t * 4.0 * std::f32::consts::TAU).sin() * 0.3 + 0.7;
            let g = (220.0 * pulse) as u8;
            ui.label(
                egui::RichText::new("●")
                    .color(egui::Color32::from_gray(g))
                    .size(10.0),
            );
            ui.ctx().request_repaint();
        } else {
            ui.label(egui::RichText::new("●").color(theme::ASH).size(10.0));
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
                        app.state.write().llm_agents[idx].model_path = None;
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
                                egui::RichText::new(&short)
                                    .monospace()
                                    .size(8.0)
                                    .color(if selected { theme::CHALK } else { theme::SMOKE }),
                            )
                            .clicked()
                        {
                            app.state.write().llm_agents[idx].model_path = Some(path.clone());
                        }
                    }
                });
        }
        // Status: tok/s or cycle count
        if inferring {
            ui.label(
                egui::RichText::new(format!("{:.0}t/s", tps))
                    .color(theme::FOG)
                    .monospace()
                    .size(7.5),
            );
        } else if cycles > 0 {
            ui.label(
                egui::RichText::new(format!("#{}", cycles))
                    .color(theme::IRON)
                    .monospace()
                    .size(7.5),
            );
        }
    });

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
    // ── User instructions ───────────────────────────────────────────────
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
    ui.label(
        egui::RichText::new(format!("{:.1} t/s  #{}", tps, cycles))
            .color(theme::ASH)
            .monospace()
            .size(7.5),
    );
}
