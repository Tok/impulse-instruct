// ─── ui/widgets/pan.rs ── compact pan slider ─────────────────────────────────
// L ──●── R slider for stereo placement, range -1.0 to +1.0.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

/// Draw a compact pan slider. Returns true if the value changed.
/// `width`: total width in pixels. `pan`: mutable reference to -1.0..+1.0.
pub fn pan_slider(ui: &mut Ui, pan: &mut f32, width: f32) -> bool {
    let height = 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::drag());
    let changed = if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
            *pan = (t * 2.0 - 1.0).clamp(-1.0, 1.0);
            true
        } else {
            false
        }
    } else if resp.double_clicked() {
        *pan = 0.0; // double-click resets to center
        true
    } else {
        false
    };

    let painter = ui.painter_at(rect);
    let cy = rect.center().y;

    // Track line
    painter.line_segment(
        [
            Pos2::new(rect.min.x + 2.0, cy),
            Pos2::new(rect.max.x - 2.0, cy),
        ],
        Stroke::new(1.0, Color32::from_gray(40)),
    );

    // Center tick
    let cx = rect.center().x;
    painter.line_segment(
        [Pos2::new(cx, cy - 3.0), Pos2::new(cx, cy + 3.0)],
        Stroke::new(1.0, Color32::from_gray(50)),
    );

    // Position indicator
    let t = (*pan + 1.0) * 0.5; // -1..+1 → 0..1
    let px = rect.min.x + 2.0 + t * (rect.width() - 4.0);
    let indicator_col = if pan.abs() < 0.05 {
        Color32::from_gray(100) // near center = dim
    } else {
        Color32::from_gray(180) // off-center = bright
    };
    painter.circle_filled(Pos2::new(px, cy), 4.0, indicator_col);
    painter.circle_stroke(
        Pos2::new(px, cy),
        4.0,
        Stroke::new(1.0, Color32::from_gray(30)),
    );

    // L / R labels
    let font = egui::FontId::monospace(6.0);
    painter.text(
        Pos2::new(rect.min.x + 1.0, cy),
        egui::Align2::LEFT_CENTER,
        "L",
        font.clone(),
        Color32::from_gray(45),
    );
    painter.text(
        Pos2::new(rect.max.x - 1.0, cy),
        egui::Align2::RIGHT_CENTER,
        "R",
        font,
        Color32::from_gray(45),
    );

    changed
}
