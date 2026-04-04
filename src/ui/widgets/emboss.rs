// ─── ui/widgets/emboss.rs ────────────────────────────────────────────────────
// Neumorphic embossed button widget.

use egui::{Color32, Sense, Stroke, Ui, Vec2};

use crate::ui::theme;

/// Neumorphic raised/pressed toggle button.
/// Inactive = raised (bright top-left, dark bottom-right edge).
/// Active = pressed (dark top-left, bright bottom-right edge, darker fill).
pub fn button_emboss(ui: &mut Ui, label: &str, active: &mut bool) -> bool {
    let size = Vec2::new(44.0, 20.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let rounding = egui::Rounding::same(3.0);

        let (fill, top_col, bot_col, text_col) = if *active {
            (
                Color32::from_gray(20),
                Color32::from_gray(10),
                Color32::from_gray(70),
                theme::FOG,
            )
        } else {
            (
                Color32::from_gray(38),
                Color32::from_gray(75),
                Color32::from_gray(12),
                theme::SMOKE,
            )
        };

        painter.rect_filled(rect, rounding, fill);

        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(1.0, top_col),
        );
        painter.line_segment(
            [rect.left_top(), rect.left_bottom()],
            Stroke::new(1.0, top_col),
        );
        painter.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, bot_col),
        );
        painter.line_segment(
            [rect.right_top(), rect.right_bottom()],
            Stroke::new(1.0, bot_col),
        );

        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(9.0),
            text_col,
        );
    }

    if response.clicked() {
        *active = !*active;
        return true;
    }
    false
}
