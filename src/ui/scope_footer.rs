// ─── ui/scope_footer.rs ──────────────────────────────────────────────────────
// Oscilloscope waveform display and DSP load sparkline — extracted from mod.rs
// to stay under the 1000-line limit.

use crate::ui::theme;

/// Draw oscilloscope with explicit width and height (convenience wrapper).
#[allow(dead_code)]
pub fn draw_scope_sized(
    ui: &mut egui::Ui,
    buf: &[f32],
    history: &std::collections::VecDeque<Vec<f32>>,
    w: f32,
    h: f32,
) {
    draw_scope_colored(ui, buf, history, w, h, None);
}

/// Draw oscilloscope with optional Huth color override for the waveform.
pub fn draw_scope_colored(
    ui: &mut egui::Ui,
    buf: &[f32],
    history: &std::collections::VecDeque<Vec<f32>>,
    w: f32,
    h: f32,
    huth_color: Option<egui::Color32>,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, egui::Rounding::ZERO, theme::PIT);
    painter.rect_stroke(
        rect,
        egui::Rounding::ZERO,
        egui::Stroke::new(1.0, theme::SLATE),
    );

    let w = rect.width();
    let mid = rect.center().y;
    let amp = rect.height() * 0.45;

    // Draw older frames with fading brightness (phosphor persistence)
    let hist_len = history.len();
    for (age, frame) in history.iter().enumerate() {
        let n = frame.len();
        if n < 2 {
            continue;
        }
        // Older frames are dimmer: brightness ramps from ~15 to ~90
        // More pronounced phosphor glow — recent trails are clearly visible
        let t = age as f32 / hist_len.max(1) as f32;
        let brightness = (15.0 + t * 75.0) as u8;
        let thickness = 1.0 + t * 0.8; // trails get slightly thicker toward recent
        let col = egui::Color32::from_gray(brightness);
        let mut prev = egui::Pos2::new(rect.min.x, mid - frame[0].clamp(-1.0, 1.0) * amp);
        for (i, &s) in frame.iter().enumerate().skip(1) {
            let x = rect.min.x + (i as f32 / (n - 1) as f32) * w;
            let cur = egui::Pos2::new(x, mid - s.clamp(-1.0, 1.0) * amp);
            painter.line_segment([prev, cur], egui::Stroke::new(thickness, col));
            prev = cur;
        }
    }

    // Draw current frame at full brightness
    let n = buf.len();
    if n < 2 {
        return;
    }
    let wave_col = huth_color.unwrap_or(theme::CHALK);
    let mut prev = egui::Pos2::new(rect.min.x, mid - buf[0].clamp(-1.0, 1.0) * amp);
    for (i, &s) in buf.iter().enumerate().skip(1) {
        let x = rect.min.x + (i as f32 / (n - 1) as f32) * w;
        let cur = egui::Pos2::new(x, mid - s.clamp(-1.0, 1.0) * amp);
        painter.line_segment([prev, cur], egui::Stroke::new(1.5, wave_col));
        prev = cur;
    }
}

/// Detect dominant frequency from a waveform buffer via zero-crossing rate.
/// Returns the estimated MIDI note number, or None if signal is too quiet.
pub fn detect_note(buf: &[f32], sample_rate: f32) -> Option<u8> {
    if buf.len() < 64 {
        return None;
    }
    // RMS check — skip if signal is too quiet
    let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
    if rms < 0.01 {
        return None;
    }
    // Zero-crossing count
    let mut crossings = 0u32;
    for w in buf.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            crossings += 1;
        }
    }
    // Each full cycle has 2 crossings
    let duration_s = buf.len() as f32 / sample_rate;
    let freq = crossings as f32 / (2.0 * duration_s);
    if !(20.0..=8000.0).contains(&freq) {
        return None;
    }
    // Frequency to MIDI note: note = 12 * log2(freq / 440) + 69
    let midi = (12.0 * (freq / 440.0).log2() + 69.0).round() as i32;
    Some(midi.clamp(0, 127) as u8)
}

/// Draw a right-aligned DSP load sparkline + percentage label.
pub fn draw_dsp_sparkline(ui: &mut egui::Ui, buf: &[f32]) {
    if buf.is_empty() {
        return;
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let avg = buf.iter().sum::<f32>() / buf.len() as f32;
        let peak = buf.iter().cloned().fold(0.0_f32, f32::max);
        let col = if peak > 0.8 {
            egui::Color32::from_gray(160)
        } else {
            theme::ASH
        };
        ui.label(
            egui::RichText::new(format!("DSP {:.0}%", avg * 100.0))
                .color(col)
                .monospace()
                .size(9.0),
        );
        // Sparkline: up to 64 bars × 12px high
        let bar_w = 1.5_f32;
        let h = 12.0_f32;
        let n = buf.len();
        let total_w = n as f32 * bar_w;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, h), egui::Sense::hover());
        let p = ui.painter();
        for (i, &load) in buf.iter().enumerate() {
            let x = rect.right() - (n - i) as f32 * bar_w;
            let bar_h = (load.clamp(0.0, 1.0) * h).max(1.0);
            let bar_col = if load > 0.8 {
                egui::Color32::from_gray(160)
            } else if load > 0.5 {
                theme::SMOKE
            } else {
                egui::Color32::from_gray(45)
            };
            p.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(x, rect.max.y - bar_h),
                    egui::vec2(bar_w - 0.5, bar_h),
                ),
                0.0,
                bar_col,
            );
        }
    });
}

/// Draw mode indicators (Zoom/Lock/Flip) + MIDI status in the footer strip.
/// Double-click an indicator to lock that mode on without holding the key.
pub fn draw_footer_status(
    ui: &mut egui::Ui,
    midi_port: &Option<String>,
    dsp_buf: &[f32],
    rack_flipped: &mut bool,
    ctrl_locked: &mut bool,
    alt_locked: &mut bool,
    n_modules: usize,
    n_agents: usize,
    n_cables: usize,
    uptime_secs: u64,
) {
    ui.horizontal(|ui| {
        let ctrl_key = ui.input(|i| i.modifiers.ctrl);
        let alt_key = ui.input(|i| i.modifiers.alt);
        let ctrl = ctrl_key || *ctrl_locked;
        let alt = alt_key || *alt_locked;
        let c = |on, locked| {
            if locked {
                super::theme::CHALK
            } else if on {
                egui::Color32::from_gray(180)
            } else {
                super::theme::IRON
            }
        };
        let ctrl_resp = ui.add(
            egui::Label::new(
                egui::RichText::new(if *ctrl_locked { "Ctrl●" } else { "Ctrl" })
                    .monospace()
                    .size(8.0)
                    .color(c(ctrl, *ctrl_locked)),
            )
            .sense(egui::Sense::click()),
        );
        if ctrl_resp.double_clicked() {
            *ctrl_locked = !*ctrl_locked;
        }
        ctrl_resp.on_hover_text("Ctrl + scroll: zoom. Double-click to lock.");
        let alt_resp = ui.add(
            egui::Label::new(
                egui::RichText::new(if *alt_locked { "Alt●" } else { "Alt" })
                    .monospace()
                    .size(8.0)
                    .color(c(alt, *alt_locked)),
            )
            .sense(egui::Sense::click()),
        );
        if alt_resp.double_clicked() {
            *alt_locked = !*alt_locked;
        }
        alt_resp.on_hover_text("Alt + click knob: lock mode. Double-click to lock.");
        let tab_resp = ui.add(
            egui::Label::new(
                egui::RichText::new(if *rack_flipped { "Tab:BACK" } else { "Tab" })
                    .monospace()
                    .size(8.0)
                    .color(c(*rack_flipped, *rack_flipped)),
            )
            .sense(egui::Sense::click()),
        );
        if tab_resp.double_clicked() {
            *rack_flipped = !*rack_flipped;
        }
        tab_resp.on_hover_text("Tab: flip rack. Double-click to toggle.");
        ui.separator();
        let midi_text = match midi_port {
            Some(port) => format!("MIDI: {}", port.trim()),
            None => "MIDI: no device".to_string(),
        };
        ui.label(
            egui::RichText::new(midi_text)
                .color(super::theme::ASH)
                .monospace()
                .size(9.0),
        );
        draw_dsp_sparkline(ui, dsp_buf);

        ui.separator();

        // ── Session stats ──
        let mins = uptime_secs / 60;
        let secs = uptime_secs % 60;
        ui.label(
            egui::RichText::new(format!(
                "Modules: {}  Agents: {}  Cables: {}  Session: {}:{:02}",
                n_modules, n_agents, n_cables, mins, secs
            ))
            .monospace()
            .size(8.0)
            .color(super::theme::ASH),
        );

        // ── GitHub (right-justified) ──
        let right_rect = ui.available_rect_before_wrap();
        ui.allocate_ui_at_rect(right_rect, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("GitHub ↗")
                                .monospace()
                                .size(8.0)
                                .color(super::theme::FOG),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_text("github.com/Tok/impulse-instruct")
                    .clicked()
                {
                    let _ = super::webbrowser_open("https://github.com/Tok/impulse-instruct");
                }
            });
        });
    });
}

/// Keyboard shortcuts help overlay. Returns true if close was clicked.
pub fn draw_shortcuts_overlay(ctx: &egui::Context) -> bool {
    let mut close = false;
    egui::Area::new(egui::Id::new("shortcuts_overlay"))
        .fixed_pos(egui::pos2(40.0, 40.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(egui::Color32::from_rgba_premultiplied(18, 18, 18, 240))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::same(16.0))
                .show(ui, |ui| {
                    ui.heading(
                        egui::RichText::new("Keyboard Shortcuts")
                            .color(egui::Color32::from_gray(200))
                            .monospace(),
                    );
                    ui.add_space(8.0);
                    let row = |ui: &mut egui::Ui, key: &str, desc: &str| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{:<16}", key))
                                    .monospace()
                                    .color(egui::Color32::from_gray(180)),
                            );
                            ui.label(
                                egui::RichText::new(desc)
                                    .monospace()
                                    .color(egui::Color32::from_gray(120)),
                            );
                        });
                    };
                    row(ui, "Space", "Play / Stop sequencer");
                    row(ui, "Tab", "Flip rack (front / back)");
                    row(ui, "Ctrl+Z", "Undo");
                    row(ui, "Ctrl+Y", "Redo");
                    row(ui, "Ctrl+Scroll", "Zoom (global or per-module)");
                    row(ui, "Alt+Click", "Cycle param lock mode");
                    row(ui, "Arrow keys", "Navigate / adjust focused knob");
                    row(ui, "? / F1", "Toggle this help overlay");
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
        });
    close
}

/// CRT scan-line overlay + edge vignette.
pub fn draw_crt_overlay(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crt_overlay"),
    ));
    // Scan lines: semi-transparent dark lines at regular intervals
    let line_spacing = 6.0;
    let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(0, 0, 0, 18));
    let mut y = screen.min.y;
    while y < screen.max.y {
        painter.line_segment(
            [egui::pos2(screen.min.x, y), egui::pos2(screen.max.x, y)],
            stroke,
        );
        y += line_spacing;
    }
    // Vignette: darkened edges
    let vig_size = screen.width().min(screen.height()) * 0.15;
    let vig_alpha = 60u8;
    painter.rect_filled(
        egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), vig_size)),
        0.0,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, vig_alpha),
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(screen.min.x, screen.max.y - vig_size),
            egui::vec2(screen.width(), vig_size),
        ),
        0.0,
        egui::Color32::from_rgba_premultiplied(0, 0, 0, vig_alpha),
    );
}

/// Draw the scope buffer as a circular ring — a diagnostic/aesthetic display.
/// `buf` is the most recent scope samples; `size` is the widget diameter.
#[allow(dead_code)]
pub fn draw_ring_scope(ui: &mut egui::Ui, buf: &[f32], size: f32) {
    draw_ring_scope_colored(ui, buf, size, None);
}

/// Ring scope with optional Huth color override.
pub fn draw_ring_scope_colored(
    ui: &mut egui::Ui,
    buf: &[f32],
    size: f32,
    huth_color: Option<egui::Color32>,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) || buf.is_empty() {
        return;
    }
    let painter = ui.painter();
    let center = rect.center();
    let outer_r = size * 0.45;
    let inner_r = outer_r * 0.4;

    // Background circle
    painter.circle_stroke(center, outer_r, egui::Stroke::new(1.0, theme::SLATE));
    painter.circle_stroke(center, inner_r, egui::Stroke::new(0.5, theme::IRON));

    // Draw waveform as a polar plot
    let n = buf.len().min(256);
    let step = buf.len() / n;
    let mut points = Vec::with_capacity(n + 1);
    for i in 0..n {
        let sample = buf[i * step];
        let angle = (i as f32 / n as f32) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let r = inner_r + (outer_r - inner_r) * (0.5 + sample.clamp(-0.5, 0.5));
        points.push(egui::pos2(
            center.x + angle.cos() * r,
            center.y + angle.sin() * r,
        ));
    }
    if let Some(&first) = points.first() {
        points.push(first); // close the ring
    }
    // Phosphor glow: thicker dim layer underneath, then bright trace on top
    let trace_col = huth_color.unwrap_or(egui::Color32::from_gray(160));
    let glow_col = if huth_color.is_some() {
        egui::Color32::from_rgba_unmultiplied(trace_col.r(), trace_col.g(), trace_col.b(), 60)
    } else {
        egui::Color32::from_gray(35)
    };
    painter.add(egui::Shape::Path(egui::epaint::PathShape::line(
        points.clone(),
        egui::Stroke::new(3.0, glow_col),
    )));
    painter.add(egui::Shape::Path(egui::epaint::PathShape::line(
        points,
        egui::Stroke::new(1.5, trace_col),
    )));

    // Write-head dot: tracks the newest sample position on the ring
    let head_frac = (n.saturating_sub(1)) as f32 / n as f32;
    let head_angle = head_frac * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let head_pos = egui::pos2(
        center.x + head_angle.cos() * (outer_r + 3.0),
        center.y + head_angle.sin() * (outer_r + 3.0),
    );
    painter.circle_filled(head_pos, 1.5, egui::Color32::from_gray(200));
}
