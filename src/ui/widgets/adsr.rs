// ─── ui/widgets/adsr.rs ───────────────────────────────────────────────────────
// Interactive ADSR envelope visualiser widget + simplified decay-only display.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::ui::theme;

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

        // Always-on parameter labels
        let label_col = theme::ASH;
        let font = egui::FontId::monospace(7.0);
        // A label (attack time as ms: 0–1 → 0–500ms)
        let a_ms = *attack * 500.0;
        painter.text(
            Pos2::new(px(x_a * 0.5), py(0.0) + 2.0),
            egui::Align2::CENTER_TOP,
            format!("A {:.0}ms", a_ms),
            font.clone(),
            label_col,
        );
        // D label (decay time as ms: 0–1 → 0–2000ms)
        let d_ms = *decay * 2000.0;
        painter.text(
            Pos2::new(px((x_a + x_d) * 0.5), py(*sustain) - 3.0),
            egui::Align2::CENTER_BOTTOM,
            format!("D {:.0}ms", d_ms),
            font.clone(),
            label_col,
        );
        // S label (sustain level)
        painter.text(
            Pos2::new(px((x_d + x_s) * 0.5), py(*sustain) - 3.0),
            egui::Align2::CENTER_BOTTOM,
            format!("S {:.0}%", *sustain * 100.0),
            font.clone(),
            label_col,
        );
        // R label (release time as ms: 0–1 → 0–3000ms)
        let r_ms = *release * 3000.0;
        painter.text(
            Pos2::new(px((x_s + x_r) * 0.5), py(0.0) + 2.0),
            egui::Align2::CENTER_TOP,
            format!("R {:.0}ms", r_ms),
            font,
            label_col,
        );

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

/// Simplified decay-only envelope display for the 303 bass.
/// Shows an instant attack to `env_mod` peak, then linear decay to zero.
/// Drag horizontally to edit `decay`. Returns `true` if changed.
pub fn decay_display(ui: &mut Ui, decay: &mut f32, env_mod: f32, width: f32, height: f32) -> bool {
    let size = Vec2::new(width, height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());

    let mut changed = false;

    if response.dragged() {
        let dx = response.drag_delta().x / rect.width();
        *decay = (*decay + dx * 2.0).clamp(0.0, 1.0);
        changed = true;
    }

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();

        painter.rect_filled(rect, egui::Rounding::same(2.0), Color32::from_gray(10));
        painter.rect_stroke(
            rect,
            egui::Rounding::same(2.0),
            Stroke::new(1.0, Color32::from_gray(30)),
        );
        painter.line_segment(
            [
                rect.left_top() + Vec2::new(2.0, 0.5),
                rect.right_top() - Vec2::new(2.0, 0.0),
            ],
            Stroke::new(1.0, Color32::from_gray(45)),
        );

        let pad = 3.0_f32;
        let inner_w = rect.width() - pad * 2.0;
        let inner_h = rect.height() - pad * 2.0;
        let px = |t: f32| rect.min.x + pad + t * inner_w;
        let py = |v: f32| rect.min.y + pad + (1.0 - v) * inner_h;

        // Shape: (0,0) → instant rise to env_mod at x=0.1 → decay to 0 at x=0.1+decay*0.9
        let peak_x = 0.08_f32;
        let decay_end_x = peak_x + *decay * (1.0 - peak_x);
        let peak = env_mod.clamp(0.0, 1.0);

        let pts = [
            Pos2::new(px(0.0), py(0.0)),
            Pos2::new(px(peak_x), py(peak)),
            Pos2::new(px(decay_end_x), py(0.0)),
            Pos2::new(px(1.0), py(0.0)),
        ];

        let fill_col = Color32::from_rgba_premultiplied(55, 55, 55, 30);
        painter.add(egui::Shape::convex_polygon(
            pts.to_vec(),
            fill_col,
            Stroke::NONE,
        ));

        let line_col = if response.hovered() || response.dragged() {
            Color32::from_gray(200)
        } else {
            Color32::from_gray(140)
        };
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_col));
        }
        painter.circle_filled(pts[1], 2.5, Color32::from_gray(180));

        // Labels: peak Hz value + decay time
        let label_col = theme::ASH;
        let font = egui::FontId::monospace(7.0);
        // Decay time: 0–1 → 50–2000 ms
        let d_ms = 50.0 + *decay * 1950.0;
        painter.text(
            Pos2::new(px(decay_end_x), py(0.0) + 2.0),
            egui::Align2::CENTER_TOP,
            format!("{:.0}ms", d_ms),
            font.clone(),
            label_col,
        );
        // Env mod depth
        painter.text(
            pts[1] + Vec2::new(4.0, -2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{:.0}%", peak * 100.0),
            font,
            label_col,
        );
    }

    changed
}

/// Filter frequency response display. Draws an approximate magnitude response
/// curve for a ladder filter given cutoff (0–1), resonance (0–1), and filter mode.
/// Cutoff maps to 200–8000 Hz via `200 * 40^cutoff`. Non-interactive (display only).
pub fn filter_response(
    ui: &mut Ui,
    cutoff: f32,
    resonance: f32,
    filter_mode: u8, // 0=LP, 1=HP, 2=BP
    width: f32,
    height: f32,
) {
    let size = Vec2::new(width, height);
    let (rect, _response) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();

    // Background
    painter.rect_filled(rect, egui::Rounding::same(2.0), Color32::from_gray(10));
    painter.rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        Stroke::new(1.0, Color32::from_gray(30)),
    );
    painter.line_segment(
        [
            rect.left_top() + Vec2::new(2.0, 0.5),
            rect.right_top() - Vec2::new(2.0, 0.0),
        ],
        Stroke::new(1.0, Color32::from_gray(45)),
    );

    let pad = 3.0_f32;
    let inner_w = rect.width() - pad * 2.0;
    let inner_h = rect.height() - pad * 2.0;

    // Frequency range: 20 Hz to 20 kHz (log scale)
    let f_min = 20.0_f32;
    let f_max = 20000.0_f32;
    let log_min = f_min.ln();
    let log_max = f_max.ln();

    // Cutoff frequency in Hz
    let fc = 200.0 * 40.0_f32.powf(cutoff.clamp(0.0, 1.0));
    let k = resonance.clamp(0.0, 1.0) * 3.8; // resonance factor

    // Approximate 4-pole ladder magnitude response
    let n_points = 128;
    let mut points = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let t = i as f32 / (n_points - 1) as f32;
        let freq = (log_min + t * (log_max - log_min)).exp();
        let ratio = freq / fc;

        let mag_lp = {
            let r8 = ratio.powi(8);
            let base = 1.0 / (1.0 + r8).sqrt();
            let peak_boost = if k > 0.01 {
                let q = 0.5 + k * 4.0;
                let peak_w = (ratio - 1.0).abs();
                1.0 + q / (1.0 + (peak_w * q * 3.0).powi(2)) * 0.5
            } else {
                1.0
            };
            (base * peak_boost).min(3.0)
        };

        let mag = match filter_mode {
            1 => {
                // HP
                let r8 = ratio.powi(8);
                let hp_base = (r8 / (1.0 + r8)).sqrt();
                let peak_boost = if k > 0.01 {
                    let q = 0.5 + k * 4.0;
                    let peak_w = (ratio - 1.0).abs();
                    1.0 + q / (1.0 + (peak_w * q * 3.0).powi(2)) * 0.5
                } else {
                    1.0
                };
                (hp_base * peak_boost).min(3.0)
            }
            2 => {
                // BP
                let bw = 0.5 + (1.0 - resonance) * 2.0;
                let bp = 1.0 / (1.0 + (ratio.ln() / bw).powi(2));
                let peak_boost = if k > 0.01 { 1.0 + k * 0.3 } else { 1.0 };
                (bp * peak_boost).min(3.0)
            }
            _ => mag_lp,
        };

        let db = 20.0 * mag.max(0.001).log10();
        let db_norm = ((db + 48.0) / 60.0).clamp(0.0, 1.0);

        let x = rect.min.x + pad + t * inner_w;
        let y = rect.min.y + pad + (1.0 - db_norm) * inner_h;
        points.push(Pos2::new(x, y));
    }

    // Fill under curve
    let fill_col = Color32::from_rgba_premultiplied(55, 55, 55, 25);
    let mut fill_pts = points.clone();
    fill_pts.push(Pos2::new(rect.max.x - pad, rect.max.y - pad));
    fill_pts.push(Pos2::new(rect.min.x + pad, rect.max.y - pad));
    painter.add(egui::Shape::convex_polygon(
        fill_pts,
        fill_col,
        Stroke::NONE,
    ));

    // Response curve
    let line_col = Color32::from_gray(150);
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_col));
    }

    // Cutoff marker (vertical dashed line)
    let fc_t = (fc.ln() - log_min) / (log_max - log_min);
    let fc_x = rect.min.x + pad + fc_t.clamp(0.0, 1.0) * inner_w;
    painter.line_segment(
        [
            Pos2::new(fc_x, rect.min.y + pad),
            Pos2::new(fc_x, rect.max.y - pad),
        ],
        Stroke::new(1.0, Color32::from_gray(70)),
    );

    // Labels
    let label_col = theme::ASH;
    let font = egui::FontId::monospace(7.0);

    // Cutoff frequency
    let fc_label = if fc >= 1000.0 {
        format!("{:.1}kHz", fc / 1000.0)
    } else {
        format!("{:.0}Hz", fc)
    };
    painter.text(
        Pos2::new(fc_x, rect.min.y + pad + 1.0),
        egui::Align2::CENTER_TOP,
        &fc_label,
        font.clone(),
        label_col,
    );

    // Resonance
    painter.text(
        rect.right_bottom() + Vec2::new(-4.0, -3.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("Q {:.0}%", resonance * 100.0),
        font.clone(),
        label_col,
    );

    // Filter mode label
    let mode_name = match filter_mode {
        1 => "HP",
        2 => "BP",
        _ => "LP",
    };
    painter.text(
        rect.left_top() + Vec2::new(4.0, 3.0),
        egui::Align2::LEFT_TOP,
        mode_name,
        font.clone(),
        Color32::from_gray(70),
    );

    // Frequency scale markers
    for &(f, label) in &[(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
        let t = ((f as f32).ln() - log_min) / (log_max - log_min);
        if t > 0.05 && t < 0.95 {
            let x = rect.min.x + pad + t * inner_w;
            painter.line_segment(
                [
                    Pos2::new(x, rect.max.y - pad),
                    Pos2::new(x, rect.max.y - pad - 3.0),
                ],
                Stroke::new(0.5, Color32::from_gray(40)),
            );
            painter.text(
                Pos2::new(x, rect.max.y - pad + 1.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::monospace(6.0),
                Color32::from_gray(50),
            );
        }
    }
}
