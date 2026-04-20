// ─── ui/widgets/knob.rs ──────────────────────────────────────────────────────
// The main rotary `knob` widget: the 0–1 left-drag control used by every
// param knob in the rack + panels.  Extracted from widgets/mod.rs to
// keep that entry module under the 1000-line cap — it holds the
// knob-chrome variant, the slider, toggle button, glass helpers,
// section header, and the re-exports for every other sub-widget.
//
// `draw_knob` and `draw_arc` are the pure painting helpers invoked by
// `knob`; they're also called from the chrome variant in mod.rs so
// they stay `pub(super)`.

use egui::{Color32, Painter, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use std::f32::consts::TAU;

use crate::state::ParamMode;

use super::{
    KNOB_ARC_R, KNOB_BODY_R, ctrl_effective, dir_key_down, mode_char, mode_tooltip, theme,
    touch_mode,
};

/// A rotary knob widget.
/// Returns `(value_changed, mode_cycled)`.
/// Left-drag changes the value.  When a touch-paint mode is active, a primary
/// click paints that mode instead of dragging; otherwise no mode change occurs.
pub fn knob(ui: &mut Ui, label: &str, value: &mut f32, mode: ParamMode, size: f32) -> (bool, bool) {
    let label_h = (size * 0.28).max(14.0).round();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(size, size + label_h), Sense::click_and_drag());

    let knob_center = Pos2::new(rect.center().x, rect.min.y + size * 0.5);
    let knob_rect = Rect::from_center_size(knob_center, Vec2::splat(size));
    let label_rect = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + size + 1.0),
        Vec2::new(rect.width(), label_h),
    );
    let label_font_size = (size * 0.175).clamp(8.0, 13.0);

    let tmode = touch_mode(ui);
    let mut changed = false;

    // Only drag-to-change when no touch-paint mode is active.
    if tmode.is_none() && response.dragged() {
        let delta = response.drag_delta();
        *value = (*value - delta.y * 0.005 + delta.x * 0.003).clamp(0.0, 1.0);
        changed = true;
    }
    // Arrow-key fine adjustment when hovered.
    if response.hovered() && tmode.is_none() {
        let step = 0.01;
        if dir_key_down(ui, egui::Key::ArrowRight, egui::Key::D) {
            *value = (*value + step).clamp(0.0, 1.0);
            changed = true;
        }
        if dir_key_down(ui, egui::Key::ArrowLeft, egui::Key::A) {
            *value = (*value - step).clamp(0.0, 1.0);
            changed = true;
        }
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let anim_time = ui.ctx().input(|i| i.time) as f32;
        draw_knob(
            painter,
            knob_rect,
            *value,
            mode,
            response.hovered(),
            anim_time,
        );
        if mode == ParamMode::LlmFocus {
            ui.ctx().request_repaint(); // keep Focus shimmer animating
        }
        painter.text(
            label_rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(label_font_size),
            if mode == ParamMode::UserOwned {
                theme::ASH
            } else {
                theme::SMOKE
            },
        );
    }

    // Only attach the lock-state tooltip when the knob is non-default —
    // untouched knobs stay silent to avoid visual/verbal noise.
    let response = match mode_tooltip(mode) {
        Some(text) => response.on_hover_text(text),
        None => response,
    };
    // Ctrl+click cycles lock mode. Touch-paint mode also uses primary click.
    let ctrl_held = ctrl_effective(&response.ctx);
    let primary_click = response.clicked_by(egui::PointerButton::Primary);
    let mode_cycled = primary_click && (ctrl_held || tmode.is_some());
    (changed, mode_cycled)
}

fn draw_knob(painter: &Painter, rect: Rect, value: f32, mode: ParamMode, hovered: bool, time: f32) {
    let center = rect.center();
    let radius = rect.width() * KNOB_BODY_R;

    let start_angle: f32 = std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4 * 3.0;
    let sweep = TAU * 0.75;
    let filled_sweep = sweep * value;

    // ── Scale marks (same geometry as chrome for visual consistency) ──────────
    for i in 0..=12usize {
        let t = i as f32 / 12.0;
        let is_major = i % 3 == 0;
        let angle = start_angle + sweep * t;
        let (inner_r, outer_r, col, width) = if is_major {
            (radius * 1.10, radius * 1.28, 80u8, 1.5_f32)
        } else {
            (radius * 1.12, radius * 1.21, 42u8, 1.0_f32)
        };
        let p_inner = center + Vec2::new(angle.cos(), angle.sin()) * inner_r;
        let p_outer = center + Vec2::new(angle.cos(), angle.sin()) * outer_r;
        painter.line_segment(
            [p_inner, p_outer],
            Stroke::new(width, Color32::from_gray(col)),
        );
    }

    // ── Brushed body — radial lines instead of concentric rings ──────────────
    // Drop shadow
    painter.circle_filled(center + Vec2::new(0.5, 1.0), radius + 1.0, theme::VOID);

    // Base fill — mode-tinted
    let mode_shift: i16 = match mode {
        ParamMode::UserOwned => -10,
        ParamMode::LlmFocus => 12,
        ParamMode::Free => 0,
    };
    let base_g = (32i16 + mode_shift + if hovered { 8 } else { 0 }).clamp(4, 80) as u8;
    painter.circle_filled(center, radius, Color32::from_gray(base_g));

    // Radial brush strokes — 48 uniform spokes from inner to outer
    let spoke_g = (base_g as i16 + 10).clamp(4, 90) as u8;
    for i in 0..48u32 {
        let angle = TAU * i as f32 / 48.0;
        let a = center + Vec2::new(angle.cos(), angle.sin()) * (radius * 0.20);
        let b = center + Vec2::new(angle.cos(), angle.sin()) * (radius * 0.88);
        painter.line_segment([a, b], Stroke::new(1.0, Color32::from_gray(spoke_g)));
    }

    // Centre hub — dark recessed disc
    let hub_r = radius * 0.22;
    painter.circle_filled(center, hub_r, Color32::from_gray(12));
    painter.circle_stroke(center, hub_r, Stroke::new(0.5, Color32::from_gray(40)));

    // Outer rim — polished edge; Focus mode shimmers
    let rim_col = match mode {
        ParamMode::LlmFocus => {
            let pulse = (time * TAU).sin() * 0.5 + 0.5;
            (110.0 + pulse * 60.0) as u8
        }
        _ => {
            if hovered {
                140u8
            } else {
                90u8
            }
        }
    };
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_gray(rim_col)),
    );

    // Knurled edge — subtle inset groove just inside the rim
    painter.circle_stroke(
        center,
        radius * 0.94,
        Stroke::new(0.5, Color32::from_gray(16)),
    );

    // ── Value arc ring ───────────────────────────────────────────────────────
    let ring_r = radius * KNOB_ARC_R;
    draw_arc(
        painter,
        center,
        ring_r,
        start_angle,
        sweep,
        2.5,
        Color32::from_gray(35),
    );

    let arc_color = match mode {
        ParamMode::UserOwned => Color32::from_gray(70),
        ParamMode::LlmFocus => theme::CHALK,
        ParamMode::Free => Color32::from_gray(180),
    };
    if filled_sweep > 0.01 {
        draw_arc(
            painter,
            center,
            ring_r,
            start_angle,
            filled_sweep,
            2.0,
            arc_color,
        );
    }

    // ── Pointer line — shadow + bright ───────────────────────────────────────
    let ptr_angle = start_angle + filled_sweep;
    let ptr_a = center + Vec2::new(ptr_angle.cos(), ptr_angle.sin()) * (radius * 0.15);
    let ptr_b = center + Vec2::new(ptr_angle.cos(), ptr_angle.sin()) * (radius * 0.80);
    let shadow = Vec2::new(0.5, 0.5);
    painter.line_segment(
        [ptr_a + shadow, ptr_b + shadow],
        Stroke::new(1.0, theme::VOID),
    );
    painter.line_segment([ptr_a, ptr_b], Stroke::new(2.0, Color32::from_gray(200)));

    // ── Mode indicator ───────────────────────────────────────────────────────
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        mode_char(mode),
        egui::FontId::monospace(7.0),
        match mode {
            ParamMode::Free => Color32::from_gray(42),
            ParamMode::UserOwned => Color32::from_gray(72),
            ParamMode::LlmFocus => Color32::from_gray(185),
        },
    );
}

pub(super) fn draw_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    start: f32,
    sweep: f32,
    width: f32,
    color: Color32,
) {
    let steps = ((sweep.abs() * radius * 2.0) as usize).clamp(12, 96);
    let points: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let a = start + sweep * i as f32 / steps as f32;
            center + Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect();
    // Single polyline avoids seam artifacts at joints (vs individual line_segments)
    painter.add(egui::Shape::line(points, Stroke::new(width, color)));
}
