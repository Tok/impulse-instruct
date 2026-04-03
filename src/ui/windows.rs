// ─── ui/windows.rs ────────────────────────────────────────────────────────────
// Floating windows: Preferences, About, System Info.

use crate::llm::LlmInput;
use crate::state::{ConversationMode, StyleVerbosity};
use crate::ui::{ImpulseApp, theme, widgets};
use crate::ui::{LOG_LEVELS, scan_models};

impl ImpulseApp {
    /// Draw all floating overlay windows (prefs, about, sysinfo).
    pub(super) fn draw_windows(&mut self, ctx: &egui::Context) {
        self.draw_prefs_window(ctx);
        self.draw_about_window(ctx);
        self.draw_sysinfo_window(ctx);
    }

    fn draw_prefs_window(&mut self, ctx: &egui::Context) {
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
                        widgets::section_header(ui, "PERSONA");
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Name")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            let mut name = self.state.read().llm.persona_name.clone();
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut name)
                                    .desired_width(120.0)
                                    .font(egui::TextStyle::Monospace),
                            );
                            if resp.changed() {
                                self.state.write().llm.persona_name = name;
                            }
                            ui.label(
                                egui::RichText::new("(injected into system prompt)")
                                    .monospace()
                                    .size(8.0)
                                    .color(theme::IRON),
                            );
                        });
                        ui.add_space(8.0);

                        widgets::section_header(ui, "MODEL");
                        if ui.small_button("scan models/").clicked() {
                            self.available_models = scan_models();
                        }
                        if self.available_models.is_empty() {
                            self.available_models = scan_models();
                        }
                        let cur_model = self.state.read().llm.model_path.clone();
                        for path in &self.available_models {
                            let short = std::path::Path::new(path)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy();
                            let selected = *path == cur_model;
                            let text = egui::RichText::new(short.as_ref())
                                .monospace()
                                .size(9.5)
                                .color(if selected { theme::CHALK } else { theme::FOG });
                            if ui.selectable_label(selected, text).clicked() && !selected {
                                let _ = self.llm_tx.try_send(LlmInput::SwitchModel(path.clone()));
                            }
                        }
                        ui.add_space(8.0);

                        widgets::section_header(ui, "PERSONALITY");
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
                            (
                                "Producer",
                                ConversationMode::Producer,
                                "what & why (default)",
                            ),
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
                                ui.label(
                                    egui::RichText::new(*hint)
                                        .monospace()
                                        .size(8.5)
                                        .color(theme::IRON),
                                );
                            });
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("TTS voice (espeak-ng)")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut tts = self.state.read().llm.tts_enabled;
                                    if widgets::toggle_button(
                                        ui,
                                        if tts { "ON" } else { "OFF" },
                                        &mut tts,
                                    ) {
                                        self.state.write().llm.tts_enabled = tts;
                                    }
                                },
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Style description")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut is_full = self.state.read().llm.style_verbosity
                                        == StyleVerbosity::Full;
                                    if widgets::toggle_button(
                                        ui,
                                        if is_full { "FULL" } else { "BRIEF" },
                                        &mut is_full,
                                    ) {
                                        self.state.write().llm.style_verbosity = if is_full {
                                            StyleVerbosity::Full
                                        } else {
                                            StyleVerbosity::Brief
                                        };
                                    }
                                },
                            );
                        });
                        ui.add_space(8.0);

                        widgets::section_header(ui, "SYSTEM PROMPT");
                        ui.label(
                            egui::RichText::new(
                                "Override: replaces the generated prompt entirely.",
                            )
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
                    }

                    // ── Tab 1: Controls ───────────────────────────────────────
                    1 => {
                        widgets::section_header(ui, "KNOB LAYOUT");
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Control style")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::toggle_button(
                                        ui,
                                        if self.use_sliders { "SLIDERS" } else { "KNOBS" },
                                        &mut self.use_sliders,
                                    ) {}
                                },
                            );
                        });
                        ui.add_space(8.0);

                        widgets::section_header(ui, "LOCK BEHAVIOUR");
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Auto-lock on touch")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let mut alt = self.state.read().llm.auto_lock_on_touch;
                                    if widgets::toggle_button(
                                        ui,
                                        if alt { "ON" } else { "OFF" },
                                        &mut alt,
                                    ) {
                                        self.state.write().llm.auto_lock_on_touch = alt;
                                    }
                                },
                            );
                        });
                        ui.label(
                            egui::RichText::new(
                                "  Off: knobs are free — click knob to toggle lock",
                            )
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                    }

                    // ── Tab 2: Display ────────────────────────────────────────
                    2 => {
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
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Farbige Noten colors")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::FOG),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    widgets::toggle_button(
                                        ui,
                                        if self.piano_show_colors { "ON" } else { "OFF" },
                                        &mut self.piano_show_colors,
                                    );
                                },
                            );
                        });
                    }

                    // ── Tab 3: System ─────────────────────────────────────────
                    _ => {
                        widgets::section_header(ui, "LOG VERBOSITY");
                        ui.label(
                            egui::RichText::new("Controls what appears in the terminal log.")
                                .monospace()
                                .size(8.0)
                                .color(theme::IRON),
                        );
                        ui.add_space(4.0);
                        for (i, (label, filter)) in LOG_LEVELS.iter().enumerate() {
                            let selected = self.log_level_idx == i;
                            let text = egui::RichText::new(*label)
                                .monospace()
                                .size(10.0)
                                .color(if selected { theme::CHALK } else { theme::FOG });
                            if ui.selectable_label(selected, text).clicked() && !selected {
                                self.log_level_idx = i;
                                log::set_max_level(*filter);
                            }
                        }
                        ui.label(
                            egui::RichText::new(
                                "  Current: applies immediately, resets on restart.",
                            )
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                    }
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

    fn draw_about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        egui::Window::new("About Impulse Instruct")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("◆ IMPULSE INSTRUCT")
                            .monospace()
                            .size(14.0)
                            .color(theme::CHALK),
                    );
                    ui.label(
                        egui::RichText::new("v0.1 — LLM-controlled synthesizer")
                            .monospace()
                            .size(9.5)
                            .color(theme::SMOKE),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Bass Synth  ·  Drum Kit A  ·  Drum Kit B")
                            .monospace()
                            .size(9.0)
                            .color(theme::ASH),
                    );
                    ui.label(
                        egui::RichText::new("LLM engine: llama.cpp")
                            .monospace()
                            .size(9.0)
                            .color(theme::ASH),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Type a prompt and press ASK.")
                            .monospace()
                            .size(9.0)
                            .color(theme::FOG),
                    );
                    ui.label(
                        egui::RichText::new("Toggle JAM for continuous mutation.")
                            .monospace()
                            .size(9.0)
                            .color(theme::FOG),
                    );
                    ui.label(
                        egui::RichText::new("HEAT controls how wild it gets.")
                            .monospace()
                            .size(9.0)
                            .color(theme::FOG),
                    );
                    ui.add_space(10.0);
                    if ui.button("Close").clicked() {
                        self.show_about = false;
                    }
                });
            });
    }

    fn draw_sysinfo_window(&mut self, ctx: &egui::Context) {
        if !self.show_sysinfo {
            return;
        }
        let si = self
            .sys_info
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default();
        egui::Window::new("System Info")
            .collapsible(false)
            .resizable(false)
            .min_width(320.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let row = |ui: &mut egui::Ui, label: &str, val: &str| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(label)
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.label(
                            egui::RichText::new(val)
                                .color(theme::FOG)
                                .monospace()
                                .size(9.5),
                        );
                    });
                };

                ui.label(
                    egui::RichText::new("GPU")
                        .color(theme::CHALK)
                        .monospace()
                        .size(10.5)
                        .strong(),
                );
                ui.separator();
                if si.gpu_name.is_empty() {
                    row(ui, "GPU:    ", "nvidia-smi not found or no NVIDIA GPU");
                } else {
                    row(ui, "Name:   ", &si.gpu_name);
                    row(ui, "Driver: ", &si.driver_version);
                    if !si.cuda_version.is_empty() {
                        row(ui, "CUDA:   ", &si.cuda_version);
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let frac = if si.vram_total_mb > 0 {
                            si.vram_used_mb as f32 / si.vram_total_mb as f32
                        } else {
                            0.0
                        };
                        ui.label(
                            egui::RichText::new("VRAM:   ")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.add_sized([120.0, 10.0], egui::ProgressBar::new(frac));
                        ui.label(
                            egui::RichText::new(format!(
                                "  {} / {}  ({:.0}%)",
                                crate::sysinfo::fmt_mb(si.vram_used_mb),
                                crate::sysinfo::fmt_mb(si.vram_total_mb),
                                frac * 100.0
                            ))
                            .color(theme::FOG)
                            .monospace()
                            .size(9.5),
                        );
                    });
                }

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("System Memory")
                        .color(theme::CHALK)
                        .monospace()
                        .size(10.5)
                        .strong(),
                );
                ui.separator();
                if si.ram_total_mb > 0 {
                    ui.horizontal(|ui| {
                        let frac = si.ram_used_mb as f32 / si.ram_total_mb as f32;
                        ui.label(
                            egui::RichText::new("RAM:    ")
                                .color(theme::SMOKE)
                                .monospace()
                                .size(9.5),
                        );
                        ui.add_sized([120.0, 10.0], egui::ProgressBar::new(frac));
                        ui.label(
                            egui::RichText::new(format!(
                                "  {} / {}  ({:.0}%)",
                                crate::sysinfo::fmt_mb(si.ram_used_mb),
                                crate::sysinfo::fmt_mb(si.ram_total_mb),
                                frac * 100.0
                            ))
                            .color(theme::FOG)
                            .monospace()
                            .size(9.5),
                        );
                    });
                } else {
                    row(ui, "RAM:    ", "/proc/meminfo not available");
                }

                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Updated every 3 seconds")
                        .color(theme::IRON)
                        .monospace()
                        .size(8.5),
                );
                ui.add_space(4.0);
                if ui.button("Close").clicked() {
                    self.show_sysinfo = false;
                }
            });
    }
}
