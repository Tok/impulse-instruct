// ─── ui/scope_footer.rs ──────────────────────────────────────────────────────
// Oscilloscope waveform display and DSP load sparkline — extracted from mod.rs
// to stay under the 1000-line limit.

use crate::ui::theme;

/// Draw oscilloscope waveform from the scope sample buffer.
pub fn draw_scope(ui: &mut egui::Ui, buf: &[f32]) {
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

    let n = buf.len();
    if n < 2 {
        return;
    }
    let w = rect.width();
    let mid = rect.center().y;
    let amp = rect.height() * 0.45;

    let points: Vec<egui::Pos2> = buf
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let x = rect.min.x + (i as f32 / (n - 1) as f32) * w;
            let y = mid - s.clamp(-1.0, 1.0) * amp;
            egui::Pos2::new(x, y)
        })
        .collect();

    for i in 0..points.len().saturating_sub(1) {
        painter.line_segment(
            [points[i], points[i + 1]],
            egui::Stroke::new(1.0, theme::CHALK),
        );
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
