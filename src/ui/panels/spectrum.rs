// ─── ui/panels/spectrum.rs ────────────────────────────────────────────────────
// Spectrum analyser display — real-time FFT magnitude bars.

use crate::audio::SAMPLE_RATE;
use crate::ui::{ImpulseApp, theme};

/// Number of logarithmic display bands (20 Hz – 20 kHz).
const NUM_BANDS: usize = 64;
/// Display height for the spectrum bars.
const DISPLAY_H: f32 = 100.0;
/// Minimum dB shown (floor).
const DB_FLOOR: f32 = -96.0;
/// Maximum dB shown (ceiling).
const DB_CEIL: f32 = 0.0;

pub fn draw_spectrum(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Controls row
    ui.horizontal(|ui| {
        let mut smoothing = app.state.read().spectrum.smoothing;
        ui.label(
            egui::RichText::new("SMOOTH")
                .color(theme::SMOKE)
                .monospace()
                .size(8.0),
        );
        if ui
            .add(
                egui::DragValue::new(&mut smoothing)
                    .range(0.0..=0.98)
                    .speed(0.01)
                    .fixed_decimals(2),
            )
            .changed()
        {
            app.state.write().spectrum.smoothing = smoothing;
        }

        let mut peak_hold = app.state.read().spectrum.peak_hold;
        if ui
            .checkbox(
                &mut peak_hold,
                egui::RichText::new("PEAK")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(8.0),
            )
            .changed()
        {
            app.state.write().spectrum.peak_hold = peak_hold;
        }
    });

    // Compute log bands from the smoothed magnitudes
    let bands = if app.spectrum_magnitudes.is_empty() {
        vec![DB_FLOOR; NUM_BANDS]
    } else {
        crate::audio::spectrum::log_bands(
            &app.spectrum_magnitudes,
            SAMPLE_RATE / crate::audio::spectrum::FFT_SIZE as f32,
            NUM_BANDS,
        )
    };

    // Draw the bar display
    let avail_w = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::Vec2::new(avail_w, DISPLAY_H), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        // Dark background
        painter.rect_filled(rect, egui::Rounding::same(2.0), egui::Color32::from_gray(8));

        let bar_w = rect.width() / NUM_BANDS as f32;
        let db_range = DB_CEIL - DB_FLOOR;

        for (i, &db) in bands.iter().enumerate() {
            let norm = ((db - DB_FLOOR) / db_range).clamp(0.0, 1.0);
            let bar_h = norm * rect.height();
            if bar_h < 0.5 {
                continue;
            }
            let x = rect.left() + i as f32 * bar_w;
            let bar_rect = egui::Rect::from_min_size(
                egui::Pos2::new(x, rect.bottom() - bar_h),
                egui::Vec2::new((bar_w - 1.0).max(1.0), bar_h),
            );
            // Brightness ramps with amplitude: quiet = dim, loud = bright
            let g = (40.0 + norm * 160.0) as u8;
            painter.rect_filled(bar_rect, egui::Rounding::ZERO, egui::Color32::from_gray(g));
        }

        // Peak-hold markers
        if app.state.read().spectrum.peak_hold {
            for (i, &db) in app.spectrum_peaks.iter().enumerate().take(NUM_BANDS) {
                let norm = ((db - DB_FLOOR) / db_range).clamp(0.0, 1.0);
                if norm < 0.01 {
                    continue;
                }
                let x = rect.left() + i as f32 * bar_w;
                let y = rect.bottom() - norm * rect.height();
                painter.line_segment(
                    [
                        egui::Pos2::new(x, y),
                        egui::Pos2::new(x + (bar_w - 1.0).max(1.0), y),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(220)),
                );
            }
        }

        // Frequency labels at bottom
        for &(freq, label) in &[(100.0_f32, "100"), (1000.0, "1k"), (10000.0, "10k")] {
            let log_pos = (freq.ln() - 20.0_f32.ln()) / (20000.0_f32.ln() - 20.0_f32.ln());
            let x = rect.left() + log_pos * rect.width();
            painter.text(
                egui::Pos2::new(x, rect.bottom() - 8.0),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(7.0),
                egui::Color32::from_gray(60),
            );
        }
    }
}
