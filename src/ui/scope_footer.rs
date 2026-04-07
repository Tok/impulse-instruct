// ─── ui/scope_footer.rs ──────────────────────────────────────────────────────
// Oscilloscope waveform display and DSP load sparkline — extracted from mod.rs
// to stay under the 1000-line limit.

use crate::ui::theme;

/// Draw oscilloscope waveform with phosphor persistence (older frames fade).
pub fn draw_scope(ui: &mut egui::Ui, buf: &[f32], history: &std::collections::VecDeque<Vec<f32>>) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
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
        // Older frames are dimmer: brightness ramps from ~20 to ~60
        let brightness = 20 + (age as u32 * 40 / hist_len.max(1) as u32).min(60);
        let col = egui::Color32::from_gray(brightness as u8);
        let mut prev = egui::Pos2::new(rect.min.x, mid - frame[0].clamp(-1.0, 1.0) * amp);
        for (i, &s) in frame.iter().enumerate().skip(1) {
            let x = rect.min.x + (i as f32 / (n - 1) as f32) * w;
            let cur = egui::Pos2::new(x, mid - s.clamp(-1.0, 1.0) * amp);
            painter.line_segment([prev, cur], egui::Stroke::new(1.0, col));
            prev = cur;
        }
    }

    // Draw current frame at full brightness
    let n = buf.len();
    if n < 2 {
        return;
    }
    let mut prev = egui::Pos2::new(rect.min.x, mid - buf[0].clamp(-1.0, 1.0) * amp);
    for (i, &s) in buf.iter().enumerate().skip(1) {
        let x = rect.min.x + (i as f32 / (n - 1) as f32) * w;
        let cur = egui::Pos2::new(x, mid - s.clamp(-1.0, 1.0) * amp);
        painter.line_segment([prev, cur], egui::Stroke::new(1.0, theme::CHALK));
        prev = cur;
    }
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

/// Draw modifier key indicators (Ctrl/Alt) + MIDI status in the footer strip.
pub fn draw_footer_status(ui: &mut egui::Ui, midi_port: &Option<String>, dsp_buf: &[f32]) {
    ui.horizontal(|ui| {
        let ctrl = ui.input(|i| i.modifiers.ctrl);
        let alt = ui.input(|i| i.modifiers.alt);
        let c = |on| {
            if on {
                super::theme::CHALK
            } else {
                super::theme::IRON
            }
        };
        ui.add(
            egui::Label::new(
                egui::RichText::new("Ctrl")
                    .monospace()
                    .size(8.0)
                    .color(c(ctrl)),
            )
            .sense(egui::Sense::hover()),
        )
        .on_hover_text("Ctrl + scroll wheel: zoom (global or per-module)");
        ui.add(
            egui::Label::new(
                egui::RichText::new("Alt")
                    .monospace()
                    .size(8.0)
                    .color(c(alt)),
            )
            .sense(egui::Sense::hover()),
        )
        .on_hover_text("Alt + click knob: cycle lock mode (Free / User / Focus)");
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
    });
}
