// ─── ui/widgets/adsr.rs ───────────────────────────────────────────────────────
// Interactive ADSR envelope visualiser widget.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

/// Interactive ADSR envelope display. Draws the classic attack-decay-sustain-release
/// shape as a polyline. Drag in each zone to edit the corresponding parameter:
///   - Left zone       → A (attack time)
///   - Center-left     → D (decay time) and vertical → S (sustain level)
///   - Center-right    → S (sustain level, vertical drag)
///   - Right zone      → R (release time)
///
/// All parameters are 0–1. Returns `true` if any value changed.
/// `width` and `height` are the total pixel dimensions of the display area.
pub fn adsr_display(
    ui: &mut Ui,
    attack: &mut f32,
    decay: &mut f32,
    sustain: &mut f32,
    release: &mut f32,
    width: f32,
    height: f32,
) -> bool {
    let size = Vec2::new(width, height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    let mut changed = false;

    // ── Drag handling ─────────────────────────────────────────────────────────
    // Zone widths proportional to A, D, sustain_hold (fixed 0.2), R.
    let scale = 0.6_f32; // A+D+R each share up to 35% of scale; sustain hold fixed 20%
    let zones = |a: f32, d: f32, r: f32| -> (f32, f32, f32) {
        let aw = a * scale * 0.35;
        let dw = d * scale * 0.35;
        let sw = 0.20_f32;
        let rw = r * scale * 0.35;
        let total = (aw + dw + sw + rw).max(0.01);
        (aw / total, (aw + dw) / total, (aw + dw + sw) / total)
    };

    if response.dragged()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let delta = response.drag_delta();
        let px = (pos.x - rect.min.x) / rect.width();
        let dx = delta.x / rect.width();
        let dy = delta.y / rect.height(); // positive = down = smaller value

        let (a_end, d_end, s_end) = zones(*attack, *decay, *release);

        if px < a_end {
            *attack = (*attack + dx * 4.0).clamp(0.0, 1.0);
            changed = true;
        } else if px < d_end {
            *decay = (*decay + dx * 4.0).clamp(0.0, 1.0);
            *sustain = (*sustain - dy * 2.0).clamp(0.0, 1.0);
            changed = true;
        } else if px < s_end {
            *sustain = (*sustain - dy * 2.0).clamp(0.0, 1.0);
            changed = true;
        } else {
            *release = (*release + dx * 4.0).clamp(0.0, 1.0);
            changed = true;
        }
    }

    // ── Drawing ───────────────────────────────────────────────────────────────
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        // Background
        painter.rect_filled(rect, egui::Rounding::same(2.0), Color32::from_gray(10));
        painter.rect_stroke(
            rect,
            egui::Rounding::same(2.0),
            Stroke::new(1.0, Color32::from_gray(30)),
        );
        // Top sheen
        painter.line_segment(
            [
                rect.left_top() + Vec2::new(2.0, 0.5),
                rect.right_top() - Vec2::new(2.0, 0.0),
            ],
            Stroke::new(1.0, Color32::from_gray(45)),
        );

        // Geometry
        let (x_a, x_d, x_s) = zones(*attack, *decay, *release);
        let x_r = 1.0_f32;

        let pad = 3.0_f32;
        let inner_w = rect.width() - pad * 2.0;
        let inner_h = rect.height() - pad * 2.0;
        let px = |t: f32| rect.min.x + pad + t * inner_w;
        let py = |v: f32| rect.min.y + pad + (1.0 - v) * inner_h;

        let pts = [
            Pos2::new(px(0.0), py(0.0)),
            Pos2::new(px(x_a), py(1.0)),
            Pos2::new(px(x_d), py(*sustain)),
            Pos2::new(px(x_s), py(*sustain)),
            Pos2::new(px(x_r), py(0.0)),
        ];

        // Fill under the curve
        let fill_col = Color32::from_rgba_premultiplied(55, 55, 55, 30);
        let fill_pts: Vec<Pos2> = pts
            .iter()
            .cloned()
            .chain([Pos2::new(px(x_r), py(0.0)), Pos2::new(px(0.0), py(0.0))])
            .collect();
        painter.add(egui::Shape::convex_polygon(
            fill_pts,
            fill_col,
            Stroke::NONE,
        ));

        // Envelope polyline
        let line_col = if response.hovered() || response.dragged() {
            Color32::from_gray(200)
        } else {
            Color32::from_gray(140)
        };
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_col));
        }

        // Breakpoint dots at peak, decay-end, sustain-end
        for &pt in &pts[1..4] {
            painter.circle_filled(pt, 2.5, Color32::from_gray(180));
        }

        // Hover: show zone label
        if response.hovered()
            && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
        {
            let hx = (pos.x - rect.min.x) / rect.width();
            let label = if hx < x_a {
                "A"
            } else if hx < x_d {
                "D/S"
            } else if hx < x_s {
                "S"
            } else {
                "R"
            };
            painter.text(
                rect.right_top() + Vec2::new(-4.0, 4.0),
                egui::Align2::RIGHT_TOP,
                label,
                egui::FontId::monospace(8.0),
                Color32::from_gray(100),
            );
        }
    }

    changed
}
