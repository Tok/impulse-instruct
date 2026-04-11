// ─── ui/wizard.rs ─────────────────────────────────────────────────────────────
// First-launch startup wizard: GPU detection, agent preset selector, VRAM budget.

use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    /// Sentinel value for `wizard_selected` meaning "Resume last session".
    const WIZARD_RESUME: usize = usize::MAX;

    pub(super) fn draw_wizard_window(&mut self, ctx: &egui::Context) {
        if !self.show_wizard {
            return;
        }

        // Lazy-load available models for the wizard.
        if self.available_models.is_empty() {
            self.available_models = super::scan_models();
        }

        let si = self
            .sys_info
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default();

        let statuses = crate::llm::vram::check_presets(si.vram_total_mb, &self.available_models);

        // Check Enter key at ctx level — works regardless of widget focus
        let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));

        // Does the current state already have agents from a prior session?
        let has_prior_agents = !self.state.read().llm_agents.is_empty();
        let prior_agent_count = self.state.read().llm_agents.len();

        egui::Window::new("Agent Setup")
            .collapsible(false)
            .resizable(false)
            .min_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // ── GPU info ─────────────────────────────────────────────
                ui.label(
                    egui::RichText::new("GPU")
                        .color(theme::CHALK)
                        .monospace()
                        .size(10.5)
                        .strong(),
                );
                ui.separator();

                if si.gpu_name.is_empty() {
                    ui.label(
                        egui::RichText::new("No GPU detected — CPU inference (slow)")
                            .color(theme::IRON)
                            .monospace()
                            .size(9.5),
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&si.gpu_name)
                                .color(theme::FOG)
                                .monospace()
                                .size(9.5),
                        );
                    });
                    ui.horizontal(|ui| {
                        let frac = if si.vram_total_mb > 0 {
                            si.vram_used_mb as f32 / si.vram_total_mb as f32
                        } else {
                            0.0
                        };
                        ui.label(
                            egui::RichText::new("VRAM:")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.add_sized([120.0, 10.0], egui::ProgressBar::new(frac));
                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {}",
                                crate::sysinfo::fmt_mb(si.vram_used_mb),
                                crate::sysinfo::fmt_mb(si.vram_total_mb),
                            ))
                            .color(theme::FOG)
                            .monospace()
                            .size(9.5),
                        );
                    });
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("CONFIGURATION")
                        .color(theme::CHALK)
                        .monospace()
                        .size(10.5)
                        .strong(),
                );
                ui.separator();
                ui.add_space(2.0);

                // ── Resume last session (default when prior state exists) ─
                if has_prior_agents {
                    let is_resume = self.wizard_selected == Self::WIZARD_RESUME;
                    let text_color = if is_resume { theme::CHALK } else { theme::FOG };
                    let fill = if is_resume {
                        egui::Color32::from_gray(35)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let resp = ui.add(
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "Resume last session  ({} agent{})",
                                prior_agent_count,
                                if prior_agent_count == 1 { "" } else { "s" }
                            ))
                            .monospace()
                            .size(10.0)
                            .color(text_color),
                        )
                        .fill(fill)
                        .stroke(if is_resume {
                            egui::Stroke::new(1.0, theme::ASH)
                        } else {
                            egui::Stroke::new(1.0, egui::Color32::from_gray(24))
                        })
                        .rounding(egui::Rounding::same(3.0))
                        .min_size(egui::vec2(460.0, 28.0)),
                    );
                    if resp.clicked() {
                        self.wizard_selected = Self::WIZARD_RESUME;
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("  Or start fresh with a preset:")
                            .color(theme::IRON)
                            .monospace()
                            .size(8.5),
                    );
                    ui.add_space(2.0);
                }

                // ── Preset list ──────────────────────────────────────────
                for (i, status) in statuses.iter().enumerate() {
                    let selectable = status.fits_vram && status.models_available;
                    let selected = self.wizard_selected == i;

                    let name = status.preset.name;
                    let desc = status.preset.description;
                    let n_agents = status.preset.agents.len();
                    let vram_label =
                        format!("~{:.1}G", status.preset.total_vram_mb as f64 / 1024.0);

                    let name_color = if !selectable {
                        theme::IRON
                    } else if selected {
                        theme::CHALK
                    } else {
                        theme::FOG
                    };
                    let desc_color = if !selectable { theme::IRON } else { theme::ASH };

                    let fill = if selected && selectable {
                        egui::Color32::from_gray(30)
                    } else {
                        egui::Color32::from_gray(16)
                    };
                    let stroke = if selected && selectable {
                        egui::Stroke::new(1.0, theme::ASH)
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::from_gray(24))
                    };

                    let resp = ui.add_enabled(
                        selectable,
                        egui::Button::new({
                            let mut job = egui::text::LayoutJob::default();
                            job.append(
                                name,
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::monospace(10.0),
                                    color: name_color,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                &format!(
                                    "  {} agent{}  {}",
                                    n_agents,
                                    if n_agents == 1 { "" } else { "s" },
                                    vram_label
                                ),
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::monospace(8.5),
                                    color: desc_color,
                                    ..Default::default()
                                },
                            );
                            job.append(
                                &format!("\n{}", desc),
                                0.0,
                                egui::TextFormat {
                                    font_id: egui::FontId::monospace(8.5),
                                    color: desc_color,
                                    ..Default::default()
                                },
                            );
                            job
                        })
                        .fill(fill)
                        .stroke(stroke)
                        .min_size(egui::vec2(460.0, 32.0))
                        .rounding(egui::Rounding::same(3.0)),
                    );
                    if resp.clicked() && selectable {
                        self.wizard_selected = i;
                    }

                    if !status.models_available {
                        ui.label(
                            egui::RichText::new(format!(
                                "  download required: {}",
                                status.missing_models.join(", ")
                            ))
                            .monospace()
                            .size(7.5)
                            .color(theme::IRON),
                        );
                    } else if !status.fits_vram {
                        ui.label(
                            egui::RichText::new("  exceeds available VRAM")
                                .monospace()
                                .size(7.5)
                                .color(theme::IRON),
                        );
                    }
                }

                // ── VRAM budget bar for selected preset ──────────────────
                if let Some(status) = statuses.get(self.wizard_selected) {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let frac = if si.vram_total_mb > 0 {
                            status.preset.total_vram_mb as f32 / si.vram_total_mb as f32
                        } else {
                            0.0
                        };
                        ui.label(
                            egui::RichText::new("VRAM:")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.add_sized([120.0, 10.0], egui::ProgressBar::new(frac.min(1.0)));
                        ui.label(
                            egui::RichText::new(format!(
                                "{:.1}G / {}",
                                status.preset.total_vram_mb as f64 / 1024.0,
                                if si.vram_total_mb > 0 {
                                    crate::sysinfo::fmt_mb(si.vram_total_mb)
                                } else {
                                    "CPU".to_string()
                                },
                            ))
                            .color(theme::FOG)
                            .monospace()
                            .size(9.5),
                        );
                    });
                }

                ui.add_space(10.0);

                // ── Action buttons ───────────────────────────────────────
                let is_resume = self.wizard_selected == Self::WIZARD_RESUME;
                let can_apply = is_resume
                    || statuses
                        .get(self.wizard_selected)
                        .map(|s| s.fits_vram && s.models_available)
                        .unwrap_or(false);

                ui.horizontal(|ui| {
                    let label = if is_resume {
                        "Resume"
                    } else if has_prior_agents {
                        "Apply"
                    } else {
                        "Start"
                    };
                    let clicked = ui
                        .add_enabled(
                            can_apply,
                            egui::Button::new(
                                egui::RichText::new(label)
                                    .monospace()
                                    .size(10.0)
                                    .color(if can_apply { theme::CHALK } else { theme::IRON }),
                            )
                            .min_size(egui::vec2(100.0, 26.0)),
                        )
                        .clicked();
                    if clicked || (enter_pressed && can_apply) {
                        if is_resume {
                            self.mark_wizard_done();
                        } else {
                            self.apply_wizard_preset(self.wizard_selected);
                        }
                        self.show_wizard = false;
                        // Auto-play: start the sequencer after wizard completes
                        if !self.state.read().sequencer.running {
                            let s = self.state.read().clone();
                            *self.state.write() = crate::state::toggle_sequencer_running(s);
                        }
                    }
                });

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("You can add/remove agents anytime from the rack.")
                        .color(theme::IRON)
                        .monospace()
                        .size(8.0),
                );
            });
    }

    /// Apply a wizard preset: spawn agents with the right models/personas/scopes.
    fn apply_wizard_preset(&mut self, preset_idx: usize) {
        use crate::llm::vram::{PRESETS, find_model};
        let preset = match PRESETS.get(preset_idx) {
            Some(p) => p,
            None => return,
        };

        // Remove all existing agents — wizard applies a clean setup.
        {
            let old_ids: Vec<u32> = self.state.read().llm_agents.iter().map(|a| a.id).collect();
            let mut s = self.state.write();
            for id in &old_ids {
                s.rack.remove_module(*id);
            }
            s.llm_agents.clear();
        }

        for pa in preset.agents {
            let model_path = find_model(pa.model_pattern, &self.available_models);
            let scope: Vec<String> = pa.scope.iter().map(|s| s.to_string()).collect();

            let snapshot = self.state.read().clone();
            let (new_state, _id) =
                crate::state::spawn_agent(snapshot, pa.persona, &scope, pa.role, model_path);
            *self.state.write() = new_state;
        }

        self.mark_wizard_done();
        self.push_fx_plan();
        self.session_dirty = true;
    }

    /// Persist wizard_done so it doesn't show again.
    fn mark_wizard_done(&self) {
        let state = self.state.read().clone();
        crate::state::save_session_ext(&state, self.show_cables, self.rack_flipped, Some(true));
    }
}
