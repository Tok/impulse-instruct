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
/// `note_label`: when Some, a tiny note name (e.g. "C4") is drawn in
///   `dot_color` inside the cell, above the dot. Ignored for drum rows.
/// `size_px` comes from `UiPrefs.pad_size.px()`.
pub fn step_button(
    ui: &mut Ui,
    active: bool,
    current: bool,
    vel: f32,
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
            painter.rect_filled(inner, r, Color32::from_gray(18));
            // Bright bottom-right rim (light bounces off raised surround)
            painter.rect_stroke(
                inner.shrink(0.5),
                r,
                Stroke::new(1.0, Color32::from_gray(60)),
            );
            // Dark top-left rim (pressed inward — shadow side)
            painter.rect_stroke(inner, r, Stroke::new(1.0, Color32::from_gray(8)));
            // Inset well
            let inset = inner.shrink(2.5);
            painter.rect_filled(inset, r, Color32::from_gray(22));

            if let Some(col) = dot_color {
                let dot_r = (size_px * 0.18).max(2.5);
                let dot_pos = Pos2::new(inset.center().x, inset.max.y - dot_r - 1.0);
                painter.circle_filled(dot_pos, dot_r, col);
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
                let dim = 0.35_f32 + vel * 0.65;
                let g = (200.0 * dim) as u8;
                painter.rect_filled(inset, r, Color32::from_rgba_unmultiplied(g, g, g, 70));
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

        // Note name label — top-center of cell, over the Huth fill
        if active && size_px >= 26.0 {
            let font_sz = (size_px * 0.22).clamp(7.0, 10.0);
            let text_pos = Pos2::new(rect.center().x, rect.min.y + font_sz * 0.5 + 2.0);
            let label = crate::ui::note_name(note);
            painter.text(
                text_pos,
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(font_sz),
                Color32::from_white_alpha(200),
            );
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
