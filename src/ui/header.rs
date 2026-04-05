// ─── ui/header.rs ─────────────────────────────────────────────────────────────
// Menu bar and header panel (BPM, transport, HEAT, JAM, vol, VRAM/RAM).

use crate::audio::AudioCommand;
use crate::export::{export_mp3, export_stems, export_wav};
use crate::llm::LlmInput;
use crate::state::save_project;
use crate::ui::{ImpulseApp, theme, webbrowser_open};
use egui::{Frame, TopBottomPanel};

impl ImpulseApp {
    /// Menu bar + header transport strip.
    pub(super) fn draw_menu_and_header(&mut self, ctx: &egui::Context) {
        self.draw_menu_bar(ctx);
        self.draw_header_bar(ctx);
    }

    fn draw_menu_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("menu_bar")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(4.0, 2.0)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button(egui::RichText::new("File").monospace().size(10.0), |ui| {
                        if ui
                            .button(egui::RichText::new("Save project").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            match save_project(&snapshot) {
                                Ok(path) => {
                                    let msg = format!("[ saved → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ save failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("Bars:")
                                    .monospace()
                                    .size(9.5)
                                    .color(theme::SMOKE),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.export_bars)
                                    .range(1..=64)
                                    .speed(1.0),
                            );
                        });

                        if ui
                            .button(egui::RichText::new("Export WAV").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            match export_wav(&snapshot, bars) {
                                Ok(path) => {
                                    let msg = format!("[ exported → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ export failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        if ui
                            .button(egui::RichText::new("Export MP3").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            match export_mp3(&snapshot, bars) {
                                Ok(path) => {
                                    let msg = format!("[ exported → {} ]", path.display());
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ export: {} ]", e);
                                    log::warn!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        if ui
                            .button(egui::RichText::new("Export Stems").monospace().size(10.0))
                            .clicked()
                        {
                            let snapshot = self.state.read().clone();
                            let bars = self.export_bars;
                            let ts = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            let prefix = format!("stems-{}", ts);
                            match export_stems(&snapshot, bars, &prefix) {
                                Ok(paths) => {
                                    let msg = format!(
                                        "[ stems → {} files ({}) ]",
                                        paths.len(),
                                        paths
                                            .iter()
                                            .filter_map(|p| p.file_name())
                                            .map(|n| n.to_string_lossy())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    );
                                    log::info!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                                Err(e) => {
                                    let msg = format!("[ stems failed: {} ]", e);
                                    log::error!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }

                        ui.separator();

                        if ui
                            .button(egui::RichText::new("Quit").monospace().size(10.0))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button(egui::RichText::new("Edit").monospace().size(10.0), |ui| {
                        let can_undo = self.history.can_undo();
                        let can_redo = self.history.can_redo();
                        if ui
                            .add_enabled(
                                can_undo,
                                egui::Button::new(
                                    egui::RichText::new("Undo  Ctrl+Z").monospace().size(10.0),
                                ),
                            )
                            .clicked()
                        {
                            let current = self.state.read().clone();
                            if let Some(prev) = self.history.undo(current) {
                                *self.state.write() = prev;
                                self.push_audio_params();
                            }
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(
                                can_redo,
                                egui::Button::new(
                                    egui::RichText::new("Redo  Ctrl+Y").monospace().size(10.0),
                                ),
                            )
                            .clicked()
                        {
                            let current = self.state.read().clone();
                            if let Some(next) = self.history.redo(current) {
                                *self.state.write() = next;
                                self.push_audio_params();
                            }
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(egui::RichText::new("Help").monospace().size(10.0), |ui| {
                        if ui
                            .button(egui::RichText::new("Preferences…").monospace().size(10.0))
                            .clicked()
                        {
                            self.show_prefs = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("System…").monospace().size(10.0))
                            .clicked()
                        {
                            self.show_sysinfo = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .button(egui::RichText::new("About").monospace().size(10.0))
                            .clicked()
                        {
                            self.show_about = true;
                            ui.close_menu();
                        }
                    });
                });
            });
    }

    fn draw_header_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("header")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(8.0, 5.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // ── LOGO + MODEL  (fixed 188px) ───────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(188.0);
                        ui.set_max_width(188.0);
                        let (cur_model, initializing) = {
                            let s = self.state.read();
                            (s.llm.model_path.clone(), s.llm.llm_initializing)
                        };
                        let cur_short = std::path::Path::new(&cur_model)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        if self.available_models.is_empty() {
                            self.available_models = super::scan_models();
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new("◆ IMPULSE INSTRUCT")
                                    .color(theme::CHALK)
                                    .size(12.0)
                                    .monospace()
                                    .strong(),
                            );
                            ui.add_enabled_ui(!initializing, |ui| {
                                let label_color = if initializing {
                                    theme::ASH
                                } else {
                                    theme::SMOKE
                                };
                                egui::ComboBox::from_id_source("model_dropdown")
                                    .selected_text(
                                        egui::RichText::new(&cur_short)
                                            .color(label_color)
                                            .size(8.5)
                                            .monospace(),
                                    )
                                    .width(176.0)
                                    .show_ui(ui, |ui: &mut egui::Ui| {
                                        for path in &self.available_models.clone() {
                                            let short = std::path::Path::new(path)
                                                .file_stem()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or(path)
                                                .to_string();
                                            let selected = *path == cur_model;
                                            let text = egui::RichText::new(&short)
                                                .monospace()
                                                .size(9.5)
                                                .color(if selected {
                                                    theme::CHALK
                                                } else {
                                                    theme::FOG
                                                });
                                            if ui.selectable_label(selected, text).clicked()
                                                && !selected
                                            {
                                                let _ = self
                                                    .llm_tx
                                                    .try_send(LlmInput::SwitchModel(path.clone()));
                                                self.state.write().llm.llm_initializing = true;
                                            }
                                        }
                                    });
                            });
                        });
                    });

                    ui.separator();

                    // ── MODEL STATUS  (fixed 240px) ───────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(240.0);
                        ui.set_max_width(240.0);

                        let s = self.state.read();
                        let inferring = s.llm.is_inferring;
                        let tps = s.llm.tokens_per_sec;
                        let ptok = s.llm.prompt_tokens;
                        let ctok = s.llm.completion_tokens;
                        let tthink = s.llm.thinking_tokens;
                        let ctx_used = s.llm.context_used;
                        let ctx_max = s.llm.context_max;
                        let ctx_pct = if ctx_max > 0 {
                            ctx_used as f32 / ctx_max as f32 * 100.0
                        } else {
                            0.0
                        };
                        let is_mock = s.llm.is_mock;
                        let model_missing = s.llm.model_missing;
                        let initializing = s.llm.llm_initializing;
                        let auto_compact = s.llm.auto_compact;
                        drop(s);

                        ui.horizontal(|ui| {
                            if initializing {
                                ui.label(
                                    egui::RichText::new("○  Loading model…")
                                        .color(theme::ASH)
                                        .size(9.0)
                                        .monospace(),
                                );
                            } else if model_missing {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("! NO MODEL")
                                            .color(egui::Color32::from_rgb(255, 100, 60))
                                            .size(10.0)
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "run download-models.sh then select in Prefs",
                                        )
                                        .color(egui::Color32::from_rgb(180, 70, 40))
                                        .size(8.0)
                                        .monospace(),
                                    );
                                });
                            } else if is_mock {
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("! MOCK MODE")
                                            .color(egui::Color32::from_rgb(255, 100, 60))
                                            .size(10.0)
                                            .monospace()
                                            .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "build-llama-server.sh + download-models.sh",
                                        )
                                        .color(egui::Color32::from_rgb(180, 70, 40))
                                        .size(8.0)
                                        .monospace(),
                                    );
                                });
                            } else {
                                let inf_color = if inferring { theme::CHALK } else { theme::ASH };
                                ui.label(egui::RichText::new("●").color(inf_color).size(10.0));
                                ui.vertical(|ui| {
                                    let ctx_color = if ctx_pct < 60.0 {
                                        theme::ASH
                                    } else if ctx_pct < 85.0 {
                                        theme::SMOKE
                                    } else {
                                        egui::Color32::from_rgb(220, 80, 50)
                                    };
                                    ui.horizontal(|ui| {
                                        let tps_str = format!("{:.0}t/s", tps);
                                        ui.label(
                                            egui::RichText::new(tps_str)
                                                .color(theme::SMOKE)
                                                .size(9.0)
                                                .monospace(),
                                        );
                                        ui.add_sized(
                                            [44.0, 6.0],
                                            egui::ProgressBar::new(ctx_pct / 100.0),
                                        );
                                        let ctx_label = format!("{}/{}", ctx_used, ctx_max);
                                        ui.label(
                                            egui::RichText::new(ctx_label)
                                                .color(ctx_color)
                                                .size(8.0)
                                                .monospace(),
                                        );
                                    });
                                    if ptok > 0 || ctok > 0 {
                                        let line2 = if tthink > 0 {
                                            format!("in:{} out:{} think:~{}", ptok, ctok, tthink)
                                        } else {
                                            format!("in:{}  out:{}", ptok, ctok)
                                        };
                                        ui.label(
                                            egui::RichText::new(line2)
                                                .color(theme::ASH)
                                                .size(8.5)
                                                .monospace(),
                                        );
                                    }
                                });

                                let reset_color = if ctx_pct >= 85.0 {
                                    egui::Color32::from_rgb(220, 80, 50)
                                } else {
                                    theme::ASH
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("CTX")
                                                .monospace()
                                                .size(8.5)
                                                .color(reset_color),
                                        )
                                        .fill(egui::Color32::TRANSPARENT),
                                    )
                                    .on_hover_text("Reset context window")
                                    .clicked()
                                {
                                    let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                                }

                                let ac_color = if auto_compact {
                                    theme::SMOKE
                                } else {
                                    theme::ASH
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("AUTO")
                                                .monospace()
                                                .size(8.5)
                                                .color(ac_color),
                                        )
                                        .fill(egui::Color32::TRANSPARENT),
                                    )
                                    .on_hover_text("Auto-compact when context > 85%")
                                    .clicked()
                                {
                                    self.state.write().llm.auto_compact = !auto_compact;
                                }
                            }
                        });
                    });

                    ui.separator();

                    // ── TRANSPORT  (fixed 200px) ──────────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(200.0);
                        ui.set_max_width(200.0);

                        let (running, bpm, live_record) = {
                            let s = self.state.read();
                            (s.sequencer.running, s.sequencer.bpm, s.live_record)
                        };

                        ui.horizontal(|ui| {
                            // BPM — fixed 56px so value changes don't shift neighbors
                            ui.scope(|ui| {
                                ui.set_min_width(56.0);
                                ui.set_max_width(56.0);
                                let run_color = if running { theme::CHALK } else { theme::ASH };
                                ui.label(
                                    egui::RichText::new(format!("{:.0} BPM", bpm))
                                        .color(run_color)
                                        .size(11.0)
                                        .monospace(),
                                );
                            });

                            ui.add_space(4.0);

                            // PLAY / STOP — fixed size so "▶ PLAY" ↔ "■ STOP" don't shift
                            let play_label = if running { "■ STOP" } else { "▶ PLAY" };
                            if ui
                                .add_sized(
                                    [60.0, 20.0],
                                    egui::Button::new(
                                        egui::RichText::new(play_label).monospace().size(10.0),
                                    ),
                                )
                                .clicked()
                            {
                                let next = crate::state::toggle_sequencer_running(
                                    self.state.read().clone(),
                                );
                                *self.state.write() = next;
                            }

                            ui.add_space(2.0);

                            // REC
                            let rec_col = if live_record && running {
                                theme::CHALK
                            } else {
                                theme::ASH
                            };
                            let rec_fill = if live_record && running {
                                egui::Color32::from_gray(60)
                            } else {
                                egui::Color32::TRANSPARENT
                            };
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("⏺ REC")
                                            .monospace()
                                            .size(10.0)
                                            .color(rec_col),
                                    )
                                    .fill(rec_fill),
                                )
                                .clicked()
                            {
                                let next =
                                    crate::state::toggle_live_record(self.state.read().clone());
                                *self.state.write() = next;
                            }
                        });
                    });

                    ui.separator();

                    // ── CONTROLS  (fixed 220px) ───────────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(260.0);
                        ui.set_max_width(260.0);

                        ui.horizontal(|ui| {
                            // Heat
                            let mut heat = self.state.read().llm.heat;
                            let heat_pct = (heat * 100.0).round() as u32;
                            let heat_color = if heat < 0.3 {
                                theme::ASH
                            } else if heat < 0.6 {
                                theme::SMOKE
                            } else if heat < 0.85 {
                                theme::FOG
                            } else {
                                theme::CHALK
                            };
                            ui.label(
                                egui::RichText::new("HEAT")
                                    .color(heat_color)
                                    .monospace()
                                    .size(9.0),
                            );
                            let slider = egui::Slider::new(&mut heat, 0.0..=1.0)
                                .show_value(false)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                            if ui.add_sized([110.0, 18.0], slider).changed() {
                                self.state.write().llm.heat = heat;
                            }
                            ui.label(
                                egui::RichText::new(format!("{heat_pct}%"))
                                    .color(heat_color)
                                    .monospace()
                                    .size(9.0),
                            );

                            ui.add_space(4.0);

                            ui.add_space(2.0);

                            // Knobs / Sliders toggle — fixed size
                            let use_sliders = self.state.read().ui_prefs.use_sliders;
                            let color = if use_sliders {
                                theme::CHALK
                            } else {
                                theme::ASH
                            };
                            if ui
                                .add_sized(
                                    [58.0, 18.0],
                                    egui::Button::new(
                                        egui::RichText::new(if use_sliders {
                                            "SLIDERS"
                                        } else {
                                            "KNOBS  "
                                        })
                                        .color(color)
                                        .monospace()
                                        .size(9.0),
                                    ),
                                )
                                .clicked()
                            {
                                self.state.write().ui_prefs.use_sliders = !use_sliders;
                            }
                        });
                    });

                    // ── RIGHT-ALIGNED: VOL + VRAM/RAM + API ───────────────────
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(port) = self.api_port {
                            let api_label = format!("API :{}", port);
                            let btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(&api_label)
                                        .color(theme::SMOKE)
                                        .monospace()
                                        .size(8.5),
                                )
                                .fill(theme::VOID)
                                .stroke(egui::Stroke::new(1.0, theme::SLATE)),
                            );
                            if btn.clicked() {
                                let url = format!("http://localhost:{}/api/schema", port);
                                let _ = webbrowser_open(&url);
                            }
                            ui.add_space(6.0);
                        }

                        // Monitor volume
                        let vol_color = if self.ui_volume < 0.4 {
                            theme::ASH
                        } else if self.ui_volume < 0.75 {
                            theme::SMOKE
                        } else {
                            theme::FOG
                        };
                        ui.label(
                            egui::RichText::new(format!("{:.0}%", self.ui_volume * 100.0))
                                .color(vol_color)
                                .monospace()
                                .size(9.0),
                        );
                        if ui
                            .add_sized(
                                [64.0, 16.0],
                                egui::Slider::new(&mut self.ui_volume, 0.0..=1.0).show_value(false),
                            )
                            .changed()
                        {
                            let _ = self
                                .audio_tx
                                .push(AudioCommand::SetMonitorVolume(self.ui_volume));
                        }
                        ui.label(
                            egui::RichText::new("VOL")
                                .color(vol_color)
                                .monospace()
                                .size(9.0),
                        );

                        ui.add_space(6.0);

                        // VRAM / RAM bars
                        if let Ok(si) = self.sys_info.lock() {
                            ui.vertical(|ui| {
                                if si.vram_total_mb > 0 {
                                    let frac = si.vram_used_mb as f32 / si.vram_total_mb as f32;
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("VRAM")
                                                .color(theme::ASH)
                                                .monospace()
                                                .size(8.0),
                                        );
                                        ui.add_sized([54.0, 6.0], egui::ProgressBar::new(frac));
                                        ui.label(
                                            egui::RichText::new(
                                                crate::sysinfo::fmt_mb(si.vram_used_mb).to_string(),
                                            )
                                            .color(theme::ASH)
                                            .monospace()
                                            .size(8.0),
                                        );
                                    });
                                }
                                if si.ram_total_mb > 0 {
                                    let frac = si.ram_used_mb as f32 / si.ram_total_mb as f32;
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("RAM ")
                                                .color(theme::ASH)
                                                .monospace()
                                                .size(8.0),
                                        );
                                        ui.add_sized([54.0, 6.0], egui::ProgressBar::new(frac));
                                        ui.label(
                                            egui::RichText::new(
                                                crate::sysinfo::fmt_mb(si.ram_used_mb).to_string(),
                                            )
                                            .color(theme::ASH)
                                            .monospace()
                                            .size(8.0),
                                        );
                                    });
                                }
                            });
                        }
                    });
                });
            });
    }
}
