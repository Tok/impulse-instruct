// ─── ui/widgets.rs ───────────────────────────────────────────────────────────
#![allow(dead_code)] // widget library grows alongside panels
// Custom widgets: rotary knob, step button, LED indicator.

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::f32::consts::TAU;

use super::theme;

// ─── Rotary Knob ─────────────────────────────────────────────────────────────

/// A rotary knob widget. Returns true if the value changed.
pub fn knob(ui: &mut Ui, label: &str, value: &mut f32, locked: bool) -> bool {
    let size = 44.0_f32;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size + 14.0), Sense::click_and_drag());

    let knob_rect = Rect::from_center_size(
        rect.center() - Vec2::new(0.0, 6.0),
        Vec2::splat(size),
    );
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, knob_rect.max.y + 1.0),
        Vec2::new(rect.width(), 12.0),
    );

    let mut changed = false;

    if !locked && response.dragged() {
        let delta = response.drag_delta();
        *value = (*value - delta.y * 0.005 + delta.x * 0.003).clamp(0.0, 1.0);
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        draw_knob(painter, knob_rect, *value, locked, response.hovered());
        painter.text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(8.5),
            if locked { theme::ASH } else { theme::SMOKE },
        );
    }

    changed
}

fn draw_knob(painter: &Painter, rect: Rect, value: f32, locked: bool, hovered: bool) {
    let center = rect.center();
    let radius = rect.width() * 0.45;

    // Background circle
    let bg = if locked { theme::PIT } else if hovered { theme::SLATE } else { theme::PIT };
    painter.circle_filled(center, radius, bg);
    painter.circle_stroke(center, radius, Stroke::new(1.0, if locked { theme::IRON } else { theme::ASH }));

    // Arc track (270° sweep, starting bottom-left)
    let start_angle: f32 = std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4 * 3.0;
    let sweep = TAU * 0.75;
    let track_r = radius * 0.72;

    // Draw track ghost
    draw_arc(painter, center, track_r, start_angle, sweep, 1.0, theme::SLATE);

    // Draw filled portion
    let filled_sweep = sweep * value;
    let active_color = if locked { theme::IRON } else { theme::FOG };
    if filled_sweep > 0.01 {
        draw_arc(painter, center, track_r, start_angle, filled_sweep, 2.0, active_color);
    }

    // Pointer dot
    let end_angle = start_angle + filled_sweep;
    let dot_pos = center + Vec2::new(end_angle.cos(), end_angle.sin()) * (radius * 0.58);
    let dot_color = if locked { theme::ASH } else { theme::CHALK };
    painter.circle_filled(dot_pos, 2.5, dot_color);

    // Lock indicator
    if locked {
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "L",
            egui::FontId::monospace(8.0),
            theme::IRON,
        );
    }
}

fn draw_arc(painter: &Painter, center: Pos2, radius: f32, start: f32, sweep: f32, width: f32, color: Color32) {
    let steps = ((sweep.abs() * radius * 2.0) as usize).max(8).min(64);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect();
    for i in 0..points.len().saturating_sub(1) {
        painter.line_segment([points[i], points[i + 1]], Stroke::new(width, color));
    }
}

// ─── Horizontal Slider ───────────────────────────────────────────────────────

/// A labeled horizontal slider. Label is left-aligned in a fixed column;
/// slider fills remaining width.  Returns true if the value changed.
pub fn slider(ui: &mut Ui, label: &str, value: &mut f32, locked: bool) -> bool {
    let label_w = 72.0_f32;
    let mut changed = false;

    ui.horizontal(|ui| {
        let text_color = if locked { theme::ASH } else { theme::SMOKE };
        ui.add_sized(
            [label_w, 14.0],
            egui::Label::new(egui::RichText::new(label).monospace().size(9.0).color(text_color)),
        );

        let avail = (ui.available_width() - if locked { 18.0 } else { 0.0 }).max(40.0);
        if locked {
            // Show a non-interactive, dimmed slider
            let mut v = *value;
            ui.add_enabled(false, egui::Slider::new(&mut v, 0.0..=1.0).show_value(false));
            ui.add_sized(
                [18.0, 14.0],
                egui::Label::new(egui::RichText::new("L").monospace().size(9.0).color(theme::IRON)),
            );
        } else {
            let resp = ui.add_sized(
                [avail, 14.0],
                egui::Slider::new(value, 0.0..=1.0).show_value(false),
            );
            if resp.changed() {
                changed = true;
            }
        }
    });

    changed
}

/// Dispatch to `knob` or `slider` based on `use_sliders`.
pub fn param_control(ui: &mut Ui, label: &str, value: &mut f32, locked: bool, use_sliders: bool) -> bool {
    if use_sliders {
        slider(ui, label, value, locked)
    } else {
        knob(ui, label, value, locked)
    }
}

// ─── Step Button ──────────────────────────────────────────────────────────────

/// A step sequencer button. Returns true if clicked (toggle).
/// `note_color` — if Some, the active-step dot is tinted with that color (Huth palette).
pub fn step_button(
    ui: &mut Ui,
    active: bool,
    is_current: bool,
    velocity: f32,
    note_color: Option<Color32>,
) -> bool {
    let size = Vec2::new(28.0, 22.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Background — active color takes priority; cursor shows via the border only,
        // with a faint warm tint on inactive steps so the position is visible without
        // the step turning white.
        let bg = if active {
            theme::lerp_gray(45, 160, velocity)
        } else if is_current {
            theme::lerp_gray(28, 55, 0.5) // dim mid-gray cursor hint
        } else {
            theme::PIT
        };

        painter.rect_filled(rect.shrink(1.0), egui::Rounding::same(2.0), bg);

        // Border
        let border = if response.hovered() { theme::FOG }
            else if is_current { theme::CHALK }
            else if active { theme::ASH }
            else { theme::SLATE };
        painter.rect_stroke(rect.shrink(1.0), egui::Rounding::same(2.0), Stroke::new(1.0, border));

        // Active dot — use Huth note color when provided, otherwise plain CHALK
        if active && !is_current {
            let dot_y = rect.center().y + 3.0;
            let dot_col = note_color.unwrap_or(theme::CHALK);
            painter.circle_filled(
                Pos2::new(rect.center().x, dot_y),
                2.0,
                dot_col,
            );
        }
    }

    response.clicked()
}

// ─── LED indicator ────────────────────────────────────────────────────────────

pub fn led(ui: &mut Ui, on: bool, label: &str) {
    let size = Vec2::new(8.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if ui.is_rect_visible(rect) {
        let color = if on { theme::CHALK } else { theme::SLATE };
        ui.painter().circle_filled(rect.center(), 3.5, color);
    }
    ui.label(egui::RichText::new(label).color(theme::SMOKE).size(9.0));
}

// ─── Section header bar ───────────────────────────────────────────────────────

pub fn section_header(ui: &mut Ui, title: &str) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        let label = egui::RichText::new(title)
            .color(theme::FOG)
            .size(10.5)
            .monospace();
        ui.label(label);
    });
    ui.add_space(3.0);
}

// ─── Small toggle button ──────────────────────────────────────────────────────

// ─── XY Control Square ────────────────────────────────────────────────────────
//
// A 2D parameter pad inspired by the Ableton Learning Synths playground.
// Drag anywhere in the square to simultaneously control two parameters.
//
// Layout:
//   ┌─────────────────────┐
//   │  Y_MAX              │  ← Y label (rotated text drawn manually)
//   │         ·           │
//   │   [cursor dot]      │
//   │                     │
//   │  Y_MIN              │
//   └─────────────────────┘
//     X_MIN           X_MAX
//     ←── label_x  ──→
//
// Usage in a panel:
//   if xy_pad(ui, "CUTOFF", "RESO", &mut cutoff, &mut resonance, 100.0, locked) {
//       state.bass.cutoff = cutoff; state.bass.resonance = resonance;
//   }

/// 2D XY control pad. Returns true if either value changed.
///
/// `size` — side length of the square in logical pixels (e.g. 100.0).
/// X increases left→right, Y increases bottom→top (audio convention).
pub fn xy_pad(
    ui: &mut Ui,
    label_x: &str,
    label_y: &str,
    x: &mut f32,
    y: &mut f32,
    size: f32,
    locked: bool,
) -> bool {
    // Total allocation: square + label row below + label col left
    let label_h = 13.0_f32;
    let label_w = 12.0_f32; // left column for rotated Y label
    let total = Vec2::new(label_w + size + 2.0, size + label_h + 2.0);
    let (outer, response) = ui.allocate_exact_size(total, Sense::click_and_drag());

    let pad_rect = Rect::from_min_size(
        Pos2::new(outer.min.x + label_w, outer.min.y),
        Vec2::splat(size),
    );

    let mut changed = false;

    if !locked {
        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                *x = ((pos.x - pad_rect.min.x) / pad_rect.width()).clamp(0.0, 1.0);
                // Y is inverted: top = 1.0, bottom = 0.0
                *y = (1.0 - (pos.y - pad_rect.min.y) / pad_rect.height()).clamp(0.0, 1.0);
                changed = true;
            }
        }
    }

    if ui.is_rect_visible(outer) {
        let painter = ui.painter();

        // Background
        let bg_col = if locked { theme::PIT } else if response.hovered() { theme::SLATE } else { theme::PIT };
        painter.rect_filled(pad_rect, egui::Rounding::same(2.0), bg_col);
        painter.rect_stroke(
            pad_rect,
            egui::Rounding::same(2.0),
            Stroke::new(1.0, if locked { theme::IRON } else { theme::ASH }),
        );

        // Subtle grid lines at 25%, 50%, 75%
        let grid_col = theme::SLATE;
        for t in [0.25_f32, 0.5, 0.75] {
            let gx = pad_rect.min.x + pad_rect.width() * t;
            let gy = pad_rect.min.y + pad_rect.height() * t;
            painter.line_segment(
                [Pos2::new(gx, pad_rect.min.y), Pos2::new(gx, pad_rect.max.y)],
                Stroke::new(0.5, grid_col),
            );
            painter.line_segment(
                [Pos2::new(pad_rect.min.x, gy), Pos2::new(pad_rect.max.x, gy)],
                Stroke::new(0.5, grid_col),
            );
        }

        // Crosshair lines from cursor to edges (dim guide lines)
        let cx = pad_rect.min.x + pad_rect.width() * x.clamp(0.0, 1.0);
        let cy = pad_rect.min.y + pad_rect.height() * (1.0 - y.clamp(0.0, 1.0));
        let guide_col = if locked { theme::IRON } else { theme::IRON };
        painter.line_segment(
            [Pos2::new(cx, pad_rect.min.y), Pos2::new(cx, pad_rect.max.y)],
            Stroke::new(0.5, guide_col),
        );
        painter.line_segment(
            [Pos2::new(pad_rect.min.x, cy), Pos2::new(pad_rect.max.x, cy)],
            Stroke::new(0.5, guide_col),
        );

        // Cursor dot
        let dot_col = if locked { theme::ASH } else { theme::CHALK };
        painter.circle_filled(Pos2::new(cx, cy), 4.5, dot_col);
        painter.circle_stroke(Pos2::new(cx, cy), 4.5, Stroke::new(1.0, if locked { theme::IRON } else { theme::FOG }));

        // Lock indicator in center
        if locked {
            painter.text(
                pad_rect.center(),
                egui::Align2::CENTER_CENTER,
                "L",
                egui::FontId::monospace(8.0),
                theme::IRON,
            );
        }

        // X axis label (below square, centered)
        let x_label_rect = Rect::from_min_size(
            Pos2::new(pad_rect.min.x, pad_rect.max.y + 1.0),
            Vec2::new(pad_rect.width(), label_h),
        );
        let col = if locked { theme::IRON } else { theme::SMOKE };
        painter.text(
            x_label_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{} {:.2}", label_x, x),
            egui::FontId::monospace(8.0),
            col,
        );

        // Y axis label (left of square, rotated — draw as vertical text via characters)
        // We paint it as a short text rotated 90°
        let y_label_center = Pos2::new(
            outer.min.x + label_w * 0.5,
            pad_rect.center().y,
        );
        painter.text(
            y_label_center,
            egui::Align2::CENTER_CENTER,
            format!("{:.2}", y),
            egui::FontId::monospace(7.5),
            col,
        );
        painter.text(
            Pos2::new(outer.min.x + label_w * 0.5, pad_rect.min.y + 5.0),
            egui::Align2::CENTER_CENTER,
            label_y,
            egui::FontId::monospace(7.0),
            col,
        );
    }

    changed
}

pub fn toggle_button(ui: &mut Ui, label: &str, active: &mut bool) -> bool {
    let fill = if *active { theme::IRON } else { theme::PIT };
    let text_color = if *active { theme::CHALK } else { theme::ASH };

    let button = egui::Button::new(
        egui::RichText::new(label).color(text_color).size(9.5).monospace()
    )
    .fill(fill)
    .stroke(Stroke::new(1.0, if *active { theme::ASH } else { theme::SLATE }))
    .min_size(Vec2::new(36.0, 16.0));

    let resp = ui.add(button);
    if resp.clicked() {
        *active = !*active;
        return true;
    }
    false
}
