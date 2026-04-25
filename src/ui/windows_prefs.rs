// ─── ui/windows_prefs.rs ──────────────────────────────────────────────────────
// Preferences window (AI / Controls / Display / System tabs) plus helpers
// `draw_style_overrides_editor` and `draw_system_tab` used only from the
// prefs tab strip.  Split out of `windows.rs` to stay under the 1000-line cap.
use crate::state::{AutosaveInterval, ConversationMode, StyleVerbosity};
use crate::ui::LOG_LEVELS;
use crate::ui::{ImpulseApp, theme, widgets};

impl ImpulseApp {
    pub(super) fn draw_prefs_window(&mut self, ctx: &egui::Context) {
        if !self.show_prefs {
            return;
        }
        egui::Window::new("Preferences")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(340.0);

                // ── Tab strip ─────────────────────────────────────────────────
                let tabs = ["AI", "Controls", "Display", "System"];
                ui.horizontal(|ui| {
                    for (i, label) in tabs.iter().enumerate() {
                        let active = self.prefs_tab == i;
                        let color = if active { theme::CHALK } else { theme::ASH };
                        let text = egui::RichText::new(*label)
                            .monospace()
                            .size(10.0)
                            .color(color);
                        if ui.selectable_label(active, text).clicked() {
                            self.prefs_tab = i;
                        }
                    }
                });
                ui.separator();
                ui.add_space(4.0);

                match self.prefs_tab {
                    // ── Tab 0: AI ─────────────────────────────────────────────
                    0 => {
                        // ── AI sub-tab strip ──────────────────────────────────
                        let ai_tabs = ["Model", "Sampling", "Personality", "TTS"];
                        ui.horizontal(|ui| {
                            for (i, label) in ai_tabs.iter().enumerate() {
                                let active = self.llm_tab == i;
                                let color = if active { theme::CHALK } else { theme::SLATE };
                                let text = egui::RichText::new(*label)
                                    .monospace()
                                    .size(9.5)
                                    .color(color);
                                if ui.selectable_label(active, text).clicked() {
                                    self.llm_tab = i;
                                }
                            }
                        });
                        ui.add_space(6.0);

                        match self.llm_tab {
                            // ── Model ─────────────────────────────────────────
                            0 => {
                                widgets::section_header(ui, "MODEL");
                                ui.label(
                                    egui::RichText::new("Selected in the header dropdown. Restart required for path changes.")
                                        .monospace()
                                        .size(8.0)
                                        .color(theme::IRON),
                                );
                                ui.add_space(6.0);

                                // ctx_size
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("ctx_size").monospace().size(9.0).color(theme::FOG));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut v = self.state.read().llm.context_max;
                                        if ui.add(egui::DragValue::new(&mut v).range(4096..=131072).speed(256)).changed() {
                                            v = (v / 256) * 256;
                                            if v < 4096 { v = 4096; }
                                            self.state.write().llm.context_max = v;
                                        }
                                    });
                                });
                                ui.label(egui::RichText::new("tokens; takes effect on next model restart — more VRAM for larger values").monospace().size(7.5).color(theme::IRON));

                                // seed
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("seed").monospace().size(9.0).color(theme::FOG));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut v = self.state.read().llm.seed;
                                        if ui.add(egui::DragValue::new(&mut v).range(-1..=i64::MAX).speed(1)).changed() {
                                            self.state.write().llm.seed = v;
                                        }
                                    });
                                });
                                ui.label(egui::RichText::new("-1 = random each call; fixed seed gives reproducible outputs").monospace().size(7.5).color(theme::IRON));
                                ui.add_space(8.0);

                                widgets::section_header(ui, "INFERENCE");
                                let is_mock = self.state.read().llm.is_mock;
                                ui.add_enabled_ui(!is_mock, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Reasoning mode (/think)")
                                                .monospace()
                                                .size(9.5)
                                                .color(if is_mock { theme::IRON } else { theme::FOG }),
                                        );
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let mut thinking = self.state.read().llm.enable_thinking;
                                            if widgets::toggle_button(ui, if thinking { "ON" } else { "OFF" }, &mut thinking) {
                                                self.state.write().llm.enable_thinking = thinking;
                                            }
                                        });
                                    });
                                    ui.label(
                                        egui::RichText::new("Qwen3: slower, deeper reasoning — off for quick commands")
                                            .monospace()
                                            .size(8.0)
                                            .color(theme::IRON),
                                    );
                                });
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Show reasoning in log").monospace().size(9.5).color(theme::FOG));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut show = self.state.read().llm.show_thinking_in_log;
                                        if widgets::toggle_button(ui, if show { "ON" } else { "OFF" }, &mut show) {
                                            self.state.write().llm.show_thinking_in_log = show;
                                        }
                                    });
                                });
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Lane pipeline (planner + per-voice)").monospace().size(9.5).color(theme::FOG));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut on = self.state.read().llm.use_pipeline;
                                        if widgets::toggle_button(ui, if on { "ON" } else { "OFF" }, &mut on) {
                                            self.state.write().llm.use_pipeline = on;
                                        }
                                    });
                                });
                                ui.label(
                                    egui::RichText::new("Splits each turn into focused per-voice calls — shorter, more reliable than one-shot.")
                                        .monospace()
                                        .size(8.0)
                                        .color(theme::IRON),
                                );
                                if is_mock {
                                    ui.label(
                                        egui::RichText::new("  (unavailable in mock mode)")
                                            .monospace()
                                            .size(8.0)
                                            .color(theme::IRON),
                                    );
                                }
                            }

                            // ── Sampling ──────────────────────────────────────
                            1 => {
                                widgets::section_header(ui, "SAMPLING  [experimental]");
                                ui.label(
                                    egui::RichText::new("Gemma defaults: top_k 64, top_p 0.95, min_p 0.05")
                                        .monospace()
                                        .size(8.0)
                                        .color(theme::IRON),
                                );
                                ui.add_space(4.0);

                                macro_rules! sampling_row {
                                    ($label:expr, $field:ident, $min:expr, $max:expr, $speed:expr, $hint:expr) => {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new($label).monospace().size(9.0).color(theme::FOG));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let mut v = self.state.read().llm.$field;
                                                if ui.add(egui::DragValue::new(&mut v).range($min..=$max).speed($speed).max_decimals(3)).changed() {
                                                    self.state.write().llm.$field = v;
                                                }
                                            });
                                        });
                                        ui.label(egui::RichText::new($hint).monospace().size(7.5).color(theme::IRON));
                                    };
                                }

                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("top_k").monospace().size(9.0).color(theme::FOG));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut v = self.state.read().llm.top_k;
                                        if ui.add(egui::DragValue::new(&mut v).range(0..=200).speed(1)).changed() {
                                            self.state.write().llm.top_k = v;
                                        }
                                    });
                                });
                                ui.label(egui::RichText::new("0 = disabled; Gemma default 64").monospace().size(7.5).color(theme::IRON));
                                sampling_row!("top_p", top_p, 0.0_f32, 1.0_f32, 0.01, "nucleus cutoff — Gemma default 0.95");
                                sampling_row!("min_p", min_p, 0.0_f32, 0.5_f32, 0.005, "min-prob floor — 0.05 default, 0 to disable");
                                sampling_row!("repeat_penalty", repeat_penalty, 1.0_f32, 2.0_f32, 0.01, "1.0 = off; >1.0 penalises repeated tokens");
                                sampling_row!("freq_penalty", frequency_penalty, 0.0_f32, 2.0_f32, 0.01, "0.0 = off; reduces repetitive phrasing");
                            }

                            // ── Personality ───────────────────────────────────
                            2 => {
                                widgets::section_header(ui, "PERSONA");
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Name").monospace().size(9.5).color(theme::FOG));
                                    let mut name = self.state.read().llm.persona_name.clone();
                                    let resp = ui.add(
                                        egui::TextEdit::singleline(&mut name)
                                            .desired_width(120.0)
                                            .font(egui::TextStyle::Monospace),
                                    );
                                    if resp.changed() {
                                        self.state.write().llm.persona_name = name;
                                    }
                                    ui.label(egui::RichText::new("(injected into system prompt)").monospace().size(8.0).color(theme::IRON));
                                });
                                ui.add_space(8.0);

                                widgets::section_header(ui, "VOICE MODE");
                                ui.label(
                                    egui::RichText::new("How the AI narrates its moves")
                                        .monospace()
                                        .size(8.5)
                                        .color(theme::IRON),
                                );
                                ui.add_space(4.0);
                                let cur_mode = self.state.read().llm.conversation_mode.clone();
                                for (label, mode, hint) in &[
                                    ("Off", ConversationMode::Off, "no commentary"),
                                    ("Producer", ConversationMode::Producer, "what & why (default)"),
                                    ("DJ", ConversationMode::Dj, "hype party energy"),
                                    ("MC", ConversationMode::Mc, "jungle/rave MC"),
                                ] {
                                    ui.horizontal(|ui| {
                                        let selected = cur_mode == *mode;
                                        let text = egui::RichText::new(*label)
                                            .monospace()
                                            .size(10.0)
                                            .color(if selected { theme::CHALK } else { theme::FOG });
                                        if ui.selectable_label(selected, text).clicked() && !selected {
                                            self.state.write().llm.conversation_mode = mode.clone();
                                        }
                                        ui.label(egui::RichText::new(*hint).monospace().size(8.5).color(theme::IRON));
                                    });
                                }
                                ui.add_space(6.0);

                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("Style description").monospace().size(9.5).color(theme::FOG));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let mut is_full = self.state.read().llm.style_verbosity == StyleVerbosity::Full;
                                        if widgets::toggle_button(ui, if is_full { "FULL" } else { "BRIEF" }, &mut is_full) {
                                            self.state.write().llm.style_verbosity = if is_full {
                                                StyleVerbosity::Full
                                            } else {
                                                StyleVerbosity::Brief
                                            };
                                        }
                                    });
                                });
                                ui.add_space(8.0);

                                widgets::section_header(ui, "SYSTEM PROMPT");
                                ui.label(
                                    egui::RichText::new("Override: replaces the generated prompt entirely.")
                                        .monospace()
                                        .size(8.0)
                                        .color(theme::IRON),
                                );
                                ui.add_space(2.0);
                                let mut sp_override = self.state.read().llm.system_prompt_override.clone();
                                let sp_resp = ui.add(
                                    egui::TextEdit::multiline(&mut sp_override)
                                        .desired_rows(4)
                                        .desired_width(f32::INFINITY)
                                        .font(egui::TextStyle::Monospace)
                                        .hint_text("Leave empty for auto-generated system prompt…"),
                                );
                                if sp_resp.changed() {
                                    self.state.write().llm.system_prompt_override = sp_override;
                                }
                                ui.add_space(8.0);

                                // ── Style overrides (mc_lines / themes) ──────
                                self.draw_style_overrides_editor(ui);
                            }

                            // ── TTS ───────────────────────────────────────────
                            _ => {
                                widgets::section_header(ui, "TEXT-TO-SPEECH");
                                ui.label(
                                    egui::RichText::new("TTS voice settings are on the TTS rack module panel.")
                                        .monospace().size(9.0).color(theme::IRON),
                                );
                            }
                        }
                    }

                    // ── Tab 1: Controls ───────────────────────────────────────
                    1 => { self.draw_controls_tab(ui); }

                    // ── Tab 2: Display ────────────────────────────────────────
                    2 => {
                        widgets::section_header(ui, "HUTH FARBIGE NOTEN");
                        ui.label(
                            egui::RichText::new(
                                "Color system by Ch. A. B. Huth (Hamburg, 1888). Each semitone\n\
                                 maps to a unique color counter-clockwise around the RYB wheel.\n\n\
                                 Sequencer step dots and the event-stream history are always\n\
                                 Huth-coloured. Toggle the other surfaces individually:",
                            )
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                        ui.add_space(6.0);

                        // Four independent per-component toggles replace the
                        // old on/off/full triset.  `viz_toggle` is defined
                        // below in the same tab — we reuse the same widget
                        // shape for visual consistency.
                        let huth_toggle = |ui: &mut egui::Ui, label: &str, val: bool| -> bool {
                            let mut out = val;
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(label)
                                        .monospace()
                                        .size(9.5)
                                        .color(theme::FOG),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        widgets::toggle_button(
                                            ui,
                                            if out { "ON" } else { "OFF" },
                                            &mut out,
                                        );
                                    },
                                );
                            });
                            out
                        };
                        let prefs = self.state.read().ui_prefs.clone();
                        let piano = huth_toggle(ui, "Piano keys + labels", prefs.huth_piano);
                        let bar = huth_toggle(ui, "Bar oscilloscope", prefs.huth_bar_osc);
                        let ring = huth_toggle(ui, "Ring oscilloscope", prefs.huth_ring_osc);
                        let spec = huth_toggle(ui, "Spectrum bars", prefs.huth_spectrum);
                        if piano != prefs.huth_piano {
                            self.state.write().ui_prefs.huth_piano = piano;
                        }
                        if bar != prefs.huth_bar_osc {
                            self.state.write().ui_prefs.huth_bar_osc = bar;
                        }
                        if ring != prefs.huth_ring_osc {
                            self.state.write().ui_prefs.huth_ring_osc = ring;
                        }
                        if spec != prefs.huth_spectrum {
                            self.state.write().ui_prefs.huth_spectrum = spec;
                        }

                        ui.add_space(8.0);
                        widgets::section_header(ui, "PIANO DISPLAY");
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Note labels")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    widgets::toggle_button(
                                        ui,
                                        if self.piano_show_labels { "ON" } else { "OFF" },
                                        &mut self.piano_show_labels,
                                    );
                                },
                            );
                        });
                        ui.label(
                            egui::RichText::new("Active keys always bloom in Huth color.")
                                .monospace()
                                .size(8.0)
                                .color(theme::IRON),
                        );

                        ui.add_space(8.0);
                        widgets::section_header(ui, "HEADER VISUALIZATIONS");
                        ui.label(
                            egui::RichText::new(
                                "Toggle header info panels. Remaining panels reclaim the space.",
                            )
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                        ui.add_space(4.0);
                        {
                            let viz_toggle = |ui: &mut egui::Ui, label: &str, val: bool| -> bool {
                                let mut out = val;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(label)
                                            .monospace()
                                            .size(9.5)
                                            .color(theme::FOG),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            widgets::toggle_button(
                                                ui,
                                                if out { "ON" } else { "OFF" },
                                                &mut out,
                                            );
                                        },
                                    );
                                });
                                out
                            };
                            let prefs = self.state.read().ui_prefs.clone();
                            let spectrum =
                                viz_toggle(ui, "Spectrum bars", prefs.show_spectrum_bars);
                            let bar =
                                viz_toggle(ui, "Bar oscilloscope", prefs.show_bar_oscilloscope);
                            let ring =
                                viz_toggle(ui, "Ring oscilloscope", prefs.show_ring_oscilloscope);
                            let stream = viz_toggle(ui, "Event stream", prefs.show_event_stream);
                            let stereo = viz_toggle(ui, "Stereo pan layer", prefs.stream_stereo);
                            let automation = viz_toggle(
                                ui,
                                "Automation overlay (LFO sparkline)",
                                prefs.show_automation_overlay,
                            );
                            let minimap =
                                viz_toggle(ui, "Rack mini-map", prefs.show_rack_minimap);
                            if spectrum != prefs.show_spectrum_bars {
                                self.state.write().ui_prefs.show_spectrum_bars = spectrum;
                            }
                            if bar != prefs.show_bar_oscilloscope {
                                self.state.write().ui_prefs.show_bar_oscilloscope = bar;
                            }
                            if ring != prefs.show_ring_oscilloscope {
                                self.state.write().ui_prefs.show_ring_oscilloscope = ring;
                            }
                            if stream != prefs.show_event_stream {
                                self.state.write().ui_prefs.show_event_stream = stream;
                            }
                            if stereo != prefs.stream_stereo {
                                self.state.write().ui_prefs.stream_stereo = stereo;
                            }
                            if automation != prefs.show_automation_overlay {
                                self.state.write().ui_prefs.show_automation_overlay = automation;
                            }
                            if minimap != prefs.show_rack_minimap {
                                self.state.write().ui_prefs.show_rack_minimap = minimap;
                            }
                        }
                    }

                    // ── Tab 3: System ─────────────────────────────────────────
                    _ => self.draw_system_tab(ui),
                }

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    if ui.button("Close").clicked() {
                        self.show_prefs = false;
                    }
                });
            });
    }

    // draw_controls_tab is in prefs_controls.rs

    /// Compact editor for per-style mc_lines / themes overrides.
    /// Lives inside the AI / Personality tab.  User picks a style from
    /// a dropdown, sees its effective (override or baseline) lines,
    /// and can stamp a new override.  Clearing the text fields reverts
    /// to baseline on save.
    fn draw_style_overrides_editor(&mut self, ui: &mut egui::Ui) {
        use crate::llm::styles::StyleCatalog;
        use crate::state::StyleOverride;

        widgets::section_header(ui, "STYLE OVERRIDES");
        ui.label(
            egui::RichText::new(
                "Per-style MC lines + themes.  Overrides styles.json for one genre.",
            )
            .monospace()
            .size(8.0)
            .color(theme::IRON),
        );
        ui.add_space(4.0);

        // Per-session UI state — which style's overrides are currently
        // being edited.  Stored in egui memory so it survives frames
        // without needing an AppState field.
        let pick_id = egui::Id::new("prefs_style_overrides_pick");
        let mut pick: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(pick_id))
            .unwrap_or_else(|| {
                self.state
                    .read()
                    .llm
                    .active_style
                    .clone()
                    .unwrap_or_else(|| {
                        StyleCatalog::get()
                            .styles()
                            .first()
                            .map(|s| s.id.clone())
                            .unwrap_or_default()
                    })
            });

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Style")
                    .monospace()
                    .size(9.5)
                    .color(theme::FOG),
            );
            let current_label = StyleCatalog::get()
                .find_by_id(&pick)
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "— none —".into());
            egui::ComboBox::from_id_source("style_overrides_dropdown")
                .selected_text(
                    egui::RichText::new(current_label)
                        .monospace()
                        .size(9.5)
                        .color(theme::FOG),
                )
                .width(160.0)
                .show_ui(ui, |ui| {
                    for s in StyleCatalog::get().styles() {
                        let is_sel = pick == s.id;
                        if ui
                            .selectable_label(
                                is_sel,
                                egui::RichText::new(&s.name).monospace().size(9.5),
                            )
                            .clicked()
                        {
                            pick = s.id.clone();
                        }
                    }
                });
            let has_override = self.state.read().style_overrides.contains_key(&pick);
            if has_override {
                ui.label(
                    egui::RichText::new("◆ override active")
                        .monospace()
                        .size(8.5)
                        .color(theme::CHALK),
                );
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(pick_id, pick.clone()));

        // Render the two list editors alongside the baseline for context.
        let snap = self.state.read();
        let mc_current = crate::state::effective_mc_lines(&snap, &pick).join("\n");
        let themes_current = crate::state::effective_themes(&snap, &pick).join(", ");
        drop(snap);

        // MC lines editor — one per line to match how they're used.
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("MC lines (one per line)")
                .monospace()
                .size(9.0)
                .color(theme::FOG),
        );
        let mc_edit_id = egui::Id::new(("prefs_style_mc", pick.clone()));
        let mut mc_buf: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(mc_edit_id))
            .unwrap_or_else(|| mc_current.clone());
        let mc_resp = ui.add(
            egui::TextEdit::multiline(&mut mc_buf)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if mc_resp.changed() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(mc_edit_id, mc_buf.clone()));
        }

        // Themes — comma-separated list for compactness.
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Themes (comma-separated)")
                .monospace()
                .size(9.0)
                .color(theme::FOG),
        );
        let themes_edit_id = egui::Id::new(("prefs_style_themes", pick.clone()));
        let mut themes_buf: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(themes_edit_id))
            .unwrap_or_else(|| themes_current.clone());
        let themes_resp = ui.add(
            egui::TextEdit::singleline(&mut themes_buf)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if themes_resp.changed() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(themes_edit_id, themes_buf.clone()));
        }

        // Save / Revert row.  Save stamps the override — an empty list
        // means "explicitly clear this style's lines" rather than "fall
        // back to baseline".  Revert drops the override entirely.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .button(
                    egui::RichText::new("Save override")
                        .monospace()
                        .size(9.5)
                        .color(theme::CHALK),
                )
                .clicked()
            {
                let mc_lines: Vec<String> = mc_buf
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                let themes: Vec<String> = themes_buf
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let ov = StyleOverride {
                    mc_lines: Some(mc_lines),
                    themes: Some(themes),
                };
                self.state.write().style_overrides.insert(pick.clone(), ov);
            }
            if ui
                .button(
                    egui::RichText::new("Revert to baseline")
                        .monospace()
                        .size(9.5)
                        .color(theme::FOG),
                )
                .clicked()
            {
                self.state.write().style_overrides.remove(&pick);
                // Also drop the in-memory edit buffers so the next draw
                // re-reads from the (now baseline) catalog.
                ui.ctx().data_mut(|d| {
                    d.remove::<String>(mc_edit_id);
                    d.remove::<String>(themes_edit_id);
                });
            }
        });
    }

    fn draw_system_tab(&mut self, ui: &mut egui::Ui) {
        let hint = |ui: &mut egui::Ui, text: &str| {
            ui.label(
                egui::RichText::new(text)
                    .monospace()
                    .size(8.0)
                    .color(theme::IRON),
            );
        };
        widgets::section_header(ui, "LOG VERBOSITY");
        hint(ui, "Controls what appears in the terminal log.");
        ui.add_space(4.0);
        let cur_idx = self.state.read().ui_prefs.log_level_idx;
        for (i, (label, filter)) in LOG_LEVELS.iter().enumerate() {
            let selected = cur_idx == i;
            let text = egui::RichText::new(*label)
                .monospace()
                .size(10.0)
                .color(if selected { theme::CHALK } else { theme::FOG });
            if ui.selectable_label(selected, text).clicked() && !selected {
                self.state.write().ui_prefs.log_level_idx = i;
                log::set_max_level(*filter);
                log::info!("Log level changed to {}", label);
            }
        }
        hint(
            ui,
            "  Current: applies immediately, persisted across sessions.",
        );
        ui.add_space(8.0);
        widgets::section_header(ui, "AUTOSAVE");
        hint(ui, "How often session state is written to session.json.");
        ui.add_space(4.0);
        let cur_interval = self.state.read().ui_prefs.autosave_interval;
        for interval in [
            AutosaveInterval::Immediate,
            AutosaveInterval::FiveSec,
            AutosaveInterval::ThirtySec,
            AutosaveInterval::Manual,
        ] {
            let selected = cur_interval == interval;
            let text = egui::RichText::new(interval.label())
                .monospace()
                .size(10.0)
                .color(if selected { theme::CHALK } else { theme::FOG });
            if ui.selectable_label(selected, text).clicked() && !selected {
                self.state.write().ui_prefs.autosave_interval = interval;
                self.session_dirty = true;
            }
        }
        hint(ui, "  Manual: use File → Save Project to persist state.");
    }
}
