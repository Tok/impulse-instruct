// ─── ui/widgets/xy_pad.rs ────────────────────────────────────────────────────

use super::super::theme;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Return the currently selected pair index for an XY pad identified by `pad_id`.
/// Call this *before* `xy_pad` to dispatch the correct `&mut f32` values on every frame.
/// Return the currently selected pair index for an XY pad with the given `pad_id`.
/// Call this *before* `xy_pad` on each frame to dispatch the correct `&mut f32` values.
pub fn xy_pad_pair(ctx: &egui::Context, pad_id: &str) -> usize {
    ctx.data(|d| d.get_temp(egui::Id::new(pad_id)).unwrap_or(0usize))
}

/// XY pad with optional pair cycling.
///
/// - `num_pairs` — how many param pairs this pad cycles through (1 = fixed, no cycle indicator).
///   The active pair index is persisted in egui temp-memory keyed by the widget's allocated id.
///   Right-click cycles to the next pair.
///
/// Returns `(changed, pair_index)`.  When `changed` is true the caller should write back
/// the new `x`/`y` values.  The caller should use `pair_index` on every frame to decide
/// which param pair to pass in as `x`/`y`.
pub fn xy_pad(
    ui: &mut Ui,
    pad_id: &str,
    label_x: &str,
    label_y: &str,
    x: &mut f32,
    y: &mut f32,
    size: f32,
    locked: bool,
    num_pairs: usize,
) -> (bool, usize) {
    let label_h = 14.0_f32;
    // Allocate the full size including bottom label; no left strip — Y label goes inside.
    let total = Vec2::new(size + 2.0, size + label_h + 2.0);
    let (outer, response) = ui.allocate_exact_size(total, Sense::click_and_drag());

    let pad_rect = Rect::from_min_size(outer.min, Vec2::splat(size));

    // ── Pair cycling (right-click) ────────────────────────────────────────────
    let mem_id = egui::Id::new(pad_id);
    let pair: usize = ui.ctx().data(|d| d.get_temp(mem_id).unwrap_or(0usize));
    if num_pairs > 1 && response.secondary_clicked() {
        let next = (pair + 1) % num_pairs;
        ui.ctx().data_mut(|d| d.insert_temp(mem_id, next));
    }

    let mut changed = false;

    // Interaction is clipped to the inner well (exclude the outer bevel).
    let bevel = 5.0_f32;
    let well = pad_rect.shrink(bevel);
    if !locked
        && (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        *x = ((pos.x - well.min.x) / well.width()).clamp(0.0, 1.0);
        *y = (1.0 - (pos.y - well.min.y) / well.height()).clamp(0.0, 1.0);
        changed = true;
    }

    let show_tooltip = response.hovered();
    let tooltip_text = format!("{}: {:.3}   {}: {:.3}", label_x, *x, label_y, *y);

    if ui.is_rect_visible(outer) {
        let painter = ui.painter();
        let hovered = response.hovered() && !locked;

        // ── Outer frame (neumorphic raised panel) ─────────────────────────────
        let frame_r = egui::Rounding::same(7.0);
        painter.rect_filled(pad_rect, frame_r, Color32::from_gray(22));
        // Top-left highlight — light catches the raised frame edge
        painter.rect_stroke(
            pad_rect.shrink(0.5),
            frame_r,
            Stroke::new(1.0, Color32::from_gray(50)),
        );
        // Bottom-right shadow — underside of raised frame
        painter.rect_stroke(
            pad_rect.shrink(1.5),
            frame_r,
            Stroke::new(1.5, Color32::from_gray(8)),
        );

        // ── Inner well (recessed rubber surface) ──────────────────────────────
        let well_r = egui::Rounding::same(4.0);
        painter.rect_filled(well, well_r, Color32::from_gray(11));
        // Inset shadow along top+left edges of well
        painter.rect_stroke(
            well.shrink(0.5),
            well_r,
            Stroke::new(1.0, Color32::from_gray(5)),
        );
        // Subtle sheen — light reflected off the rubber surface top edge
        painter.line_segment(
            [
                well.left_top() + Vec2::new(4.0, 1.0),
                well.right_top() + Vec2::new(-4.0, 1.0),
            ],
            Stroke::new(1.0, Color32::from_gray(if hovered { 32 } else { 22 })),
        );

        // ── Grid — etched into the rubber surface ─────────────────────────────
        let grid_col = Color32::from_gray(19);
        let center_col = Color32::from_gray(24);
        for t in [0.25_f32, 0.5, 0.75] {
            let col = if (t - 0.5).abs() < 0.01 {
                center_col
            } else {
                grid_col
            };
            let gx = well.min.x + well.width() * t;
            let gy = well.min.y + well.height() * t;
            painter.line_segment(
                [Pos2::new(gx, well.min.y), Pos2::new(gx, well.max.y)],
                Stroke::new(0.5, col),
            );
            painter.line_segment(
                [Pos2::new(well.min.x, gy), Pos2::new(well.max.x, gy)],
                Stroke::new(0.5, col),
            );
        }
        // Corner tick marks — physical range markers
        let tk = 5.0_f32;
        let tk_col = Color32::from_gray(30);
        for (cx, cy, dx, dy) in [
            (well.min.x, well.min.y, 1.0_f32, 1.0_f32),
            (well.max.x, well.min.y, -1.0, 1.0),
            (well.min.x, well.max.y, 1.0, -1.0),
            (well.max.x, well.max.y, -1.0, -1.0),
        ] {
            painter.line_segment(
                [Pos2::new(cx, cy), Pos2::new(cx + dx * tk, cy)],
                Stroke::new(1.0, tk_col),
            );
            painter.line_segment(
                [Pos2::new(cx, cy), Pos2::new(cx, cy + dy * tk)],
                Stroke::new(1.0, tk_col),
            );
        }

        // ── Cursor position crosshairs ────────────────────────────────────────
        let cx = well.min.x + well.width() * x.clamp(0.0, 1.0);
        let cy = well.min.y + well.height() * (1.0 - y.clamp(0.0, 1.0));
        let cross_col = Color32::from_gray(if hovered { 42 } else { 30 });
        painter.line_segment(
            [Pos2::new(cx, well.min.y), Pos2::new(cx, well.max.y)],
            Stroke::new(0.5, cross_col),
        );
        painter.line_segment(
            [Pos2::new(well.min.x, cy), Pos2::new(well.max.x, cy)],
            Stroke::new(0.5, cross_col),
        );

        // ── Rubber nub cursor ─────────────────────────────────────────────────
        let dot_r = (size * 0.08).max(5.0);
        let dot_pos = Pos2::new(cx, cy);
        if !locked {
            // Soft drop shadow
            painter.circle_filled(
                dot_pos + Vec2::new(1.0, 2.0),
                dot_r + 1.5,
                Color32::from_rgba_unmultiplied(0, 0, 0, 90),
            );
            // Outer ambient ring — rubber material edge
            painter.circle_filled(dot_pos, dot_r + 1.0, Color32::from_gray(12));
            // Body — rubber dome
            painter.circle_filled(dot_pos, dot_r, Color32::from_gray(48));
            // Inner gradient: lighter center simulates dome curvature
            painter.circle_filled(dot_pos, dot_r * 0.62, Color32::from_gray(62));
            // Specular highlight — top-left catch light
            let spec_off = Vec2::new(-dot_r * 0.28, -dot_r * 0.30);
            painter.circle_filled(dot_pos + spec_off, dot_r * 0.22, Color32::from_gray(160));
            // Glow ring when hovered or active
            if hovered || response.dragged() {
                painter.circle_stroke(
                    dot_pos,
                    dot_r + 2.5,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(180, 180, 180, 30)),
                );
                painter.circle_stroke(
                    dot_pos,
                    dot_r + 1.2,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(200, 200, 200, 50)),
                );
            }
        } else {
            painter.circle_filled(dot_pos, dot_r, Color32::from_gray(28));
            painter.circle_stroke(dot_pos, dot_r, Stroke::new(1.0, Color32::from_gray(38)));
        }

        if locked {
            painter.text(
                well.center(),
                egui::Align2::CENTER_CENTER,
                "U",
                egui::FontId::monospace(8.0),
                theme::IRON,
            );
        }

        // ── Labels overlaid inside the pad ────────────────────────────────────
        let col = if locked {
            theme::IRON
        } else {
            Color32::from_gray(55)
        };
        // Y axis label — top-left of well
        painter.text(
            well.left_top() + Vec2::new(4.0, 5.0),
            egui::Align2::LEFT_TOP,
            label_y,
            egui::FontId::monospace(7.0),
            col,
        );
        // Y value — bottom-left of well
        painter.text(
            well.left_bottom() + Vec2::new(4.0, -5.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.2}", y),
            egui::FontId::monospace(7.0),
            col,
        );

        // Pair-cycle indicator — top-right inside well
        if num_pairs > 1 {
            let ind_text = format!("{}/{}", pair + 1, num_pairs);
            painter.text(
                well.right_top() + Vec2::new(-4.0, 5.0),
                egui::Align2::RIGHT_TOP,
                ind_text,
                egui::FontId::monospace(6.5),
                if response.hovered() {
                    Color32::from_gray(100)
                } else {
                    Color32::from_gray(45)
                },
            );
        }

        // X label + value — bottom of pad (below frame)
        let x_label_rect = Rect::from_min_size(
            Pos2::new(pad_rect.min.x, pad_rect.max.y + 1.0),
            Vec2::new(pad_rect.width(), label_h),
        );
        let label_col = if locked { theme::IRON } else { theme::SMOKE };
        painter.text(
            x_label_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{} {:.2}", label_x, x),
            egui::FontId::monospace(8.0),
            label_col,
        );
    }

    if show_tooltip {
        response.on_hover_ui(|ui| {
            ui.label(egui::RichText::new(&tooltip_text).monospace().size(9.5));
        });
    }

    (changed, pair)
}
