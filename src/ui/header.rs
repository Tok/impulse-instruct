// ─── ui/header.rs ─────────────────────────────────────────────────────────────
// Menu bar and header panel (BPM, transport, HEAT, JAM, vol, VRAM/RAM).

use crate::audio::AudioCommand;
use crate::export::{export_mp3, export_wav};
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

                        ui.separator();

                        if ui
                            .button(egui::RichText::new("Quit").monospace().size(10.0))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
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
            .frame(Frame::none().fill(theme::VOID).inner_margin(egui::Margin::symmetric(10.0, 6.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo + model dropdown
                    {
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
                                    .size(13.0)
                                    .monospace()
                                    .strong(),
                            );
                            ui.add_enabled_ui(!initializing, |ui| {
                                let label_color = if initializing { theme::IRON } else { theme::SMOKE };
                                egui::ComboBox::from_id_source("model_dropdown")
                                    .selected_text(
                                        egui::RichText::new(&cur_short)
                                            .color(label_color)
                                            .size(8.5)
                                            .monospace(),
                                    )
                                    .width(160.0)
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
                                                .color(if selected { theme::CHALK } else { theme::FOG });
                                            if ui.selectable_label(selected, text).clicked() && !selected {
                                                let _ = self.llm_tx.try_send(LlmInput::SwitchModel(path.clone()));
                                                self.state.write().llm.llm_initializing = true;
                                            }
                                        }
                                    });
                            });
                        });
                    }

                    ui.add_space(16.0);

                    // Model stats — or status warning when not live
                    {
                        let s = self.state.read();
                        let inferring    = s.llm.is_inferring;
                        let tps          = s.llm.tokens_per_sec;
                        let ptok         = s.llm.prompt_tokens;
                        let ctok         = s.llm.completion_tokens;
                        let tthink       = s.llm.thinking_tokens;
                        let ctx_used     = s.llm.context_used;
                        let ctx_max      = s.llm.context_max;
                        let ctx_pct      = if ctx_max > 0 {
                            ctx_used as f32 / ctx_max as f32 * 100.0
                        } else { 0.0 };
                        let is_mock      = s.llm.is_mock;
                        let initializing = s.llm.llm_initializing;
                        let auto_compact = s.llm.auto_compact;
                        let bpm          = s.sequencer.bpm;
                        let running      = s.sequencer.running;

                        if initializing {
                            ui.label(egui::RichText::new("○").color(theme::ASH).size(10.0));
                            ui.label(egui::RichText::new("Loading model…").color(theme::ASH).size(9.0).monospace());
                        } else if is_mock {
                            ui.label(egui::RichText::new("!").color(egui::Color32::from_rgb(255, 100, 60)).size(12.0).monospace().strong());
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("MOCK MODE").color(egui::Color32::from_rgb(255, 100, 60)).size(10.0).monospace().strong());
                                ui.label(egui::RichText::new("no model  —  ./build-bonsai-server.sh + ./download-models.sh").color(egui::Color32::from_rgb(200, 80, 40)).size(8.0).monospace());
                            });
                        } else {
                            let inf_color = if inferring { theme::CHALK } else { theme::IRON };
                            ui.label(egui::RichText::new("●").color(inf_color).size(10.0));
                            ui.vertical(|ui| {
                                // Context bar: color shifts red as it fills up
                                let ctx_color = if ctx_pct < 60.0 {
                                    theme::IRON
                                } else if ctx_pct < 85.0 {
                                    theme::SMOKE
                                } else {
                                    egui::Color32::from_rgb(220, 80, 50)
                                };
                                ui.horizontal(|ui| {
                                    let tps_str = format!("{:.0}t/s", tps);
                                    ui.label(egui::RichText::new(tps_str).color(theme::SMOKE).size(9.0).monospace());
                                    ui.add_sized(
                                        [44.0, 6.0],
                                        egui::ProgressBar::new(ctx_pct / 100.0),
                                    );
                                    let ctx_label = format!("{}/{}", ctx_used, ctx_max);
                                    ui.label(egui::RichText::new(ctx_label).color(ctx_color).size(8.0).monospace());
                                });
                                if ptok > 0 || ctok > 0 {
                                    let line2 = if tthink > 0 {
                                        format!("in:{} out:{} think:~{}", ptok, ctok, tthink)
                                    } else {
                                        format!("in:{}  out:{}", ptok, ctok)
                                    };
                                    ui.label(egui::RichText::new(line2).color(theme::IRON).size(8.5).monospace());
                                }
                            });

                            // CTX RESET button
                            let reset_color = if ctx_pct >= 85.0 { egui::Color32::from_rgb(220, 80, 50) } else { theme::IRON };
                            if ui.add(egui::Button::new(
                                egui::RichText::new("CTX").monospace().size(8.5).color(reset_color)
                            ).fill(egui::Color32::TRANSPARENT)).on_hover_text("Reset context window (restart server)").clicked() {
                                let _ = self.llm_tx.try_send(LlmInput::ResetContext);
                            }

                            // Auto-compact toggle
                            let ac_color = if auto_compact { theme::SMOKE } else { theme::IRON };
                            if ui.add(egui::Button::new(
                                egui::RichText::new("AUTO").monospace().size(8.5).color(ac_color)
                            ).fill(egui::Color32::TRANSPARENT)).on_hover_text("Auto-compact: restart server when context > 85% full").clicked() {
                                self.state.write().llm.auto_compact = !auto_compact;
                            }
                        }

                        ui.add_space(8.0);

                        // BPM display
                        let run_color = if running { theme::CHALK } else { theme::ASH };
                        ui.label(egui::RichText::new(format!("{:.0} BPM", bpm))
                            .color(run_color).size(11.0).monospace());
                    }

                    ui.add_space(8.0);

                    // Play / Stop
                    {
                        let running = self.state.read().sequencer.running;
                        let play_label = if running { "■ STOP" } else { "▶ PLAY" };
                        if ui.button(egui::RichText::new(play_label).monospace().size(10.0)).clicked() {
                            let next = crate::state::toggle_sequencer_running(self.state.read().clone());
                            *self.state.write() = next;
                        }
                    }

                    // REC — live record piano notes into bass pattern
                    {
                        let (live_record, running) = {
                            let s = self.state.read();
                            (s.live_record, s.sequencer.running)
                        };
                        let rec_col = if live_record && running {
                            theme::CHALK
                        } else {
                            theme::IRON
                        };
                        let rec_fill = if live_record && running {
                            egui::Color32::from_gray(60)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        if ui.add(egui::Button::new(
                            egui::RichText::new("⏺ REC").monospace().size(10.0).color(rec_col)
                        ).fill(rec_fill)).clicked() {
                            let next = crate::state::toggle_live_record(self.state.read().clone());
                            *self.state.write() = next;
                        }
                    }

                    ui.add_space(4.0);

                    // Heat slider
                    {
                        let mut heat = self.state.read().llm.heat;
                        let heat_color = if heat < 0.3 { theme::IRON }
                            else if heat < 0.6 { theme::ASH }
                            else if heat < 0.85 { theme::SMOKE }
                            else { theme::CHALK };
                        ui.label(egui::RichText::new("HEAT").color(heat_color).monospace().size(9.0));
                        if ui.add_sized(
                            [80.0, 16.0],
                            egui::Slider::new(&mut heat, 0.0..=1.0).show_value(false)
                        ).changed() {
                            self.state.write().llm.heat = heat;
                        }
                    }

                    ui.add_space(4.0);

                    // JAM toggle
                    let jam = self.state.read().llm.auto_jam;
                    let jam_color = if jam { theme::CHALK } else { theme::ASH };
                    if ui.button(egui::RichText::new("JAM").color(jam_color).monospace().size(10.0)).clicked() {
                        let mut next = self.state.read().clone();
                        next.llm.auto_jam = !next.llm.auto_jam;
                        let now_jamming = next.llm.auto_jam;
                        *self.state.write() = next;
                        if now_jamming {
                            let _ = self.llm_tx.try_send(LlmInput::Infer {
                                prompt: "start jamming".to_string(),
                                one_shot: false,
                            });
                        }
                    }

                    ui.add_space(4.0);

                    // Knob / Slider toggle
                    {
                        let use_sliders = self.state.read().ui_prefs.use_sliders;
                        let label = if use_sliders { "SLIDERS" } else { "KNOBS" };
                        let color  = if use_sliders { theme::CHALK } else { theme::ASH };
                        if ui.button(egui::RichText::new(label).color(color).monospace().size(9.0)).clicked() {
                            self.state.write().ui_prefs.use_sliders = !use_sliders;
                        }
                    }

                    // Right-aligned: VRAM/RAM bars + VOL slider + optional API link
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // API link
                        if let Some(port) = self.api_port {
                            let api_label = format!("API :{}", port);
                            let btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(&api_label).color(theme::SMOKE).monospace().size(8.5)
                                )
                                .fill(theme::VOID)
                                .stroke(egui::Stroke::new(1.0, theme::SLATE))
                            );
                            if btn.clicked() {
                                let url = format!("http://localhost:{}/api/schema", port);
                                let _ = webbrowser_open(&url);
                            }
                            if btn.hovered() {
                                btn.on_hover_text("Open API schema in browser");
                            }
                            ui.add_space(8.0);
                        }

                        // Monitor volume slider
                        let vol_color = if self.ui_volume < 0.4 { theme::IRON }
                            else if self.ui_volume < 0.75 { theme::ASH }
                            else { theme::SMOKE };
                        ui.label(egui::RichText::new(format!("{:.0}%", self.ui_volume * 100.0))
                            .color(vol_color).monospace().size(9.0));
                        if ui.add_sized(
                            [72.0, 16.0],
                            egui::Slider::new(&mut self.ui_volume, 0.0..=1.0).show_value(false)
                        ).changed() {
                            let _ = self.audio_tx.push(AudioCommand::SetMonitorVolume(self.ui_volume));
                        }
                        ui.label(egui::RichText::new("VOL").color(vol_color).monospace().size(9.0));

                        ui.add_space(8.0);

                        // VRAM / RAM compact progress bars
                        if let Ok(si) = self.sys_info.lock() {
                            ui.vertical(|ui| {
                                if si.vram_total_mb > 0 {
                                    let frac = si.vram_used_mb as f32 / si.vram_total_mb as f32;
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("VRAM").color(theme::IRON).monospace().size(8.0));
                                        ui.add_sized([54.0, 6.0], egui::ProgressBar::new(frac));
                                        ui.label(egui::RichText::new(
                                            crate::sysinfo::fmt_mb(si.vram_used_mb).to_string()
                                        ).color(theme::IRON).monospace().size(8.0));
                                    });
                                }
                                if si.ram_total_mb > 0 {
                                    let frac = si.ram_used_mb as f32 / si.ram_total_mb as f32;
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("RAM ").color(theme::IRON).monospace().size(8.0));
                                        ui.add_sized([54.0, 6.0], egui::ProgressBar::new(frac));
                                        ui.label(egui::RichText::new(
                                            crate::sysinfo::fmt_mb(si.ram_used_mb).to_string()
                                        ).color(theme::IRON).monospace().size(8.0));
                                    });
                                }
                            });
                        }
                    });
                });
            });
    }
}
