// ─── ui/windows.rs ────────────────────────────────────────────────────────────
// Floating windows: Preferences, About, System Info.
use crate::state::{AutosaveInterval, ConversationMode, StyleVerbosity};
use crate::ui::LOG_LEVELS;
use crate::ui::{ImpulseApp, theme, widgets};

impl ImpulseApp {
    /// Draw all floating overlay windows (prefs, about, sysinfo, wizard).
    pub(super) fn draw_windows(&mut self, ctx: &egui::Context) {
        self.draw_prefs_window(ctx);
        self.draw_about_window(ctx);
        self.draw_sysinfo_window(ctx);
        self.draw_wizard_window(ctx);
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
                                    egui::RichText::new("Gemma defaults: top_k 64, top_p 0.95 · Bonsai: top_k 20, top_p 0.9, temp ×0.5")
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
                                ui.label(egui::RichText::new("0 = disabled; Gemma 64, Bonsai 20").monospace().size(7.5).color(theme::IRON));
                                sampling_row!("top_p", top_p, 0.0_f32, 1.0_f32, 0.01, "nucleus cutoff — Gemma 0.95, Bonsai 0.90");
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
                        use crate::state::HuthStyle;

                        widgets::section_header(ui, "HUTH FARBIGE NOTEN");
                        ui.label(
                            egui::RichText::new(
                                "Color system by Ch. A. B. Huth (Hamburg, 1888). Each semitone\n\
                                 maps to a unique color counter-clockwise around the RYB wheel.",
                            )
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                        );
                        ui.add_space(6.0);

                        let cur = self.state.read().ui_prefs.huth_style;
                        for (style, label, hint) in [
                            (HuthStyle::Off, "OFF", "Monochrome piano, grayscale sequencer"),
                            (HuthStyle::PianoOnly, "PIANO", "Huth colors on the piano keyboard only (default)"),
                            (HuthStyle::Full, "FULL", "Piano + sequencer melodic rows as Huth U-cups"),
                        ] {
                            ui.horizontal(|ui| {
                                let selected = cur == style;
                                let label_color = if selected { theme::CHALK } else { theme::SMOKE };
                                if ui.add(egui::Button::new(
                                    egui::RichText::new(label).monospace().size(9.5).color(label_color),
                                ).frame(selected)).clicked() && !selected {
                                    self.state.write().ui_prefs.huth_style = style;
                                }
                                ui.label(
                                    egui::RichText::new(hint).monospace().size(8.0).color(theme::IRON),
                                );
                            });
                        }

                        // Huth oscilloscope toggle
                        ui.horizontal(|ui| {
                            let mut huth_osc = self.state.read().ui_prefs.huth_oscilloscope;
                            if ui.add(egui::Button::new(
                                egui::RichText::new(if huth_osc { "OSC ●" } else { "OSC ○" })
                                    .monospace().size(9.5)
                                    .color(if huth_osc { theme::CHALK } else { theme::SMOKE }),
                            ).frame(huth_osc)).clicked() {
                                huth_osc = !huth_osc;
                                self.state.write().ui_prefs.huth_oscilloscope = huth_osc;
                            }
                            ui.label(
                                egui::RichText::new("Color oscilloscope waveform by detected frequency")
                                    .monospace().size(8.0).color(theme::IRON),
                            );
                        });

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
                            let bar = viz_toggle(ui, "Bar oscilloscope", prefs.show_bar_oscilloscope);
                            let ring = viz_toggle(ui, "Ring oscilloscope", prefs.show_ring_oscilloscope);
                            let stream = viz_toggle(ui, "Event stream", prefs.show_event_stream);
                            let stereo = viz_toggle(ui, "Stereo pan layer", prefs.stream_stereo);
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
                        egui::RichText::new("IMPULSE • INSTRUCT")
                            .monospace()
                            .size(16.0)
                            .color(theme::CHALK),
                    );
                    let version = env!("CARGO_PKG_VERSION");
                    ui.label(
                        egui::RichText::new(format!("v{version}"))
                            .monospace()
                            .size(10.0)
                            .color(theme::SMOKE),
                    );
                    ui.label(
                        egui::RichText::new("A synthesizer with AI agents inside it")
                            .monospace()
                            .size(9.0)
                            .color(theme::ASH),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(
                            "303 Bass · 808 Kit · 909 Kit · AN1X · Hoover · Noise · Granular",
                        )
                        .monospace()
                        .size(8.0)
                        .color(theme::ASH),
                    );
                    ui.label(
                        egui::RichText::new("LLM: llama.cpp · Gemma 4 · Bonsai")
                            .monospace()
                            .size(8.0)
                            .color(theme::ASH),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Rust · egui · cpal · rtrb")
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
                    );
                    ui.add_space(10.0);
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new("↗ github.com/Tok/impulse-instruct")
                                    .monospace()
                                    .size(9.0)
                                    .color(theme::FOG)
                                    .underline(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Open in browser")
                        .clicked()
                    {
                        let _ = super::webbrowser_open("https://github.com/Tok/impulse-instruct");
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Contributions welcome — issues, PRs, feedback")
                            .monospace()
                            .size(8.0)
                            .color(theme::IRON),
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

// Wizard extracted to wizard.rs to stay under 1000-line limit.
// draw_wizard_window is called from draw_windows above.
