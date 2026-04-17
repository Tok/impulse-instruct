// ─── ui/widgets/step.rs ───────────────────────────────────────────────────────
// Sequencer step widgets: standard step button + Huth Farbige Noten U-cup cell.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::ui::theme;

// ─── Step Button ─────────────────────────────────────────────────────────────

/// A sequencer step button with neumorphic raised/pressed chrome style.
/// Returns true when clicked (toggle request).
/// `vel` tints the fill when active (0 = dim, 1 = full bright).
/// `dot_color`: when Some, the button body stays neutral and only a small
///   coloured dot is drawn — used for bass steps so the palette stays subtle.
/// `note_label`: when Some, a tiny note name (e.g. "C4") is drawn in
///   `dot_color` inside the cell, above the dot. Ignored for drum rows.
/// `size_px` comes from `UiPrefs.pad_size.px()`.
/// `probability` — 0.0–1.0, shown as a dim ring when < 1.0 to indicate uncertainty.
pub fn step_button(
    ui: &mut Ui,
    active: bool,
    current: bool,
    vel: f32,
    probability: f32,
    dot_color: Option<Color32>,
    note_label: Option<&str>,
    size_px: f32,
) -> bool {
    let sz = Vec2::splat(size_px);
    let (rect, response) = ui.allocate_exact_size(sz, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // Generous rounding — no hard square corners visible
        let r = egui::Rounding::same((size_px * 0.22).max(4.0));
        let inner = rect.shrink(1.5);

        if active {
            // Debossed / pressed look:
            // Dark inset shadow frame, slightly brighter outer ring, deep inner well.
            painter.rect_filled(inner, r, theme::DEEP);
            // Bright bottom-right rim (light bounces off raised surround)
            painter.rect_stroke(inner.shrink(0.5), r, Stroke::new(1.0, theme::IRON));
            // Dark top-left rim (pressed inward — shadow side)
            painter.rect_stroke(inner, r, Stroke::new(1.0, theme::VOID));
            // Inset well
            let inset = inner.shrink(2.5);
            painter.rect_filled(inset, r, Color32::from_gray(22));

            if let Some(col) = dot_color {
                let dot_r = (size_px * 0.18).max(2.5);
                let dot_pos = Pos2::new(inset.center().x, inset.max.y - dot_r - 1.0);
                theme::led(painter, dot_pos, dot_r, col, 1.0);
                // Note name label above the dot (only when cell is large enough)
                if let Some(label) = note_label
                    && size_px >= 26.0
                {
                    let font_sz = (size_px * 0.22).clamp(7.0, 10.0);
                    let text_pos = Pos2::new(inset.center().x, inset.min.y + font_sz * 0.5 + 1.0);
                    painter.text(
                        text_pos,
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::monospace(font_sz),
                        col,
                    );
                }
            } else {
                // Velocity heat fill — brighter = louder
                let dim = 0.35_f32 + vel * 0.65;
                let g = (200.0 * dim) as u8;
                painter.rect_filled(inset, r, Color32::from_rgba_unmultiplied(g, g, g, 70));
            }
            // Probability indicator: small corner dot when < 100%
            if probability < 0.99 {
                let prob_g = (40.0 + probability * 80.0) as u8;
                let dot_r = (size_px * 0.08).max(1.5);
                let pos = Pos2::new(inset.right() - dot_r - 1.0, inset.top() + dot_r + 1.0);
                painter.circle_filled(pos, dot_r, Color32::from_gray(prob_g));
            }
        } else {
            // Raised look:
            // Bright top-left rim, dark bottom-right — classic emboss.
            painter.rect_filled(inner, r, Color32::from_gray(32));
            // Top-left highlight
            painter.rect_stroke(inner, r, Stroke::new(1.0, Color32::from_gray(58)));
            // Bottom-right shadow (slightly inset to overlay only bottom/right)
            painter.rect_stroke(
                inner.shrink(0.5),
                r,
                Stroke::new(1.0, Color32::from_gray(10)),
            );

            if let Some(col) = dot_color {
                let dot_r = (size_px * 0.14).max(2.0);
                let dot_pos = Pos2::new(inner.center().x, inner.max.y - dot_r - 2.0);
                // Dim LED — half-intensity so inactive steps still feel "on standby"
                theme::led(painter, dot_pos, dot_r, col, 0.45);
            }
        }

        // Current-step cursor: outer bloom glow + bright border + inner ring
        if current {
            // Outer bloom halos
            for i in 1..=3u8 {
                let expand = i as f32 * 1.5;
                let alpha = 40u8.saturating_sub(i * 12);
                painter.rect_filled(
                    rect.expand(expand),
                    r,
                    Color32::from_rgba_unmultiplied(220, 220, 220, alpha),
                );
            }
            // Bright outer border
            painter.rect_stroke(rect.shrink(0.5), r, Stroke::new(1.5, theme::CHALK));
            // Subtle inner ring — reinforces the "lit up" face
            painter.rect_stroke(
                inner.shrink(1.5),
                r,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(200, 200, 200, 45)),
            );
        }
    }

    response.clicked()
}
