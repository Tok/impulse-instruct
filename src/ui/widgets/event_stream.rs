// ─── ui/widgets/event_stream.rs ──────────────────────────────────────────────
// Real-time event stream / note history visualization.
// Scrolls right-to-left at BPM speed showing bass notes as Huth-colored circles,
// active ramps as gradient bars, and beat/bar grid lines.

use crate::state::AppState;
use crate::ui::theme;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Draw the event stream visualization.
/// Shows recent and upcoming note events scrolling right-to-left at tempo.
pub fn event_stream(ui: &mut Ui, state: &AppState, width: f32, height: f32) {
    let size = Vec2::new(width, height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter();

    // Background
    painter.rect_filled(rect, egui::Rounding::same(2.0), Color32::from_gray(8));
    painter.rect_stroke(
        rect,
        egui::Rounding::same(2.0),
        Stroke::new(1.0, Color32::from_gray(25)),
    );

    let pad = 2.0_f32;
    let inner = Rect::from_min_max(rect.min + Vec2::splat(pad), rect.max - Vec2::splat(pad));
    let inner_w = inner.width();
    let inner_h = inner.height();

    let seq = &state.sequencer;
    let bpm = seq.bpm;
    let steps = seq.steps;
    let time_sig = seq.time_sig_num as usize;
    let current_step = seq.current_step;

    // Timing: how many steps fit in the display width
    let display_steps = steps as f32 * 2.0; // show 2 full patterns
    let step_w = inner_w / display_steps;

    // The "now" position is at 75% from left (shows some past, mostly future/upcoming)
    let now_frac = 0.75;
    let now_x = inner.min.x + inner_w * now_frac;

    // ── Beat / bar grid ─────────────────────────────────────────────────────
    let steps_per_beat = (steps as f32 / time_sig as f32).max(1.0);
    for i in 0..(display_steps as usize + 1) {
        let step_offset = i as f32 - (current_step as f32 % display_steps);
        let x = now_x + step_offset * step_w - (current_step as f32).fract() * step_w;
        if x < inner.min.x || x > inner.max.x {
            continue;
        }

        let abs_step = i;
        let is_bar = abs_step % steps == 0;
        let is_beat = (abs_step as f32 % steps_per_beat).abs() < 0.5;

        if is_bar {
            painter.line_segment(
                [Pos2::new(x, inner.min.y), Pos2::new(x, inner.max.y)],
                Stroke::new(1.0, Color32::from_gray(45)),
            );
        } else if is_beat {
            painter.line_segment(
                [Pos2::new(x, inner.min.y), Pos2::new(x, inner.max.y)],
                Stroke::new(0.5, Color32::from_gray(25)),
            );
        }
    }

    // ── "Now" cursor line ───────────────────────────────────────────────────
    painter.line_segment(
        [Pos2::new(now_x, inner.min.y), Pos2::new(now_x, inner.max.y)],
        Stroke::new(1.5, Color32::from_gray(70)),
    );

    // ── Note range for Y mapping ────────────────────────────────────────────
    // Map MIDI notes to Y: higher notes higher up. Range: 24 (C1) to 72 (C5).
    let note_min = 24.0_f32;
    let note_range = 48.0_f32;
    let note_y = |note: u8| -> f32 {
        let n = (note as f32).clamp(note_min, note_min + note_range);
        inner.max.y - ((n - note_min) / note_range) * inner_h
    };

    // ── Bass note events (all voices) ───────────────────────────────────────
    let circle_r = (inner_h * 0.08).clamp(3.0, 8.0);
    for (vi, voice) in state.bass_voices.iter().enumerate() {
        if !voice.enabled {
            continue;
        }
        let pattern = if vi == 0 {
            &seq.bass_pattern
        } else if let Some(p) = seq.bass_patterns.get(vi) {
            p
        } else {
            continue;
        };

        for (step_idx, step) in pattern.iter().enumerate().take(steps) {
            if !step.active {
                continue;
            }
            let note = step.note;
            let color = theme::note_color(note);

            // Position: step relative to current
            let step_offset = step_idx as f32 - current_step as f32;
            // Wrap around for the second pattern display
            let offsets = [step_offset, step_offset + steps as f32];
            for &off in &offsets {
                let x = now_x + off * step_w;
                if x < inner.min.x - circle_r || x > inner.max.x + circle_r {
                    continue;
                }
                let y = note_y(note);

                // Fade notes further from "now"
                let dist = (off.abs() / display_steps).clamp(0.0, 1.0);
                let alpha = ((1.0 - dist * 0.7) * 255.0) as u8;

                // Circle with thick dark border (Huth colored fill)
                let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                let r = if step.accent {
                    circle_r * 1.3
                } else {
                    circle_r
                };
                painter.circle_filled(Pos2::new(x, y), r, fill);
                painter.circle_stroke(
                    Pos2::new(x, y),
                    r,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(0, 0, 0, alpha)),
                );

                // Slide indicator: small line connecting to next note
                if step.slide
                    && step_idx + 1 < steps
                    && let Some(next) = pattern.get(step_idx + 1)
                    && next.active
                {
                    let nx = x + step_w;
                    let ny = note_y(next.note);
                    painter.line_segment(
                        [Pos2::new(x + r, y), Pos2::new(nx - circle_r, ny)],
                        Stroke::new(
                            1.0,
                            Color32::from_rgba_unmultiplied(
                                color.r(),
                                color.g(),
                                color.b(),
                                alpha / 2,
                            ),
                        ),
                    );
                }
            }
        }
    }

    // ── Active ramps ────────────────────────────────────────────────────────
    let ramp_y = inner.max.y - 3.0;
    for ramp in &state.llm.active_ramps {
        if ramp.total_global_steps == 0 {
            continue; // cycle-based ramps don't have step timing
        }
        let elapsed = state
            .global_step_count
            .saturating_sub(ramp.start_global_step);
        let remaining = ramp.total_global_steps.saturating_sub(elapsed);
        if remaining == 0 {
            continue;
        }
        // Show ramp as a horizontal bar from now to completion
        let ramp_steps = remaining as f32;
        let x_start = now_x;
        let x_end = (now_x + ramp_steps * step_w).min(inner.max.x);
        let t = elapsed as f32 / ramp.total_global_steps as f32;
        let brightness = (100.0 + t * 80.0) as u8;
        painter.line_segment(
            [Pos2::new(x_start, ramp_y), Pos2::new(x_end, ramp_y)],
            Stroke::new(2.0, Color32::from_gray(brightness)),
        );
        // Ramp target label
        let short_param = ramp.param.split('.').next_back().unwrap_or(&ramp.param);
        painter.text(
            Pos2::new(x_start + 2.0, ramp_y - 4.0),
            egui::Align2::LEFT_BOTTOM,
            short_param,
            egui::FontId::monospace(6.0),
            Color32::from_gray(60),
        );
    }

    // ── Labels ──────────────────────────────────────────────────────────────
    let font = egui::FontId::monospace(6.5);
    // BPM + step counter
    painter.text(
        inner.left_top() + Vec2::new(2.0, 1.0),
        egui::Align2::LEFT_TOP,
        format!("{:.0}bpm  #{}", bpm, current_step),
        font.clone(),
        Color32::from_gray(50),
    );
    // Global step count
    painter.text(
        inner.right_top() + Vec2::new(-2.0, 1.0),
        egui::Align2::RIGHT_TOP,
        format!("g{}", state.global_step_count),
        font,
        Color32::from_gray(40),
    );

    // Request repaint for animation
    if seq.running {
        ui.ctx().request_repaint();
    }
}
