// ─── ui/header.rs ─────────────────────────────────────────────────────────────
// Menu bar and header panel (logo, transport, monitor vol, VRAM/RAM).

use crate::audio::AudioCommand;
use crate::export::{export_mp3, export_stems, export_wav};
use crate::state::save_project;
use crate::ui::{ImpulseApp, theme};
use egui::{Frame, TopBottomPanel};

impl ImpulseApp {
    /// Menu bar + header transport strip + log/scope combined panel.
    pub(super) fn draw_menu_and_header(&mut self, ctx: &egui::Context) {
        self.draw_menu_bar(ctx);
        self.draw_header_bar(ctx);
        self.draw_log_and_scope(ctx);
    }

    /// Combined log + visualizations panel.
    ///
    /// Single resizable `TopBottomPanel` with a 3-column internal layout:
    ///   Left:   LLM log (golden-ratio width, full height)
    ///   Center: event stream + bar oscilloscope stacked vertically
    ///   Right:  ring oscilloscope (square, full height)
    ///
    /// Each visualization can be toggled off in Preferences → Display.
    /// Disabled columns are reclaimed by the remaining elements.
    pub(super) fn draw_log_and_scope(&mut self, ctx: &egui::Context) {
        let screen_h = ctx.screen_rect().height();

        TopBottomPanel::top("header_info")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::same(0.0)),
            )
            .resizable(true)
            .min_height(60.0)
            .max_height(screen_h * 0.5)
            .default_height(160.0)
            .show(ctx, |ui| {
                let (show_bar, show_ring, show_stream) = {
                    let p = &self.state.read().ui_prefs;
                    (
                        p.show_bar_oscilloscope,
                        p.show_ring_oscilloscope,
                        p.show_event_stream,
                    )
                };

                let huth_col = if self.state.read().ui_prefs.huth_oscilloscope {
                    super::scope_footer::detect_note(&self.scope_buf, crate::audio::SAMPLE_RATE)
                        .map(theme::note_color)
                } else {
                    None
                };

                // Free-form lower band — no shared grid here; widths are set
                // by the visualizers themselves and by an interactive
                // splitter on the log/center seam.
                let avail_w = ui.available_width();
                let total_h = ui.available_height();
                let any_center = show_bar || show_stream;

                // Ring is square — width = height (like the original layout).
                let ring_w = if show_ring { total_h } else { 0.0 };

                // Center default: ~40 % of available width (~2× the previous
                // grid value). User drag persists into `self.lower_center_w`.
                let default_center_w = (avail_w * 0.40).round();
                if self.lower_center_w <= 0.0 {
                    self.lower_center_w = default_center_w;
                }
                let min_center_w = 100.0_f32;
                let min_log_w = 120.0_f32;
                let max_center_w = (avail_w - ring_w - min_log_w).max(min_center_w);
                self.lower_center_w = self.lower_center_w.clamp(min_center_w, max_center_w);
                let center_w = if any_center { self.lower_center_w } else { 0.0 };
                let log_w = (avail_w - center_w - ring_w).max(0.0);

                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(avail_w, total_h), egui::Sense::hover());
                let origin = rect.min;

                // ── LEFT: global log ────────────────────────────────────────
                let log_rect = egui::Rect::from_min_size(origin, egui::vec2(log_w, total_h));
                theme::draw_screen_panel(
                    ui.painter(),
                    log_rect,
                    egui::Rounding::same(theme::SCREEN_CHIP_ROUNDING),
                    theme::DEEP,
                );
                ui.allocate_ui_at_rect(log_rect.shrink(theme::HEADER_CHIP_INSET), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    egui::ScrollArea::vertical()
                        .id_source("global_log")
                        .stick_to_bottom(true)
                        .auto_shrink([false; 2])
                        .show(ui, |ui: &mut egui::Ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let trimmed = self.log_text.trim_end_matches('\n');
                            let job = super::llm_log_color::colorize_log(trimmed, theme::FOG);
                            ui.add(egui::Label::new(job).wrap().selectable(true));
                        });
                });

                // ── DRAG SPLITTER (between log and center) ─────────────────
                if any_center {
                    let split_x = origin.x + log_w;
                    let handle_rect = egui::Rect::from_min_size(
                        egui::pos2(split_x - 2.0, origin.y),
                        egui::vec2(4.0, total_h),
                    );
                    let resp = ui.interact(
                        handle_rect,
                        ui.id().with("lower_center_split"),
                        egui::Sense::drag(),
                    );
                    if resp.hovered() || resp.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if resp.dragged() {
                        // Dragging right = log gets wider, center shrinks.
                        self.lower_center_w = (self.lower_center_w - resp.drag_delta().x)
                            .clamp(min_center_w, max_center_w);
                    }
                }

                // ── CENTER: bar-osc (top half) + event stream (bottom half) ─
                let center_x = origin.x + log_w;
                let split_h = if show_bar && show_stream {
                    total_h * 0.5
                } else {
                    total_h
                };
                if show_bar {
                    let bar_rect = egui::Rect::from_min_size(
                        egui::pos2(center_x, origin.y),
                        egui::vec2(center_w, split_h),
                    );
                    ui.allocate_ui_at_rect(bar_rect, |ui| {
                        super::scope_footer::draw_scope_colored(
                            ui,
                            &self.scope_buf,
                            &self.scope_history,
                            bar_rect.width(),
                            bar_rect.height(),
                            huth_col,
                        );
                    });
                }
                if show_stream {
                    let stream_y = origin.y + if show_bar { split_h } else { 0.0 };
                    let stream_rect = egui::Rect::from_min_size(
                        egui::pos2(center_x, stream_y),
                        egui::vec2(center_w, split_h),
                    );
                    ui.allocate_ui_at_rect(stream_rect, |ui| {
                        let state = self.state.read();
                        let now = ctx.input(|i| i.time);
                        let secs_per_step = 60.0 / (state.sequencer.bpm as f64 * 4.0);
                        let elapsed = (now - self.last_step_time).max(0.0);
                        let frac = if state.sequencer.running && secs_per_step > 0.001 {
                            (elapsed / secs_per_step).clamp(0.0, 0.99)
                        } else {
                            0.0
                        };
                        let smooth = self.last_seq_step as f64 + frac;
                        let temperature = if self.spectrum_magnitudes.is_empty() {
                            f32::NAN
                        } else {
                            let bin_hz =
                                crate::audio::SAMPLE_RATE / crate::audio::spectrum::FFT_SIZE as f32;
                            crate::audio::spectrum::spectrum_temperature(
                                &self.spectrum_magnitudes,
                                bin_hz,
                                &theme::NOTE_TEMP,
                            )
                        };
                        super::widgets::event_stream(
                            ui,
                            &state,
                            smooth,
                            stream_rect.width(),
                            stream_rect.height(),
                            temperature,
                        );
                    });
                }

                // ── RIGHT: ring oscilloscope (square = panel height) ──────
                if show_ring {
                    let ring_rect = egui::Rect::from_min_size(
                        egui::pos2(origin.x + log_w + center_w, origin.y),
                        egui::vec2(ring_w, total_h),
                    );
                    ui.allocate_ui_at_rect(ring_rect, |ui| {
                        super::scope_footer::draw_ring_scope_colored(
                            ui,
                            &self.scope_buf,
                            &self.scope_history,
                            ring_w,
                            huth_col,
                        );
                    });
                }
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
                            .button(egui::RichText::new("New project").monospace().size(10.0))
                            .clicked()
                        {
                            // Re-open the wizard and forget the wizard_done
                            // flag so the user picks a fresh preset.
                            self.show_wizard = true;
                            // Snapshot keeps current rack/style intact until
                            // the user actually applies a preset, mirroring
                            // wizard "resume" behaviour.
                            ui.close_menu();
                        }
                        if ui
                            .button(
                                egui::RichText::new("Load latest project")
                                    .monospace()
                                    .size(10.0),
                            )
                            .clicked()
                        {
                            // Find the newest project-*.json in cwd and
                            // load it.  Avoids pulling in a file-picker
                            // dependency for the menu entry.
                            let latest = std::fs::read_dir(".")
                                .ok()
                                .into_iter()
                                .flatten()
                                .filter_map(|e| e.ok())
                                .filter(|e| {
                                    e.file_name().to_string_lossy().starts_with("project-")
                                        && e.file_name().to_string_lossy().ends_with(".json")
                                })
                                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
                            match latest {
                                Some(entry) => {
                                    let path = entry.path();
                                    match crate::state::load_project(&path) {
                                        Ok(loaded) => {
                                            *self.state.write() = loaded;
                                            self.push_audio_params();
                                            let msg = format!("[ loaded ← {} ]", path.display());
                                            log::info!("{}", msg);
                                            self.log_text.push_str(&format!("{}\n", msg));
                                        }
                                        Err(e) => {
                                            let msg = format!("[ load failed: {} ]", e);
                                            log::error!("{}", msg);
                                            self.log_text.push_str(&format!("{}\n", msg));
                                        }
                                    }
                                }
                                None => {
                                    let msg = "[ no project-*.json found in working dir ]";
                                    log::warn!("{}", msg);
                                    self.log_text.push_str(&format!("{}\n", msg));
                                }
                            }
                            ui.close_menu();
                        }
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
        // Chip column layout (must sum to HEADER_TOTAL_COLS = 105).
        // [TITLE 18][STATUS 7][WARN 5][BPM 6][TRANSPORT 6][HEAT 34][MUTE+MON 21][VRAM/RAM 8]
        const C_TITLE: u32 = 18;
        const C_STATUS: u32 = 7;
        const C_WARN: u32 = 5;
        const C_BPM: u32 = 6;
        const C_TRANSPORT: u32 = 6;
        const C_HEAT: u32 = 34;
        const C_MUTE_MON: u32 = 21;
        const C_VRAM_RAM: u32 = 8;

        TopBottomPanel::top("header")
            .frame(
                Frame::none()
                    .fill(theme::VOID)
                    .inner_margin(egui::Margin::symmetric(0.0, 0.0)),
            )
            .show(ctx, |ui| {
                let avail_w = ui.available_width();
                let (cell_w, cell_h) = theme::header_cell_size(avail_w);
                let total_h = cell_h * theme::HEADER_TOP_ROWS as f32;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(avail_w, total_h), egui::Sense::hover());
                let origin = rect.min;
                let cell = |col, span_cols| {
                    theme::header_cell_rect(
                        origin,
                        cell_w,
                        cell_h,
                        col,
                        0,
                        span_cols,
                        theme::HEADER_TOP_ROWS,
                    )
                };
                let mut col = 0u32;

                // ── TITLE ───────────────────────────────────────────────────
                let r = cell(col, C_TITLE);
                col += C_TITLE;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("IMPULSE • INSTRUCT")
                                .color(theme::CHALK)
                                .size(15.0)
                                .monospace()
                                .strong(),
                        );
                    });
                });

                // ── STATUS (audio analysis as a 5-column dB table) ─────────
                let r = cell(col, C_STATUS);
                col += C_STATUS;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    let inner = ui.available_rect_before_wrap();
                    let painter = ui.painter();
                    let cols = [("SUB", 0), ("LOW", 1), ("MID", 2), ("HI", 3), ("PK", 4)];
                    let n = cols.len() as f32;
                    let col_w = inner.width() / n;
                    let label_y = inner.min.y + inner.height() * 0.30;
                    let value_y = inner.min.y + inner.height() * 0.72;
                    let label_font = egui::FontId::monospace(7.5);
                    let value_font = egui::FontId::monospace(9.5);
                    let (sub, low, mid, hi, pk) = self
                        .audio_analysis
                        .as_ref()
                        .map(|a| {
                            (
                                a.sub_rms_db,
                                a.low_rms_db,
                                a.mid_rms_db,
                                a.high_rms_db,
                                a.peak_db,
                            )
                        })
                        .unwrap_or((-96.0, -96.0, -96.0, -96.0, -96.0));
                    let vals = [sub, low, mid, hi, pk];
                    for (i, (label, _)) in cols.iter().enumerate() {
                        let cx = inner.min.x + col_w * (i as f32 + 0.5);
                        painter.text(
                            egui::pos2(cx, label_y),
                            egui::Align2::CENTER_CENTER,
                            label,
                            label_font.clone(),
                            theme::IRON,
                        );
                        // Color the value by signal strength: bright when loud,
                        // dim when near silence.
                        let v = vals[i];
                        let col = if v > -3.0 {
                            theme::CHALK
                        } else if v > -12.0 {
                            theme::FOG
                        } else if v > -30.0 {
                            theme::SMOKE
                        } else {
                            theme::IRON
                        };
                        let txt = if v <= -90.0 {
                            "—".to_string()
                        } else {
                            format!("{:>3.0}", v)
                        };
                        painter.text(
                            egui::pos2(cx, value_y),
                            egui::Align2::CENTER_CENTER,
                            txt,
                            value_font.clone(),
                            col,
                        );
                    }
                });

                // ── WARN (alerts) ──────────────────────────────────────────
                let r = cell(col, C_WARN);
                col += C_WARN;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    ui.centered_and_justified(|ui| {
                        let snap = self.state.read().audio_snapshot.clone();
                        let alert_part = snap.split("!! ").nth(1).map(|s| s.to_string());
                        if let Some(alert) = alert_part {
                            let parts: Vec<&str> = alert.split(", ").collect();
                            let display = if parts.len() <= 1 {
                                alert.clone()
                            } else {
                                let t = ui.ctx().input(|i| i.time) as usize;
                                let idx = t % parts.len();
                                parts[idx].to_string()
                            };
                            ui.label(
                                egui::RichText::new(display)
                                    .color(theme::CHALK)
                                    .monospace()
                                    .size(8.5)
                                    .strong(),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("OK")
                                    .color(theme::IRON)
                                    .monospace()
                                    .size(8.5),
                            );
                        }
                    });
                });

                let (running, bpm, live_record) = {
                    let s = self.state.read();
                    (s.sequencer.running, s.sequencer.bpm, s.live_record)
                };

                // ── BPM display ──────────────────────────────────────────────
                let r = cell(col, C_BPM);
                col += C_BPM;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    let run_color = if running { theme::CHALK } else { theme::ASH };
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{:.0} BPM", bpm))
                                .color(run_color)
                                .size(13.0)
                                .monospace()
                                .strong(),
                        );
                    });
                });

                // ── STOP / REC (compact, centered) ───────────────────────
                let r = cell(col, C_TRANSPORT);
                col += C_TRANSPORT;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    // Center the button group horizontally inside the chip.
                    let play_w = 22.0;
                    let rec_w = 28.0;
                    let group_w = play_w + 2.0 + rec_w;
                    let lpad = ((ui.available_width() - group_w) * 0.5).max(0.0);
                    ui.horizontal_centered(|ui| {
                        ui.add_space(lpad);
                        let play_label = if running { "■" } else { "▶" };
                        if ui
                            .add_sized(
                                [play_w, 20.0],
                                egui::Button::new(
                                    egui::RichText::new(play_label).monospace().size(11.0),
                                ),
                            )
                            .on_hover_text(if running { "Stop" } else { "Play" })
                            .clicked()
                        {
                            let next =
                                crate::state::toggle_sequencer_running(self.state.read().clone());
                            *self.state.write() = next;
                        }
                        let rec_col = if live_record && running {
                            theme::CHALK
                        } else {
                            theme::ASH
                        };
                        let rec_fill = if live_record && running {
                            theme::IRON
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        if ui
                            .add_sized(
                                [rec_w, 20.0],
                                egui::Button::new(
                                    egui::RichText::new("REC")
                                        .monospace()
                                        .size(9.0)
                                        .color(rec_col),
                                )
                                .fill(rec_fill),
                            )
                            .clicked()
                        {
                            let next = crate::state::toggle_live_record(self.state.read().clone());
                            *self.state.write() = next;
                        }
                    });
                });

                // ── HEAT ─────────────────────────────────────────────────────
                let r = cell(col, C_HEAT);
                col += C_HEAT;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.horizontal_centered(|ui| {
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
                        // Slider expands to fill the chip; reserve room for label
                        // and percent.
                        let slider_w = (ui.available_width() - 40.0).max(60.0);
                        if ui
                            .scope(|ui| {
                                ui.spacing_mut().slider_width = slider_w;
                                ui.add(egui::Slider::new(&mut heat, 0.0..=1.0).show_value(false))
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
                    });
                });

                // ── MONITOR + MUTE + slider ────────────────────────────────
                // Order: label → mute button → slider.
                let r = cell(col, C_MUTE_MON);
                col += C_MUTE_MON;
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.horizontal_centered(|ui| {
                        let muted = self.ui_volume <= 0.0;
                        let vol_col = if self.ui_volume < 0.5 {
                            theme::ASH
                        } else {
                            theme::SMOKE
                        };
                        ui.label(
                            egui::RichText::new("MONITOR")
                                .color(vol_col)
                                .monospace()
                                .size(8.0),
                        );
                        let mute_text_col = if muted { theme::CHALK } else { theme::ASH };
                        let mute_fill = if muted {
                            theme::IRON
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        let mute_label = if muted { "MUTED" } else { "MUTE" };
                        if ui
                            .add_sized(
                                [56.0, 20.0],
                                egui::Button::new(
                                    egui::RichText::new(mute_label)
                                        .monospace()
                                        .size(9.5)
                                        .color(mute_text_col),
                                )
                                .fill(mute_fill),
                            )
                            .on_hover_text("Mute / unmute monitor output")
                            .clicked()
                        {
                            if muted {
                                self.ui_volume = if self.pre_mute_volume > 0.0 {
                                    self.pre_mute_volume
                                } else {
                                    1.0
                                };
                            } else {
                                self.pre_mute_volume = self.ui_volume.max(0.0);
                                self.ui_volume = 0.0;
                            }
                            let _ = self
                                .audio_tx
                                .push(AudioCommand::SetMonitorVolume(self.ui_volume));
                        }
                        // Monitor slider — ~30 % shorter than the chip width.
                        let slider_w = ((ui.available_width() - 4.0) * 0.70).max(60.0);
                        if ui
                            .scope(|ui| {
                                ui.spacing_mut().slider_width = slider_w;
                                ui.add(
                                    egui::Slider::new(&mut self.ui_volume, 0.0..=1.0)
                                        .show_value(false),
                                )
                            })
                            .inner
                            .changed()
                        {
                            if self.ui_volume > 0.0 {
                                self.pre_mute_volume = self.ui_volume;
                            }
                            let _ = self
                                .audio_tx
                                .push(AudioCommand::SetMonitorVolume(self.ui_volume));
                        }
                    });
                });

                // ── VRAM / RAM ───────────────────────────────────────────────
                let r = cell(col, C_VRAM_RAM);
                let _ = col;
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
                theme::screen_chip_at(ui, r, theme::VOID, |ui| {
                    // Tight stack at the top of the chip — no inter-bar gap,
                    // no top padding beyond the bezel inset.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let bar = |ui: &mut egui::Ui, label: &str, frac: f32| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .color(theme::ASH)
                                    .monospace()
                                    .size(8.5),
                            );
                            let bar_w = (ui.available_width() - 26.0).max(20.0);
                            let (br, _) = ui
                                .allocate_exact_size(egui::vec2(bar_w, 6.5), egui::Sense::hover());
                            let p = ui.painter();
                            p.rect_filled(br, 1.0, egui::Color32::from_gray(30));
                            let fw = (br.width() * frac.clamp(0.0, 1.0)).max(0.0);
                            if fw > 0.0 {
                                p.rect_filled(
                                    egui::Rect::from_min_size(br.min, egui::vec2(fw, br.height())),
                                    1.0,
                                    egui::Color32::from_gray(if frac > 0.85 { 160 } else { 80 }),
                                );
                            }
                            ui.label(
                                egui::RichText::new(format!("{}%", (frac * 100.0) as u32))
                                    .color(theme::ASH)
                                    .monospace()
                                    .size(8.5),
                            );
                        });
                    };
                    if has_vram {
                        bar(ui, "V", vram_used as f32 / vram_total as f32);
                    }
                    if has_ram {
                        bar(ui, "R", ram_used as f32 / ram_total as f32);
                    }
                });
            });
    }
}
