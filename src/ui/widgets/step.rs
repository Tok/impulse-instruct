// ─── ui/widgets/step.rs ───────────────────────────────────────────────────────
// Sequencer step widgets: standard step button + Huth Farbige Noten U-cup cell.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::ui::theme;

// ─── Step Button ─────────────────────────────────────────────────────────────

/// A sequencer step button with neumorphic raised/pressed chrome style.
/// Returns true when clicked (toggle request).
/// `vel` tints the fill when active (0 = dim, 1 = full bright).
/// `dot_color`: when Some, the button body stays neutral and only a small
///   coloured dot is drawn — used for bass steps so the palette stays subtle.
/// `size_px` comes from `UiPrefs.pad_size.px()`.
pub fn step_button(
    ui: &mut Ui,
    active: bool,
    current: bool,
    vel: f32,
    dot_color: Option<Color32>,
    size_px: f32,
) -> bool {
    let sz = Vec2::splat(size_px);
    let (rect, response) = ui.allocate_exact_size(sz, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let r = egui::Rounding::same((size_px * 0.12).max(2.0));
        let inner = rect.shrink(1.0);

        if active {
            // Pressed / debossed look
            // Outer fill — slightly brighter than the inset so the depth reads
            painter.rect_filled(inner, r, Color32::from_gray(52));
            // Inverted edge highlights: dark top-left (pressed in), bright bottom-right
            painter.line_segment(
                [inner.left_top(), inner.right_top()],
                Stroke::new(1.0, Color32::from_gray(8)),
            );
            painter.line_segment(
                [inner.left_top(), inner.left_bottom()],
                Stroke::new(1.0, Color32::from_gray(8)),
            );
            painter.line_segment(
                [inner.left_bottom(), inner.right_bottom()],
                Stroke::new(1.0, Color32::from_gray(80)),
            );
            painter.line_segment(
                [inner.right_top(), inner.right_bottom()],
                Stroke::new(1.0, Color32::from_gray(80)),
            );
            // Inset shadow rect — the debossed inner well
            let inset = inner.shrink(2.0);
            painter.rect_filled(inset, r, Color32::from_gray(22));

            // Dot or full-fill indicator (drawn over the inset well)
            if let Some(col) = dot_color {
                // Small coloured dot — subtle note-pitch indicator
                let dot_r = (size_px * 0.18).max(2.5);
                let dot_pos = Pos2::new(inset.center().x, inset.max.y - dot_r - 1.0);
                painter.circle_filled(dot_pos, dot_r, col);
            } else {
                // Drum mode: velocity bloom over the inset well
                let dim = 0.35_f32 + vel * 0.65;
                let g = (200.0 * dim) as u8;
                painter.rect_filled(inset, r, Color32::from_rgba_unmultiplied(g, g, g, 70));
            }
        } else {
            // Raised look
            painter.rect_filled(inner, r, Color32::from_gray(30));
            painter.line_segment(
                [inner.left_top(), inner.right_top()],
                Stroke::new(1.0, Color32::from_gray(62)),
            );
            painter.line_segment(
                [inner.left_top(), inner.left_bottom()],
                Stroke::new(1.0, Color32::from_gray(62)),
            );
            painter.line_segment(
                [inner.left_bottom(), inner.right_bottom()],
                Stroke::new(1.0, Color32::from_gray(10)),
            );
            painter.line_segment(
                [inner.right_top(), inner.right_bottom()],
                Stroke::new(1.0, Color32::from_gray(10)),
            );

            // Inactive bass step with assigned note: faint dot
            if let Some(col) = dot_color {
                let dot_r = (size_px * 0.14).max(2.0);
                let dot_pos = Pos2::new(inner.center().x, inner.max.y - dot_r - 2.0);
                painter.circle_filled(
                    dot_pos,
                    dot_r,
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 80),
                );
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

// ─── Huth Note Cell ──────────────────────────────────────────────────────────
//
// Renders a single sequencer step as a Huth *Farbige Noten* U-cup:
//
//   ┌─────┐  ← open top (unused / background)
//   │█████│
//   │█████│  ← colored fill rising from the bottom; height = gate fraction
//   └──┬──┘
//      └── rounded bottom corners only
//
// White-key notes (natural C D E F G A B): solid Huth color fill.
// Black-key notes (sharps/flats C# D# F# G# A#): Huth color with a white
//   inner rectangle — the "double U" Huth used for semitones.
//
// `gate` 0–1 controls fill height (0.3 minimum so short notes stay visible).
// Returns true when clicked.

pub fn huth_note_cell(
    ui: &mut Ui,
    note: u8,
    gate: f32,
    active: bool,
    current: bool,
    size_px: f32,
) -> bool {
    let sz = Vec2::splat(size_px);
    let (rect, response) = ui.allocate_exact_size(sz, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // Background — same as inactive step
        let bg = Color32::from_gray(22);
        painter.rect_filled(rect.shrink(1.0), egui::Rounding::same(2.0), bg);

        if active {
            let huth = theme::note_color(note);
            let pitch_class = note % 12;
            // Black keys: C#=1, D#=3, F#=6, G#=8, A#=10
            let is_black_key = matches!(pitch_class, 1 | 3 | 6 | 8 | 10);

            // Fill height rises from the bottom; gate clamped to 0.3–1.0 range
            let fill_frac = gate.clamp(0.3, 1.0);
            let cell_h = rect.height() - 2.0;
            let fill_h = cell_h * fill_frac;
            let fill_top = rect.max.y - 1.0 - fill_h;

            let fill_rect = Rect::from_min_max(
                Pos2::new(rect.min.x + 1.0, fill_top),
                Pos2::new(rect.max.x - 1.0, rect.max.y - 1.0),
            );
            let rounding = egui::Rounding {
                nw: 0.0,
                ne: 0.0,
                sw: (size_px * 0.22).max(3.0),
                se: (size_px * 0.22).max(3.0),
            };
            painter.rect_filled(fill_rect, rounding, huth);

            // White inner shape for black-key (semitone) notes
            if is_black_key {
                let inset = (size_px * 0.18).max(2.5);
                let inner_h = (fill_h - inset * 1.5).max(0.0);
                if inner_h > 2.0 {
                    let inner = Rect::from_min_max(
                        Pos2::new(
                            fill_rect.min.x + inset,
                            fill_rect.max.y - inner_h - inset * 0.5,
                        ),
                        Pos2::new(fill_rect.max.x - inset, fill_rect.max.y - inset * 0.5),
                    );
                    let inner_r = egui::Rounding {
                        nw: 0.0,
                        ne: 0.0,
                        sw: (size_px * 0.14).max(2.0),
                        se: (size_px * 0.14).max(2.0),
                    };
                    painter.rect_filled(
                        inner,
                        inner_r,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 200),
                    );
                }
            }
        }

        // Current-step cursor — outer bloom + bright border + inner ring
        if current {
            let r = egui::Rounding::same(2.0);
            for i in 1..=3u8 {
                let expand = i as f32 * 1.5;
                let alpha = 40u8.saturating_sub(i * 12);
                painter.rect_filled(
                    rect.expand(expand),
                    r,
                    Color32::from_rgba_unmultiplied(220, 220, 220, alpha),
                );
            }
            painter.rect_stroke(rect.shrink(0.5), r, Stroke::new(1.5, theme::CHALK));
            painter.rect_stroke(
                rect.shrink(2.5),
                r,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(200, 200, 200, 45)),
            );
        }
    }

    response.clicked()
}
