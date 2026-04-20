// ─── ui/widgets/waveform_viz.rs ──────────────────────────────────────────────
// Small animated waveform visualizations for oscillators and LFOs.

use egui::{Color32, Pos2, Sense, Stroke, Ui, Vec2};

use crate::ui::theme;

/// Draw an animated oscillator waveform icon.
/// `kind`: 0=Saw, 1=Square, 2=Supersaw (detuned saws), 3=Sine, 4=Triangle
/// `width`/`height`: pixel dimensions (typically 60–100 × 30–50).
/// The waveform scrolls at ~2 Hz to show it's alive.
pub fn waveform_icon(ui: &mut Ui, kind: u8, width: f32, height: f32) {
    let size = Vec2::new(width, height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
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

    let pad = 2.0_f32;
    let inner_w = rect.width() - pad * 2.0;
    let inner_h = rect.height() - pad * 2.0;
    let mid_y = rect.min.y + pad + inner_h * 0.5;

    // Animate: phase offset scrolls over time
    let time = ui.ctx().input(|i| i.time) as f32;
    let phase_offset = time * 2.0; // 2 Hz scroll

    let n_points = 96;
    let cycles = 2.5; // show ~2.5 cycles
    let amp = inner_h * 0.42;

    let mut points = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let t = i as f32 / (n_points - 1) as f32;
        let phase = (t * cycles + phase_offset).fract();

        let y_norm = match kind {
            0 => {
                // Saw: ramp from -1 to +1
                phase * 2.0 - 1.0
            }
            1 => {
                // Square
                if phase < 0.5 { 1.0 } else { -1.0 }
            }
            2 => {
                // Supersaw: 3 detuned saws summed
                let s1 = (phase * 2.0 - 1.0) * 0.5;
                let s2 = ((phase * 1.02 + 0.33).fract() * 2.0 - 1.0) * 0.3;
                let s3 = ((phase * 0.98 + 0.67).fract() * 2.0 - 1.0) * 0.3;
                (s1 + s2 + s3).clamp(-1.0, 1.0)
            }
            3 => {
                // Sine
                (phase * std::f32::consts::TAU).sin()
            }
            4 => {
                // Triangle
                let p = phase * 4.0;
                if p < 1.0 {
                    p
                } else if p < 3.0 {
                    2.0 - p
                } else {
                    p - 4.0
                }
            }
            _ => 0.0,
        };

        let x = rect.min.x + pad + t * inner_w;
        let y = mid_y - y_norm * amp;
        points.push(Pos2::new(x, y));
    }

    // Draw center line (zero crossing)
    painter.line_segment(
        [
            Pos2::new(rect.min.x + pad, mid_y),
            Pos2::new(rect.max.x - pad, mid_y),
        ],
        Stroke::new(0.5, Color32::from_gray(30)),
    );

    // Draw waveform
    let line_col = Color32::from_gray(160);
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_col));
    }

    // Request repaint for animation
    ui.ctx().request_repaint();
}

/// Draw an animated LFO waveform preview with rate/depth indicators.
/// `shape`: 0=Sine, 1=Triangle, 2=Saw, 3=InvSaw, 4=Square, 5=S&H
/// `rate`: 0–1 (maps to 0.01–20 Hz for display)
/// `depth`: 0–1 modulation depth
/// `target_name`: short label for the mod target (e.g. "CUT", "VOL")
pub fn lfo_preview(
    ui: &mut Ui,
    shape: u8,
    rate: f32,
    depth: f32,
    target_name: &str,
    width: f32,
    height: f32,
) {
    let size = Vec2::new(width, height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
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

    let pad = 2.0_f32;
    let inner_w = rect.width() - pad * 2.0;
    let inner_h = rect.height() - pad * 2.0;
    let mid_y = rect.min.y + pad + inner_h * 0.5;

    let time = ui.ctx().input(|i| i.time) as f32;
    let rate_hz = 0.01 + rate * 19.99;
    let phase_offset = time * rate_hz;

    let amp = inner_h * 0.42 * depth;

    let n_points = 96;
    let display_cycles = 3.0;
    let mut points = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let t = i as f32 / (n_points - 1) as f32;
        let phase = (t * display_cycles + phase_offset).fract();

        let y_norm = match shape {
            0 => (phase * std::f32::consts::TAU).sin(), // Sine
            1 => {
                // Triangle
                let p = phase * 4.0;
                if p < 1.0 {
                    p
                } else if p < 3.0 {
                    2.0 - p
                } else {
                    p - 4.0
                }
            }
            2 => phase * 2.0 - 1.0, // Saw
            3 => 1.0 - phase * 2.0, // InvSaw
            4 => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            } // Square
            5 => {
                // S&H: step at each cycle boundary
                let step = (t * display_cycles + phase_offset) as u32;
                let hash = step.wrapping_mul(2654435761);
                (hash as f32 / u32::MAX as f32) * 2.0 - 1.0
            }
            _ => 0.0,
        };

        let x = rect.min.x + pad + t * inner_w;
        let y = mid_y - y_norm * amp;
        points.push(Pos2::new(x, y));
    }

    // Center line
    painter.line_segment(
        [
            Pos2::new(rect.min.x + pad, mid_y),
            Pos2::new(rect.max.x - pad, mid_y),
        ],
        Stroke::new(0.5, Color32::from_gray(30)),
    );

    // Depth bracket lines (show mod range)
    if depth > 0.01 {
        let bracket_col = Color32::from_gray(25);
        painter.line_segment(
            [
                Pos2::new(rect.min.x + pad, mid_y - amp),
                Pos2::new(rect.max.x - pad, mid_y - amp),
            ],
            Stroke::new(0.5, bracket_col),
        );
        painter.line_segment(
            [
                Pos2::new(rect.min.x + pad, mid_y + amp),
                Pos2::new(rect.max.x - pad, mid_y + amp),
            ],
            Stroke::new(0.5, bracket_col),
        );
    }

    // Draw waveform
    let line_col = if depth > 0.01 {
        Color32::from_gray(140)
    } else {
        Color32::from_gray(50)
    };
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], Stroke::new(1.5, line_col));
    }

    // Labels
    let label_col = Color32::from_gray(80);
    let font = egui::FontId::monospace(6.5);

    // Rate in Hz
    painter.text(
        rect.left_bottom() + Vec2::new(3.0, -2.0),
        egui::Align2::LEFT_BOTTOM,
        format!("{:.1}Hz", rate_hz),
        font.clone(),
        label_col,
    );

    // Depth %
    painter.text(
        rect.right_bottom() + Vec2::new(-3.0, -2.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("D{:.0}%", depth * 100.0),
        font.clone(),
        label_col,
    );

    // Target name
    if !target_name.is_empty() {
        painter.text(
            rect.right_top() + Vec2::new(-3.0, 2.0),
            egui::Align2::RIGHT_TOP,
            target_name,
            font,
            theme::IRON,
        );
    }

    if depth > 0.01 {
        ui.ctx().request_repaint();
    }
}
