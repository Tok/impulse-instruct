// ─── ui/header.rs ─────────────────────────────────────────────────────────────
// Menu bar and header panel (logo, transport, monitor vol, VRAM/RAM).

use crate::audio::AudioCommand;
use crate::export::{export_mp3, export_stems, export_wav};
use crate::state::save_project;
use crate::ui::{ImpulseApp, theme, webbrowser_open};
use egui::{Frame, TopBottomPanel};

impl ImpulseApp {
    /// Menu bar + header transport strip + global log.
    pub(super) fn draw_menu_and_header(&mut self, ctx: &egui::Context) {
        self.draw_menu_bar(ctx);
        self.draw_header_bar(ctx);
        self.draw_log_strip(ctx);
    }

    /// Global log strip — shared across all agents, below the header.
    /// Agent round-robin indicator on the right side (2 rows).
    fn draw_log_strip(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("log_strip")
            .frame(Frame::none().fill(theme::PIT).inner_margin(egui::Margin {
                left: 8.0,
                right: 8.0,
                top: 2.0,
                bottom: 2.0,
            }))
            .resizable(true)
            .min_height(20.0)
            .default_height(100.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // ── Log scroll area (fills remaining width) ──────────
                    let agent_panel_width = 120.0;
                    let avail = ui.available_width() - agent_panel_width - 8.0;
                    ui.allocate_ui(egui::vec2(avail.max(100.0), ui.available_height()), |ui| {
                        egui::ScrollArea::vertical()
                            .id_source("global_log")
                            .stick_to_bottom(true)
                            .auto_shrink([false; 2])
                            .show(ui, |ui: &mut egui::Ui| {
                                let job =
                                    super::llm_strip::colorize_log(&self.log_text, theme::FOG);
                                ui.add(egui::Label::new(job).selectable(true));
                            });
                    });

                    ui.separator();

                    // ── Agent status (right side, 2-row vertical layout) ─
                    ui.allocate_ui(egui::vec2(agent_panel_width, ui.available_height()), |ui| {
                        let s = self.state.read();
                        let agents = &s.llm_agents;
                        if agents.is_empty() {
                            ui.label(
                                egui::RichText::new("no agents")
                                    .color(theme::IRON)
                                    .monospace()
                                    .size(8.0),
                            );
                            return;
                        }
                        let time = ctx.input(|i| i.time) as f32;
                        let enabled_agents: Vec<_> = agents
                            .iter()
                            .filter(|a| s.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
                            .collect();
                        if enabled_agents.is_empty() {
                            return;
                        }

                        // Split into 2 rows using a grid
                        let half = enabled_agents.len().div_ceil(2);
                        for (row_idx, chunk) in enabled_agents.chunks(half).enumerate() {
                            if row_idx > 0 {
                                ui.add_space(2.0);
                            }
                            ui.horizontal(|ui| {
                                for a in chunk {
                                    let dot_col = if a.is_inferring {
                                        let p =
                                            (time * 4.0 * std::f32::consts::TAU).sin() * 0.3 + 0.7;
                                        egui::Color32::from_gray((220.0 * p) as u8)
                                    } else {
                                        egui::Color32::from_gray(60)
                                    };
                                    ui.label(egui::RichText::new("●").color(dot_col).size(8.0));
                                    let name_col = if a.is_inferring {
                                        theme::CHALK
                                    } else {
                                        theme::IRON
                                    };
                                    ui.label(
                                        egui::RichText::new(&a.persona_name)
                                            .color(name_col)
                                            .monospace()
                                            .size(8.0),
                                    );
                                }
                            });
                        }
                        if agents.iter().any(|a| a.is_inferring) {
                            ctx.request_repaint();
                        }
                    });
                });
            });
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

                    ui.menu_button(egui::RichText::new("View").monospace().size(10.0), |ui| {
                        if ui
                            .button(egui::RichText::new("Compact All").monospace().size(10.0))
                            .clicked()
                        {
                            for m in &self.state.read().rack.modules {
                                self.module_scales.insert(m.kind, 0.6);
                            }
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Expand All").monospace().size(10.0))
                            .clicked()
                        {
                            self.module_scales.clear();
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Arrange").monospace().size(10.0))
                            .clicked()
                        {
                            self.state.write().rack.arrange_canonical();
                            self.session_dirty = true;
                            ui.close_menu();
                        }
                        if ui
                            .button(egui::RichText::new("Reset Layout").monospace().size(10.0))
                            .clicked()
                        {
                            self.module_scales.clear();
                            self.state.write().rack.arrange_canonical();
                            self.session_dirty = true;
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
                    // ── LOGO ─────────────────────────────────────────────────
                    ui.label(
                        egui::RichText::new("◆ IMPULSE INSTRUCT")
                            .color(theme::CHALK)
                            .size(12.0)
                            .monospace()
                            .strong(),
                    );

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

                    // ── HEAT (global jam intensity) ──────────────────────────
                    {
                        let mut heat = self.state.read().llm.heat;
                        let heat_col = if heat < 0.3 {
                            theme::ASH
                        } else if heat < 0.6 {
                            theme::SMOKE
                        } else {
                            theme::FOG
                        };
                        ui.label(
                            egui::RichText::new("HEAT")
                                .color(heat_col)
                                .monospace()
                                .size(8.5),
                        );
                        if ui
                            .scope(|ui| {
                                ui.spacing_mut().slider_width = 180.0;
                                ui.add(
                                    egui::Slider::new(&mut heat, 0.0..=1.0).show_value(false),
                                )
                            })
                            .inner
                            .changed()
                        {
                            self.state.write().llm.heat = heat;
                        }
                        ui.label(
                            egui::RichText::new(format!("{}%", (heat * 100.0) as u32))
                                .color(heat_col)
                                .monospace()
                                .size(8.5),
                        );
                    }

                    // (Agent status moved to log strip right side)

                    ui.separator();

                    // ── RIGHT controls (KNOBS, MON, VRAM/RAM, API) ─────────
                    {
                        let (has_vram, has_ram, vram_used, vram_total, ram_used, ram_total) = self
                            .sys_info
                            .lock()
                            .map(|si| (si.vram_total_mb > 0, si.ram_total_mb > 0, si.vram_used_mb, si.vram_total_mb, si.ram_used_mb, si.ram_total_mb))
                            .unwrap_or((false, false, 0, 0, 0, 0));
                        const MON_W: f32 = 180.0;
                        const BAR_W: f32 = 80.0;
                        const BAR_H: f32 = 5.0;
                        ui.scope(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                            ui.spacing_mut().item_spacing.x = 4.0;
                            // KNOBS / SLIDERS toggle
                            let use_sliders = self.state.read().ui_prefs.use_sliders;
                            let ks_col = if use_sliders { theme::SMOKE } else { theme::ASH };
                            if ui.add(egui::Button::new(
                                egui::RichText::new(if use_sliders { "SLIDERS" } else { "KNOBS" })
                                    .color(ks_col).monospace().size(8.5),
                            ).fill(egui::Color32::TRANSPARENT)).clicked() {
                                self.state.write().ui_prefs.use_sliders = !use_sliders;
                            }
                            ui.separator();
                            // Monitor volume
                            let vol_col = if self.ui_volume < 0.4 { theme::ASH }
                                else if self.ui_volume < 0.75 { theme::SMOKE }
                                else { theme::FOG };
                            ui.label(egui::RichText::new("MON").color(vol_col).monospace().size(8.5))
                                .on_hover_text("Monitor volume");
                            if ui.scope(|ui| {
                                ui.spacing_mut().slider_width = MON_W;
                                ui.add(egui::Slider::new(&mut self.ui_volume, 0.0..=1.0).show_value(false))
                            }).inner.changed() {
                                let _ = self.audio_tx.push(AudioCommand::SetMonitorVolume(self.ui_volume));
                            }
                            ui.label(egui::RichText::new(format!("{}%", (self.ui_volume * 100.0) as u32))
                                .color(vol_col).monospace().size(8.5));
                            // VRAM / RAM bars
                            if has_vram || has_ram {
                                const TRACK: egui::Color32 = egui::Color32::from_gray(38);
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    let draw_bar = |ui: &mut egui::Ui, label: &str, frac: f32, fill: egui::Color32| {
                                        ui.horizontal(|ui| {
                                            ui.label(egui::RichText::new(label).color(theme::ASH).monospace().size(8.0));
                                            let (br, _) = ui.allocate_exact_size(egui::vec2(BAR_W, BAR_H), egui::Sense::hover());
                                            let p = ui.painter();
                                            p.rect_filled(br, 1.0, TRACK);
                                            let fw = (br.width() * frac.clamp(0.0, 1.0)).max(0.0);
                                            if fw > 0.0 {
                                                p.rect_filled(egui::Rect::from_min_size(br.min, egui::vec2(fw, br.height())), 1.0, fill);
                                            }
                                            ui.label(egui::RichText::new(format!("{}%", (frac * 100.0) as u32))
                                                .color(theme::ASH).monospace().size(7.0));
                                        });
                                    };
                                    if has_vram {
                                        let frac = vram_used as f32 / vram_total as f32;
                                        draw_bar(ui, "VRAM", frac, egui::Color32::from_gray(if frac > 0.85 { 160 } else { 90 }));
                                    }
                                    if has_ram {
                                        let frac = ram_used as f32 / ram_total as f32;
                                        draw_bar(ui, "RAM ", frac, egui::Color32::from_gray(if frac > 0.85 { 160 } else { 90 }));
                                    }
                                });
                            }
                            // API badge
                            if let Some(port) = self.api_port {
                                ui.add_space(2.0);
                                if ui.add(egui::Button::new(
                                    egui::RichText::new(format!(":{port}")).color(theme::SMOKE).monospace().size(8.5),
                                ).fill(theme::VOID).stroke(egui::Stroke::new(1.0, theme::SLATE))).clicked() {
                                    let _ = webbrowser_open(&format!("http://localhost:{port}/api/schema"));
                                }
                            }
                        });
                    }
                });
            });
    }
}
