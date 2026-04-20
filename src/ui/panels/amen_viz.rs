// ─── ui/panels/amen_viz.rs ────────────────────────────────────────────────────
// Visualization helpers for the AmenSampler panel — waveform thumbnail,
// slice wheel, and the per-frame animation state that drives them.
// Split out of amen.rs to stay under the 1000-line limit.

use crate::ui::theme;

/// Per-frame animation state for the amen panel.  Holds the waveform
/// thumbnail cache (path + min/max pairs) and the slice-trail history
/// that drives the wheel / waveform fade animations.
pub(crate) struct AmenUiState {
    pub wave_cache: (String, Vec<(f32, f32)>),
    /// Recently-played slices: (slice_idx, time_triggered).  Pruned to
    /// ~half a bar at draw time.
    pub slice_trail: Vec<(u8, f64)>,
    /// Last slice seen by the trail builder; used to detect new triggers.
    pub last_trail_slice: Option<u8>,
    /// Smoothed pointer angle (radians) for the rotating wheel indicator.
    pub wheel_angle: f32,
}

impl Default for AmenUiState {
    fn default() -> Self {
        Self {
            wave_cache: (String::new(), Vec::new()),
            slice_trail: Vec::with_capacity(16),
            last_trail_slice: None,
            wheel_angle: -std::f32::consts::FRAC_PI_2,
        }
    }
}

/// Paint the waveform thumbnail into `rect` with slice-boundary markers
/// and start/end offset shading.  `active_slice` (if any) highlights the
/// currently-playing slice wedge with a fading flash + playhead cursor.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_waveform(
    ui: &mut egui::Ui,
    thumb: &[(f32, f32)],
    slice_count: u8,
    start_offset: f32,
    end_offset: f32,
    slice_positions: &[f32],
    active_slice: Option<u8>,
    width: f32,
    height: f32,
    trail: &[(u8, f64)],
    now: f64,
    step_dur: f64,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::Rounding::same(2.0),
        egui::Color32::from_gray(10),
    );
    painter.rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
    );
    if thumb.is_empty() {
        return;
    }
    let mid = rect.center().y;
    let half_h = rect.height() * 0.45;
    let col_count = thumb.len();
    let col_w = rect.width() / col_count as f32;
    for (i, (mn, mx)) in thumb.iter().enumerate() {
        let x = rect.min.x + i as f32 * col_w + col_w * 0.5;
        let y_top = mid - mx * half_h;
        let y_bot = mid - mn * half_h;
        painter.line_segment(
            [egui::pos2(x, y_top), egui::pos2(x, y_bot)],
            egui::Stroke::new(col_w.max(1.0), egui::Color32::from_gray(160)),
        );
    }
    let shade = egui::Color32::from_rgba_unmultiplied(8, 8, 8, 180);
    if start_offset > 0.0 {
        let x0 = rect.min.x;
        let x1 = rect.min.x + start_offset * rect.width();
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.min.y), egui::pos2(x1, rect.max.y)),
            egui::Rounding::ZERO,
            shade,
        );
    }
    if end_offset < 1.0 {
        let x0 = rect.min.x + end_offset * rect.width();
        let x1 = rect.max.x;
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.min.y), egui::pos2(x1, rect.max.y)),
            egui::Rounding::ZERO,
            shade,
        );
    }
    let slices = slice_count.max(1);
    let region_w = (end_offset - start_offset).max(0.001) * rect.width();
    let region_x0 = rect.min.x + start_offset * rect.width();
    let slice_w_equal = region_w / slices as f32;
    // Custom markers are only honored if there are enough of them to
    // cover slice_count.  Otherwise auto-detection yielded a partial
    // result and falling back to equal divisions avoids drawing the
    // active-slice highlight at x=0 for the missing entries.
    let use_custom = !slice_positions.is_empty() && slice_positions.len() >= slices as usize;
    let marker_color = if use_custom {
        egui::Color32::from_gray(140)
    } else {
        egui::Color32::from_gray(80)
    };
    if use_custom {
        for &p in slice_positions {
            if !p.is_finite() {
                continue;
            }
            let x = rect.min.x + p.clamp(0.0, 1.0) * rect.width();
            painter.line_segment(
                [
                    egui::pos2(x, rect.min.y + 1.0),
                    egui::pos2(x, rect.max.y - 1.0),
                ],
                egui::Stroke::new(0.8, marker_color),
            );
        }
        let x_end = rect.min.x + end_offset * rect.width();
        painter.line_segment(
            [
                egui::pos2(x_end, rect.min.y + 1.0),
                egui::pos2(x_end, rect.max.y - 1.0),
            ],
            egui::Stroke::new(0.8, marker_color),
        );
    } else {
        for i in 0..=slices {
            let x = region_x0 + i as f32 * slice_w_equal;
            painter.line_segment(
                [
                    egui::pos2(x, rect.min.y + 1.0),
                    egui::pos2(x, rect.max.y - 1.0),
                ],
                egui::Stroke::new(0.6, marker_color),
            );
        }
    }
    if let Some(a) = active_slice
        && a < slices
    {
        let trig_t = trail
            .iter()
            .filter(|(s, _)| *s == a)
            .map(|(_, t)| *t)
            .fold(f64::NEG_INFINITY, f64::max);
        let fresh = if trig_t.is_finite() && step_dur > 0.0 {
            (1.0 - ((now - trig_t).max(0.0) / step_dur) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let alpha = (30.0 + fresh * 90.0) as u8;
        let (x0, x1) = if use_custom {
            let a_x = slice_positions
                .get(a as usize)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(start_offset)
                .clamp(0.0, 1.0);
            let next = slice_positions
                .get(a as usize + 1)
                .copied()
                .filter(|v| v.is_finite())
                .unwrap_or(end_offset)
                .clamp(0.0, 1.0);
            (
                rect.min.x + a_x * rect.width(),
                rect.min.x + next * rect.width(),
            )
        } else {
            let x0 = region_x0 + a as f32 * slice_w_equal;
            (x0, x0 + slice_w_equal)
        };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x0, rect.min.y + 1.0),
                egui::pos2(x1, rect.max.y - 1.0),
            ),
            egui::Rounding::ZERO,
            egui::Color32::from_rgba_unmultiplied(200, 200, 200, alpha),
        );
        if trig_t.is_finite() && step_dur > 0.0 {
            let frac = (((now - trig_t).max(0.0) / step_dur) as f32).clamp(0.0, 1.0);
            let px = x0 + frac * (x1 - x0);
            painter.line_segment(
                [
                    egui::pos2(px, rect.min.y + 1.0),
                    egui::pos2(px, rect.max.y - 1.0),
                ],
                egui::Stroke::new(1.0, theme::CHALK),
            );
            ui.ctx().request_repaint();
        }
    }
    painter.line_segment(
        [egui::pos2(rect.min.x, mid), egui::pos2(rect.max.x, mid)],
        egui::Stroke::new(0.3, egui::Color32::from_gray(45)),
    );
}

/// Draw the circular slice-wheel visualization with trail-fade per wedge
/// plus a smoothly rotating outer pointer notch eased toward the active
/// slice's mid-angle.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_slice_wheel(
    ui: &mut egui::Ui,
    slice_count: u8,
    active_slice: Option<u8>,
    reverse: bool,
    looping: bool,
    size: f32,
    trail: &[(u8, f64)],
    now: f64,
    trail_window: f64,
    pointer_angle: f32,
) {
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let r_outer = size * 0.46;
    let r_inner = size * 0.22;

    let n = slice_count.max(1) as usize;
    let tau = std::f32::consts::TAU;
    let dir = if reverse { -1.0 } else { 1.0 };
    let fade_for = |idx: usize| -> f32 {
        let mut best = f64::NEG_INFINITY;
        for (s, t) in trail {
            if *s as usize == idx && *t > best {
                best = *t;
            }
        }
        if best.is_finite() && trail_window > 0.0 {
            let age = (now - best).max(0.0);
            (1.0 - (age / trail_window) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };

    if n == 1 {
        let fade = fade_for(0).max(if active_slice.is_some() { 1.0 } else { 0.0 });
        let g = (40.0 + fade * 200.0) as u8;
        painter.circle_filled(center, r_outer, egui::Color32::from_gray(g));
    } else {
        let t = ui.ctx().input(|i| i.time) as f32;
        let pulse = (t * 4.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
        for i in 0..n {
            let a0 = -std::f32::consts::FRAC_PI_2 + (i as f32 / n as f32) * tau * dir;
            let a1 = -std::f32::consts::FRAC_PI_2 + ((i + 1) as f32 / n as f32) * tau * dir;
            let active = active_slice.map(|s| s as usize) == Some(i);
            let fade = fade_for(i);
            let base = 40.0 + fade * 150.0;
            let active_bonus = if active { 50.0 * pulse } else { 0.0 };
            let g = (base + active_bonus).clamp(40.0, 240.0) as u8;
            let fill = egui::Color32::from_gray(g);
            let steps = 12;
            let mut pts = vec![center];
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                let a = a0 + (a1 - a0) * t;
                pts.push(center + egui::vec2(a.cos(), a.sin()) * r_outer);
            }
            painter.add(egui::Shape::convex_polygon(
                pts,
                fill,
                egui::Stroke::new(0.5, egui::Color32::from_gray(15)),
            ));
            let mid_angle = (a0 + a1) * 0.5;
            let label_r = (r_outer + r_inner) * 0.5;
            let label_pos = center + egui::vec2(mid_angle.cos(), mid_angle.sin()) * label_r;
            let label_col = if active {
                egui::Color32::from_gray(20)
            } else {
                egui::Color32::from_gray(120)
            };
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                egui::FontId::monospace(size * 0.07),
                label_col,
            );
        }
    }
    if active_slice.is_some() || !trail.is_empty() {
        ui.ctx().request_repaint();
    }
    painter.circle_filled(center, r_inner, egui::Color32::from_gray(12));
    painter.circle_stroke(center, r_outer, egui::Stroke::new(1.0, theme::ASH));
    if active_slice.is_some() || !trail.is_empty() {
        let pa = pointer_angle;
        let tip = center + egui::vec2(pa.cos(), pa.sin()) * (r_outer + 4.0);
        let base_l = center + egui::vec2((pa + 0.18).cos(), (pa + 0.18).sin()) * (r_outer - 2.0);
        let base_r = center + egui::vec2((pa - 0.18).cos(), (pa - 0.18).sin()) * (r_outer - 2.0);
        painter.add(egui::Shape::convex_polygon(
            vec![tip, base_l, base_r],
            theme::CHALK,
            egui::Stroke::NONE,
        ));
    }
    if looping {
        painter.circle_stroke(center, r_outer + 3.0, egui::Stroke::new(1.0, theme::CHALK));
    }
    let arrow_col = theme::ASH;
    let hub_label = if reverse { "↺" } else { "↻" };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        hub_label,
        egui::FontId::monospace(size * 0.28),
        arrow_col,
    );
}
