// ─── ui/wizard.rs ─────────────────────────────────────────────────────────────
// First-launch startup wizard: GPU detection, agent preset selector, VRAM budget.

use crate::ui::{ImpulseApp, theme};

impl ImpulseApp {
    /// Sentinel value for `wizard_selected` meaning "Resume last session".
    pub(super) const WIZARD_RESUME: usize = usize::MAX;

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
        let active_style = self.state.read().llm.active_style.clone();

        // Keyboard: Enter submits, Up/Down or W/S navigate presets
        let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let nav_up =
            ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::W));
        let nav_down =
            ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::S));

        // Does the current state already have agents from a prior session?
        let has_prior_agents = !self.state.read().llm_agents.is_empty();
        let prior_agent_count = self.state.read().llm_agents.len();

        // Keyboard navigation: Up/Down cycles through options
        let n_presets = statuses.len();
        if nav_down {
            if self.wizard_selected == Self::WIZARD_RESUME {
                self.wizard_selected = 0; // resume → first preset
            } else if self.wizard_selected + 1 < n_presets {
                self.wizard_selected += 1;
            } else if has_prior_agents {
                self.wizard_selected = Self::WIZARD_RESUME;
            }
        }
        if nav_up {
            if self.wizard_selected == Self::WIZARD_RESUME {
                self.wizard_selected = n_presets.saturating_sub(1);
            } else if self.wizard_selected > 0 {
                self.wizard_selected -= 1;
            } else if has_prior_agents {
                self.wizard_selected = Self::WIZARD_RESUME;
            }
        }

        // Track whether we should apply this frame (click or Enter)
        let mut apply_choice: Option<usize> = None;

        let width = (ctx.content_rect().width() - 48.0).min(1060.0);
        egui::Window::new("Rack Setup")
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(
                width,
                (ctx.content_rect().height() - 64.0).min(420.0),
            ))
            .scroll([false, false])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 5.0);
                ui.columns(2, |columns| {
                    let left = &mut columns[0];
                    self.draw_wizard_style_picker(left);
                    left.add_space(12.0);
                    left.separator();
                    left.label(egui::RichText::new("General rack").size(13.0).strong());
                    left.horizontal_wrapped(|ui| {
                        for (i, preset) in crate::state::RACK_PRESETS.iter().enumerate() {
                            if preset.name == "Full + Viz" {
                                continue;
                            }
                            let selected =
                                self.wizard_style_id.is_none() && self.wizard_rack_preset == i;
                            if ui
                                .selectable_label(selected, preset.name)
                                .on_hover_text(preset.description)
                                .clicked()
                            {
                                self.wizard_style_id = None;
                                self.wizard_rack_preset = i;
                                if self.wizard_selected == Self::WIZARD_RESUME {
                                    self.wizard_selected = 0;
                                }
                            }
                        }
                    });
                    left.add_space(8.0);
                    left.checkbox(&mut self.wizard_include_viz, "Include visualizations")
                        .on_hover_text(
                            "Level meters, spectrum, oscilloscope, note history and beat display.",
                        );

                    let right = &mut columns[1];
                    right.label(egui::RichText::new("AI agents").size(16.0).strong());
                    right.label(
                        egui::RichText::new("Choose who plays. Start when your setup is ready.")
                            .size(11.0)
                            .color(theme::FOG),
                    );
                    right.add_space(6.0);
                    if has_prior_agents {
                        let selected = self.wizard_selected == Self::WIZARD_RESUME;
                        let marker = if selected { "●" } else { "○" };
                        if right
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!(
                                        "{marker}  Resume last session · {prior_agent_count} agents"
                                    ))
                                    .size(11.0)
                                    .color(if selected {
                                        theme::VOID
                                    } else {
                                        theme::CHALK
                                    }),
                                )
                                .fill(if selected { theme::CHALK } else { theme::PIT })
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if selected { theme::CHALK } else { theme::SLATE },
                                ))
                                .min_size(egui::vec2(right.available_width(), 30.0)),
                            )
                            .clicked()
                        {
                            self.wizard_selected = Self::WIZARD_RESUME;
                        }
                        right.separator();
                    }
                    for (i, status) in statuses.iter().enumerate() {
                        let available = status.fits_vram && status.models_available;
                        let style = self.wizard_style_id.as_deref().or(active_style.as_deref());
                        let name = crate::llm::vram::styled_preset_name(status.preset.name, style);
                        let label = format!(
                            "{}   ·   {} agents   ·   {:.1} GB",
                            name,
                            status.preset.agents.len(),
                            status.preset.total_vram_mb as f64 / 1024.0
                        );
                        let hint = if !status.models_available {
                            format!(
                                "{}\nDownload required: {}",
                                status.preset.description,
                                status.missing_models.join(", ")
                            )
                        } else if !status.fits_vram {
                            format!(
                                "{}\nExceeds available GPU memory",
                                status.preset.description
                            )
                        } else {
                            status.preset.description.to_string()
                        };
                        let selected = self.wizard_selected == i;
                        if right
                            .add_enabled(
                                available,
                                egui::Button::new(
                                    egui::RichText::new(format!(
                                        "{}  {label}",
                                        if selected { "●" } else { "○" }
                                    ))
                                    .size(11.0)
                                    .color(if selected {
                                        theme::VOID
                                    } else {
                                        theme::CHALK
                                    }),
                                )
                                .fill(if selected { theme::CHALK } else { theme::PIT })
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if selected { theme::CHALK } else { theme::SLATE },
                                ))
                                .min_size(egui::vec2(right.available_width(), 30.0)),
                            )
                            .on_hover_text(hint)
                            .clicked()
                        {
                            self.wizard_selected = i;
                        }
                    }
                    right.add_space(10.0);
                    right.separator();
                    let hardware = if si.gpu_name.is_empty() {
                        "CPU inference".to_string()
                    } else {
                        format!(
                            "{} · {} VRAM",
                            si.gpu_name,
                            crate::sysinfo::fmt_mb(si.vram_total_mb)
                        )
                    };
                    right.label(egui::RichText::new(hardware).size(10.0).color(theme::SMOKE));
                });
                ui.add_space(12.0);
                ui.separator();
                let resume = self.wizard_selected == Self::WIZARD_RESUME;
                let can_apply = (resume && has_prior_agents)
                    || statuses
                        .get(self.wizard_selected)
                        .is_some_and(|status| status.fits_vram && status.models_available);
                let selection = if resume {
                    "Last session".to_string()
                } else if let Some(id) = &self.wizard_style_id {
                    crate::llm::styles::StyleCatalog::get()
                        .find_by_id(id)
                        .map(|style| style.name.clone())
                        .unwrap_or_default()
                } else {
                    crate::state::RACK_PRESETS[self.wizard_rack_preset]
                        .name
                        .to_string()
                };
                let selection = if let Some(status) = statuses.get(self.wizard_selected) {
                    let agents = crate::llm::vram::styled_preset_name(
                        status.preset.name,
                        self.wizard_style_id.as_deref().or(active_style.as_deref()),
                    );
                    format!("{selection} · {agents}")
                } else {
                    selection
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(selection).size(12.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if resume {
                            "Resume session"
                        } else {
                            "Start new setup"
                        };
                        if ui
                            .add_enabled(
                                can_apply,
                                egui::Button::new(label).min_size(egui::vec2(150.0, 30.0)),
                            )
                            .clicked()
                            || (enter_pressed && can_apply)
                        {
                            apply_choice = Some(self.wizard_selected);
                        }
                    });
                });
            });

        // Apply the chosen preset/resume (outside the window closure)
        if let Some(choice) = apply_choice {
            if choice == Self::WIZARD_RESUME {
                self.mark_wizard_done();
            } else {
                self.apply_wizard_preset(choice);
            }
            self.show_wizard = false;
            if !self.state.read().sequencer.running {
                let s = self.state.read().clone();
                *self.state.write() = crate::state::toggle_sequencer_running(s);
            }
        }
    }

    /// Apply a wizard preset: rebuild the rack from the rack preset, then
    /// spawn agents with the right models/personas/scopes.
    fn apply_wizard_preset(&mut self, agent_preset_idx: usize) {
        use crate::llm::vram::{PRESETS, find_model};
        let agent_preset = match PRESETS.get(agent_preset_idx) {
            Some(p) => p,
            None => return,
        };

        // Apply rack layout — style pick takes precedence over the
        // generic RACK_PRESETS row.  When a style is seeded we still
        // start from an empty base rack so the style's rack_modules
        // becomes the exact spec, then stamp the style's baseline
        // params + mark it active so the first LLM cycle inherits it.
        if let Some(style_id) = self.wizard_style_id.clone() {
            let catalog = crate::llm::styles::StyleCatalog::get();
            if let Some(style) = catalog.find_by_id(&style_id) {
                // Reset to an empty base rack (Sequencer + Master + Console)
                // then let style_rack::apply add every module from rack_modules.
                let empty_preset = crate::state::RACK_PRESETS
                    .iter()
                    .find(|p| p.name == "Empty")
                    .unwrap_or(&crate::state::RACK_PRESETS[0]);
                let base_rack = crate::state::RackState::from_preset(empty_preset);
                {
                    let mut s = self.state.write();
                    s.llm_agents.clear();
                    s.rack = base_rack;
                    // Stamp baseline params so the style's resonance /
                    // decay / fx values land immediately — mirrors the
                    // LLM-console style-dropdown flow.
                    if let Some(baseline) = &style.baseline_params {
                        let owned = std::mem::take(&mut *s);
                        *s = crate::state::apply_llm_update(owned, baseline, &[]);
                    }
                    s.llm.active_style = Some(style_id.clone());
                    s.sequencer.pattern_style = Some(style_id.clone());
                    let owned = std::mem::take(&mut *s);
                    *s = crate::llm::jungle::prepare_intro(owned);
                }
                // Reshape the rack via the existing style pipeline.
                let names: Vec<String> = style.rack_modules.clone();
                crate::ui::style_rack::apply(self, &names);
                // style_rack::apply already enables the declared bassN count.
                // A single-sub style must not gain an extra bass voice here.
            }
        } else if let Some(rack_preset) = crate::state::RACK_PRESETS.get(self.wizard_rack_preset) {
            let new_rack = crate::state::RackState::from_preset(rack_preset);
            // Enable a second 303 voice whenever AcidBass is in the preset
            // — every preset that has bass benefits from two melodic lanes.
            let has_bass = rack_preset
                .voices
                .contains(&crate::state::ModuleKind::AcidBass);
            let mut s = self.state.write();
            // Preserve LLM agents — they'll be replaced below
            s.llm_agents.clear();
            s.rack = new_rack;
            if has_bass {
                if s.sequencer.bass_voice_enabled.len() > 1 {
                    s.sequencer.bass_voice_enabled[1] = true;
                }
                if let Some(v) = s.bass_voices.get_mut(1) {
                    v.enabled = true;
                }
            }
        }

        // Remove the default agent added by from_preset — we'll add preset agents
        {
            let agent_ids: Vec<u32> = self
                .state
                .read()
                .rack
                .modules
                .iter()
                .filter(|m| m.kind == crate::state::ModuleKind::LlmAgent)
                .map(|m| m.id)
                .collect();
            let mut s = self.state.write();
            for id in agent_ids {
                s.rack.remove_module(id);
            }
        }

        // Spawn agents from the agent preset.
        //
        // Pattern resolution:
        //   * If the preset's `model_pattern` already matches the user's
        //     selected global model, leave `model_path = None` so the
        //     agent inherits — avoids loading a SECOND llama-server for
        //     a model the user didn't pick (e.g. preset says "gemma",
        //     global is `gemma-4-E4B-it-Q4_K_M.gguf`, and a stale
        //     `gemma-4-26B-A4B-it-UD-IQ2_XXS.gguf` happens to sort
        //     alphabetically first → `find_model("gemma", ...)` would
        //     pin every agent to IQ2 + spin up a second server).
        //   * Only when the pattern does NOT match the global does the
        //     preset really want a different model — then we resolve
        //     explicitly.  None of the curated presets do this today,
        //     but the affordance is preserved for future per-agent
        //     model assignments.
        let global_model_lower = self.state.read().llm.model_path.to_ascii_lowercase();
        for pa in agent_preset.agents {
            let model_path = if global_model_lower.contains(pa.model_pattern) {
                None
            } else {
                find_model(pa.model_pattern, &self.available_models)
            };
            let scope: Vec<String> = pa.scope.iter().map(|s| s.to_string()).collect();

            let snapshot = self.state.read().clone();
            let (new_state, _id) =
                crate::state::spawn_agent(snapshot, pa.persona, &scope, pa.role, model_path);
            *self.state.write() = new_state;
        }

        let snapshot = self.state.read().clone();
        *self.state.write() = crate::llm::jungle_mc::configure(snapshot);

        if self.wizard_include_viz {
            let snapshot = self.state.read().clone();
            *self.state.write() = crate::state::add_setup_visualizations(snapshot);
        }

        // Re-run grid placement so newly spawned agents get valid positions
        self.state.write().rack.arrange_grid();

        self.mark_wizard_done();
        self.push_fx_plan();
        self.session_dirty = true;
    }

    /// Persist wizard_done so it doesn't show again.
    pub(super) fn mark_wizard_done(&self) {
        let state = self.state.read().clone();
        crate::state::save_session_ext(&state, self.show_cables, self.rack_flipped, Some(true));
    }
}
