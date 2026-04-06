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
                    // ── LOGO + MODEL  (fixed 180px) ───────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(180.0);
                        ui.set_max_width(180.0);
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
                                    .width(168.0)
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

                    // ── MODEL STATUS  (fixed 190px) ───────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(190.0);
                        ui.set_max_width(190.0);
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                        ui.spacing_mut().item_spacing.x = 4.0;

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
                                        egui::RichText::new("run download-models.sh")
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
                                        egui::RichText::new("build-llama-server.sh")
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
                                        ui.label(
                                            egui::RichText::new(format!("{:.0}t/s", tps))
                                                .color(theme::SMOKE)
                                                .size(9.0)
                                                .monospace(),
                                        );
                                        // Context bar (same style as VRAM/RAM bars)
                                        ui.label(
                                            egui::RichText::new("CTX")
                                                .color(ctx_color)
                                                .monospace()
                                                .size(8.0),
                                        );
                                        let ctx_frac = (ctx_pct / 100.0).clamp(0.0, 1.0);
                                        let (br, _) = ui.allocate_exact_size(
                                            egui::vec2(80.0, 5.0),
                                            egui::Sense::hover(),
                                        );
                                        let p = ui.painter();
                                        p.rect_filled(br, 1.0, egui::Color32::from_gray(38));
                                        let fw = (br.width() * ctx_frac).max(0.0);
                                        if fw > 0.0 {
                                            p.rect_filled(
                                                egui::Rect::from_min_size(
                                                    br.min,
                                                    egui::vec2(fw, br.height()),
                                                ),
                                                1.0,
                                                ctx_color,
                                            );
                                        }
                                        ui.label(
                                            egui::RichText::new(format!("{ctx_used}/{ctx_max}"))
                                                .color(ctx_color)
                                                .size(8.0)
                                                .monospace(),
                                        );
                                    });
                                    let line2 = if tthink > 0 {
                                        format!("in:{ptok} out:{ctok} ~{tthink}t")
                                    } else {
                                        format!("in:{ptok}  out:{ctok}")
                                    };
                                    ui.label(
                                        egui::RichText::new(line2)
                                            .color(theme::ASH)
                                            .size(8.0)
                                            .monospace(),
                                    );
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
                                    .on_hover_text("Auto-compact context when > 85% full")
                                    .clicked()
                                {
                                    self.state.write().llm.auto_compact = !auto_compact;
                                }
                            }
                        });
                    });

                    ui.separator();

                    // ── TRANSPORT  (fixed 150px) ──────────────────────────────
                    ui.scope(|ui| {
                        ui.set_min_width(150.0);
                        ui.set_max_width(150.0);
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let (running, bpm, live_record) = {
                            let s = self.state.read();
                            (s.sequencer.running, s.sequencer.bpm, s.live_record)
                        };
                        ui.horizontal(|ui| {
                            ui.scope(|ui| {
                                ui.set_min_width(50.0);
                                ui.set_max_width(50.0);
                                let run_color = if running { theme::CHALK } else { theme::ASH };
                                ui.label(
                                    egui::RichText::new(format!("{:.0} BPM", bpm))
                                        .color(run_color)
                                        .size(11.0)
                                        .monospace(),
                                );
                            });
                            let play_label = if running { "■ STOP" } else { "▶ PLAY" };
                            if ui
                                .add_sized(
                                    [56.0, 20.0],
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
                                        egui::RichText::new("REC")
                                            .monospace()
                                            .size(9.5)
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

                    // ── HEAT + TEMP sliders + RIGHT controls ─────────────────
                    // Two-row vertical: HEAT slider (row 1) + TEMP slider (row 2).
                    // Right controls sit to the right of row 1 only.
                    // Two-frame width approach: right controls' actual width is
                    // stored in egui memory; the NEXT frame uses that to size sliders.
                    {
                        let (has_vram, has_ram, vram_used, vram_total, ram_used, ram_total) = self
                            .sys_info
                            .lock()
                            .map(|si| {
                                (
                                    si.vram_total_mb > 0,
                                    si.ram_total_mb > 0,
                                    si.vram_used_mb,
                                    si.vram_total_mb,
                                    si.ram_used_mb,
                                    si.ram_total_mb,
                                )
                            })
                            .unwrap_or((false, false, 0, 0, 0, 0));
                        let right_w_id = egui::Id::new("header_right_w");
                        let right_w: f32 =
                            ctx.memory(|m| m.data.get_temp(right_w_id)).unwrap_or(380.0);

                        const MON_W: f32 = 180.0;
                        const BAR_W: f32 = 80.0;
                        const BAR_H: f32 = 5.0;

                        let mut heat = self.state.read().llm.heat;
                        let mut temp = self.state.read().llm.temperature;
                        let (heat_color, heat_tier) = if heat < 0.3 {
                            (theme::ASH, "COOL")
                        } else if heat < 0.6 {
                            (theme::SMOKE, "WARM")
                        } else if heat < 0.85 {
                            (theme::FOG, "HOT")
                        } else if heat < 0.95 {
                            (theme::CHALK, "FIRE")
                        } else {
                            (egui::Color32::WHITE, "CHAOS")
                        };

                        // Pre-compute shared grid column widths for both rows.
                        const LABEL_W: f32 = 36.0;
                        const DRAG_W: f32 = 48.0;
                        const RESET_W: f32 = 16.0;
                        let full_w = ui.available_width();
                        let left_w = (full_w - right_w).max(60.0);
                        let item_sp = ui.spacing().item_spacing.x;
                        let track_w =
                            (left_w - LABEL_W - DRAG_W - RESET_W - item_sp * 3.0).max(40.0);

                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            // ── Row 1: HEAT label + slider + value + right controls
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                // Label
                                ui.add_sized(
                                    [LABEL_W, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new("HEAT")
                                            .color(heat_color)
                                            .monospace()
                                            .size(8.5),
                                    ),
                                )
                                .on_hover_text(heat_tier);
                                // Slider
                                let heat_resp = ui
                                    .scope(|ui| {
                                        ui.spacing_mut().slider_width = track_w;
                                        ui.add(
                                            egui::Slider::new(&mut heat, 0.0..=1.0)
                                                .show_value(false)
                                                .trailing_fill(true),
                                        )
                                    })
                                    .inner;
                                if heat_resp.changed() {
                                    self.state.write().llm.heat = heat;
                                }
                                heat_resp.on_hover_text(
                                    "Jam energy — mutation rate. CHAOS = maximum rewriting.",
                                );
                                // Value
                                let mut pct = heat * 100.0;
                                if ui
                                    .add_sized(
                                        [DRAG_W, 18.0],
                                        egui::DragValue::new(&mut pct)
                                            .range(0.0..=100.0)
                                            .speed(0.5)
                                            .suffix("%")
                                            .fixed_decimals(0),
                                    )
                                    .changed()
                                {
                                    self.state.write().llm.heat = pct / 100.0;
                                }
                                // Reset
                                if ui
                                    .add_sized(
                                        [RESET_W, 18.0],
                                        egui::Button::new(
                                            egui::RichText::new("↺").color(theme::ASH).size(9.0),
                                        )
                                        .fill(egui::Color32::TRANSPARENT),
                                    )
                                    .on_hover_text("Reset heat to 40%")
                                    .clicked()
                                {
                                    self.state.write().llm.heat = 0.4;
                                }

                                // ── Right controls ───────────────────────────────
                                let right_resp = ui
                                    .scope(|ui| {
                                        ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.horizontal_centered(|ui| {
                                            // KNOBS / SLIDERS toggle
                                            let use_sliders =
                                                self.state.read().ui_prefs.use_sliders;
                                            let ks_col = if use_sliders {
                                                theme::SMOKE
                                            } else {
                                                theme::ASH
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new(if use_sliders {
                                                            "SLIDERS"
                                                        } else {
                                                            "KNOBS"
                                                        })
                                                        .color(ks_col)
                                                        .monospace()
                                                        .size(8.5),
                                                    )
                                                    .fill(egui::Color32::TRANSPARENT),
                                                )
                                                .clicked()
                                            {
                                                self.state.write().ui_prefs.use_sliders =
                                                    !use_sliders;
                                            }

                                            ui.separator();

                                            // Monitor volume
                                            let vol_col = if self.ui_volume < 0.4 {
                                                theme::ASH
                                            } else if self.ui_volume < 0.75 {
                                                theme::SMOKE
                                            } else {
                                                theme::FOG
                                            };
                                            ui.label(
                                                egui::RichText::new("MON")
                                                    .color(vol_col)
                                                    .monospace()
                                                    .size(8.5),
                                            )
                                            .on_hover_text(
                                                "Monitor volume — listen only, not export volume",
                                            );
                                            if ui
                                                .scope(|ui| {
                                                    ui.spacing_mut().slider_width = MON_W;
                                                    ui.add(
                                                        egui::Slider::new(
                                                            &mut self.ui_volume,
                                                            0.0..=1.0,
                                                        )
                                                        .show_value(false),
                                                    )
                                                })
                                                .inner
                                                .changed()
                                            {
                                                let _ = self.audio_tx.push(
                                                    AudioCommand::SetMonitorVolume(self.ui_volume),
                                                );
                                            }

                                            // VRAM / RAM bars
                                            if has_vram || has_ram {
                                                const TRACK: egui::Color32 =
                                                    egui::Color32::from_gray(38);
                                                ui.add_space(8.0);
                                                ui.vertical(|ui| {
                                                    let draw_bar = |ui: &mut egui::Ui,
                                                                    label: &str,
                                                                    frac: f32,
                                                                    fill: egui::Color32| {
                                                        ui.horizontal(|ui| {
                                                            ui.label(
                                                                egui::RichText::new(label)
                                                                    .color(theme::ASH)
                                                                    .monospace()
                                                                    .size(8.0),
                                                            );
                                                            let (br, _) =
                                                                ui.allocate_exact_size(
                                                                    egui::vec2(BAR_W, BAR_H),
                                                                    egui::Sense::hover(),
                                                                );
                                                            let p = ui.painter();
                                                            p.rect_filled(br, 1.0, TRACK);
                                                            let fw = (br.width()
                                                                * frac.clamp(0.0, 1.0))
                                                            .max(0.0);
                                                            if fw > 0.0 {
                                                                p.rect_filled(
                                                                    egui::Rect::from_min_size(
                                                                        br.min,
                                                                        egui::vec2(
                                                                            fw,
                                                                            br.height(),
                                                                        ),
                                                                    ),
                                                                    1.0,
                                                                    fill,
                                                                );
                                                            }
                                                        });
                                                    };
                                                    if has_vram {
                                                        let frac =
                                                            vram_used as f32 / vram_total as f32;
                                                        draw_bar(
                                                            ui,
                                                            "VRAM",
                                                            frac,
                                                            egui::Color32::from_gray(
                                                                if frac > 0.85 { 160 } else { 90 },
                                                            ),
                                                        );
                                                    }
                                                    if has_ram {
                                                        let frac =
                                                            ram_used as f32 / ram_total as f32;
                                                        draw_bar(
                                                            ui,
                                                            "RAM ",
                                                            frac,
                                                            egui::Color32::from_gray(
                                                                if frac > 0.85 { 160 } else { 90 },
                                                            ),
                                                        );
                                                    }
                                                });
                                            }

                                            // API badge
                                            if let Some(port) = self.api_port {
                                                ui.add_space(2.0);
                                                let api_label = format!(":{port}");
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
                                                    let url = format!(
                                                        "http://localhost:{port}/api/schema"
                                                    );
                                                    let _ = webbrowser_open(&url);
                                                }
                                            }
                                        });
                                    })
                                    .response;

                                // Store width for next frame.
                                ctx.memory_mut(|m| {
                                    m.data.insert_temp(
                                        right_w_id,
                                        right_resp.rect.width() + ui.spacing().item_spacing.x,
                                    );
                                });
                            });

                            // ── Row 2: TEMP label + slider + value ───────────────
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                // Label (same width as HEAT)
                                ui.add_sized(
                                    [LABEL_W, 18.0],
                                    egui::Label::new(
                                        egui::RichText::new("TEMP")
                                            .color(theme::ASH)
                                            .monospace()
                                            .size(8.5),
                                    ),
                                );
                                // Slider (same track width as HEAT)
                                let temp_resp = ui
                                    .scope(|ui| {
                                        ui.spacing_mut().slider_width = track_w;
                                        ui.add(
                                            egui::Slider::new(&mut temp, 0.0..=2.0)
                                                .show_value(false),
                                        )
                                    })
                                    .inner;
                                if temp_resp.changed() {
                                    self.state.write().llm.temperature = temp;
                                }
                                temp_resp.on_hover_text(
                                    "LLM sampling temperature (0–2). Higher = more varied output.",
                                );
                                // Value (same width as HEAT)
                                if ui
                                    .add_sized(
                                        [DRAG_W, 18.0],
                                        egui::DragValue::new(&mut temp)
                                            .range(0.0..=2.0)
                                            .speed(0.01)
                                            .fixed_decimals(2),
                                    )
                                    .changed()
                                {
                                    self.state.write().llm.temperature = temp;
                                }
                                // Reset
                                if ui
                                    .add_sized(
                                        [RESET_W, 18.0],
                                        egui::Button::new(
                                            egui::RichText::new("↺").color(theme::ASH).size(9.0),
                                        )
                                        .fill(egui::Color32::TRANSPARENT),
                                    )
                                    .on_hover_text("Reset temperature to 0.90")
                                    .clicked()
                                {
                                    self.state.write().llm.temperature = 0.9;
                                }
                            });
                        });
                    }
                });
            });
    }
}
