// ─── ui/panels/stereo_meter.rs ────────────────────────────────────────────────
// Stereo correlation meter: phase correlation bar + L/R balance indicator.

use crate::ui::{ImpulseApp, theme};

const METER_H: f32 = 14.0;

pub fn draw_stereo_meter(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let corr = app.stereo_corr;
    let bal = app.stereo_balance;

    // ── Phase correlation bar (-1 .. +1) ─────────────────────────────────
    ui.label(
        egui::RichText::new("PHASE CORRELATION")
            .color(theme::SMOKE)
            .monospace()
            .size(8.0),
    );

    let avail_w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(avail_w, METER_H), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            egui::Rounding::same(2.0),
            egui::Color32::from_gray(10),
        );

        // Center line (0 = uncorrelated)
        let cx = rect.center().x;
        painter.line_segment(
            [
                egui::Pos2::new(cx, rect.top()),
                egui::Pos2::new(cx, rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
        );

        // Scale marks at -1, -0.5, 0, +0.5, +1
        for &mark in &[-1.0_f32, -0.5, 0.0, 0.5, 1.0] {
            let x = rect.left() + (mark + 1.0) * 0.5 * rect.width();
            painter.line_segment(
                [
                    egui::Pos2::new(x, rect.bottom() - 2.0),
                    egui::Pos2::new(x, rect.bottom()),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
            );
        }

        // Correlation indicator: filled bar from center
        // corr > 0 → bar extends right (mono-ish), corr < 0 → extends left (out of phase)
        let norm = (corr + 1.0) * 0.5; // 0..1 where 0.5 = center
        let bar_x = rect.left() + norm * rect.width();
        let bar_rect = if bar_x >= cx {
            egui::Rect::from_min_max(
                egui::Pos2::new(cx, rect.top() + 2.0),
                egui::Pos2::new(bar_x, rect.bottom() - 2.0),
            )
        } else {
            egui::Rect::from_min_max(
                egui::Pos2::new(bar_x, rect.top() + 2.0),
                egui::Pos2::new(cx, rect.bottom() - 2.0),
            )
        };
        // Brightness: bright when correlated (good), dim when out of phase (bad)
        let g = (80.0 + (corr + 1.0) * 60.0) as u8;
        painter.rect_filled(bar_rect, egui::Rounding::ZERO, egui::Color32::from_gray(g));

        // Labels
        painter.text(
            egui::Pos2::new(rect.left() + 3.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "-1",
            egui::FontId::monospace(7.0),
            egui::Color32::from_gray(50),
        );
        painter.text(
            egui::Pos2::new(rect.right() - 3.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "+1",
            egui::FontId::monospace(7.0),
            egui::Color32::from_gray(50),
        );
    }

    // Numeric readout
    ui.label(
        egui::RichText::new(format!("corr: {:+.2}", corr))
            .color(theme::FOG)
            .monospace()
            .size(8.0),
    );

    ui.add_space(4.0);

    // ── L/R Balance bar ──────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("L/R BALANCE")
            .color(theme::SMOKE)
            .monospace()
            .size(8.0),
    );

    let (bal_rect, _) =
        ui.allocate_exact_size(egui::Vec2::new(avail_w, METER_H), egui::Sense::hover());

    if ui.is_rect_visible(bal_rect) {
        let painter = ui.painter_at(bal_rect);
        painter.rect_filled(
            bal_rect,
            egui::Rounding::same(2.0),
            egui::Color32::from_gray(10),
        );

        // Center line
        let cx = bal_rect.center().x;
        painter.line_segment(
            [
                egui::Pos2::new(cx, bal_rect.top()),
                egui::Pos2::new(cx, bal_rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
        );

        // Balance indicator: small bar from center
        let bal_norm = (bal + 1.0) * 0.5;
        let bx = bal_rect.left() + bal_norm * bal_rect.width();
        let indicator = if bx >= cx {
            egui::Rect::from_min_max(
                egui::Pos2::new(cx, bal_rect.top() + 2.0),
                egui::Pos2::new(bx, bal_rect.bottom() - 2.0),
            )
        } else {
            egui::Rect::from_min_max(
                egui::Pos2::new(bx, bal_rect.top() + 2.0),
                egui::Pos2::new(cx, bal_rect.bottom() - 2.0),
            )
        };
        painter.rect_filled(
            indicator,
            egui::Rounding::ZERO,
            egui::Color32::from_gray(120),
        );

        // L / R labels
        painter.text(
            egui::Pos2::new(bal_rect.left() + 3.0, bal_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "L",
            egui::FontId::monospace(7.0),
            egui::Color32::from_gray(50),
        );
        painter.text(
            egui::Pos2::new(bal_rect.right() - 3.0, bal_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "R",
            egui::FontId::monospace(7.0),
            egui::Color32::from_gray(50),
        );
    }

    ui.label(
        egui::RichText::new(format!(
            "bal: {:+.2} ({})",
            bal,
            if bal.abs() < 0.05 {
                "center"
            } else if bal < 0.0 {
                "left"
            } else {
                "right"
            }
        ))
        .color(theme::FOG)
        .monospace()
        .size(8.0),
    );
}
