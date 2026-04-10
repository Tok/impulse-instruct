// ─── ui/widgets/event_stream.rs ──────────────────────────────────────────────
// Real-time event stream / note history visualization.
// Scrolls right-to-left at BPM speed showing bass notes as Huth-colored circles,
// active ramps as gradient bars, and beat/bar grid lines.

use crate::state::AppState;
use crate::ui::theme;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Draw the event stream visualization.
/// Shows recent and upcoming note events scrolling right-to-left at tempo.
/// `smooth_step`: fractional step position for sub-step smooth scrolling.
pub fn event_stream(ui: &mut Ui, state: &AppState, smooth_step: f64, width: f32, height: f32) {
    let size = Vec2::new(width, height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect); // clip to rect bounds

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

    // Only show notes while running or after at least one step has played.
    // Also track whether we should show future notes (upcoming) or only past.
    let is_playing = seq.running && state.global_step_count > 0;

    // Timing: how many steps fit in the display width
    let display_steps = steps as f32 * 2.0; // show 2 full patterns
    let step_w = inner_w / display_steps;

    // The "now" position is at 75% from left
    let now_frac = 0.75;
    let now_x = inner.min.x + inner_w * now_frac;

    // ── Auto-range: find min/max notes in all active patterns ───────────────
    let mut lo_note = 127u8;
    let mut hi_note = 0u8;
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
        for step in pattern.iter().take(steps) {
            if step.active {
                lo_note = lo_note.min(step.note);
                hi_note = hi_note.max(step.note);
            }
        }
    }
    // Add margin of ±3 semitones, minimum 12 semitone range
    if hi_note < lo_note {
        lo_note = 36;
        hi_note = 60;
    }
    let margin = 3u8;
    lo_note = lo_note.saturating_sub(margin);
    hi_note = hi_note.saturating_add(margin).min(127);
    if hi_note - lo_note < 12 {
        let mid = (lo_note + hi_note) / 2;
        lo_note = mid.saturating_sub(6);
        hi_note = mid.saturating_add(6).min(127);
    }
    let note_lo = lo_note as f32;
    let note_range = (hi_note - lo_note) as f32;
    let note_y = |note: u8| -> f32 {
        let n = (note as f32).clamp(note_lo, note_lo + note_range);
        inner.max.y - ((n - note_lo) / note_range) * (inner_h - 4.0) - 2.0
    };

    // ── Hz frequency scale (left edge) ───────────────────────────────────────
    if state.ui_prefs.stream_hz_scale {
        let hz_font = egui::FontId::monospace(6.0);
        // Draw Hz labels at musically meaningful intervals
        for &note in &[24u8, 36, 48, 60, 72, 84, 96] {
            if note >= lo_note && note <= hi_note {
                let y = note_y(note);
                let hz = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
                let label = if hz >= 1000.0 {
                    format!("{:.1}k", hz / 1000.0)
                } else {
                    format!("{:.0}", hz)
                };
                painter.text(
                    Pos2::new(inner.min.x + 1.0, y),
                    egui::Align2::LEFT_CENTER,
                    &label,
                    hz_font.clone(),
                    Color32::from_gray(30),
                );
                // Faint horizontal guide line
                painter.line_segment(
                    [Pos2::new(inner.min.x + 22.0, y), Pos2::new(inner.max.x, y)],
                    Stroke::new(0.3, Color32::from_gray(18)),
                );
            }
        }
    }

    // ── Beat / bar grid ─────────────────────────────────────────────────────
    let steps_per_beat = (steps as f32 / time_sig as f32).max(1.0);
    // Position within the current pattern cycle (0..steps, fractional).
    // When stopped, snap to current_step (no interpolation drift).
    let pos_in_pattern = if seq.running {
        (smooth_step as f32).rem_euclid(steps as f32)
    } else {
        current_step as f32
    };
    for i in 0..(display_steps as usize + 2) {
        let step_offset = i as f32 - pos_in_pattern;
        let x = now_x + step_offset * step_w;
        if x < inner.min.x - 1.0 || x > inner.max.x + 1.0 {
            continue;
        }
        // Determine absolute step index to check bar/beat alignment
        let abs_step = i % steps;
        let is_bar = abs_step == 0;
        let is_beat = (abs_step as f32 % steps_per_beat).abs() < 0.5;

        if is_bar {
            painter.line_segment(
                [Pos2::new(x, inner.min.y), Pos2::new(x, inner.max.y)],
                Stroke::new(1.0, Color32::from_gray(45)),
            );
        } else if is_beat {
            painter.line_segment(
                [Pos2::new(x, inner.min.y), Pos2::new(x, inner.max.y)],
                Stroke::new(0.5, Color32::from_gray(20)),
            );
        }
    }

    // ── "Now" cursor line ───────────────────────────────────────────────────
    painter.line_segment(
        [Pos2::new(now_x, inner.min.y), Pos2::new(now_x, inner.max.y)],
        Stroke::new(1.5, Color32::from_gray(70)),
    );

    // ── Bass note events (all voices) ───────────────────────────────────────
    // Skip rendering entirely if sequencer hasn't produced any sound yet
    if !is_playing && state.global_step_count == 0 {
        if seq.running {
            ui.ctx().request_repaint();
        }
        return;
    }
    let circle_r = (inner_h * 0.08).clamp(3.0, 8.0);
    if state.ui_prefs.stream_bass_notes {
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

                // Position: step relative to smooth current position within pattern
                let step_offset = step_idx as f32 - pos_in_pattern;
                let offsets = [
                    step_offset,
                    step_offset + steps as f32,
                    step_offset - steps as f32,
                ];
                for &off in &offsets {
                    let x = now_x + off * step_w;
                    if x < inner.min.x - circle_r || x > inner.max.x + circle_r {
                        continue;
                    }
                    // Only show notes that have already been played (left of now cursor).
                    // A small margin (2px) allows the note right at "now" to be visible.
                    if x > now_x + 2.0 {
                        continue;
                    }
                    let y = note_y(note);

                    let dist = (off.abs() / display_steps).clamp(0.0, 1.0);
                    let alpha = ((1.0 - dist * 0.7) * 255.0) as u8;

                    let fill =
                        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                    // Size: accent = 1.4x, gate scales 0.7x–1.0x
                    let gate_scale = 0.7 + step.gate * 0.3;
                    let r = circle_r * gate_scale * if step.accent { 1.4 } else { 1.0 };
                    painter.circle_filled(Pos2::new(x, y), r, fill);
                    // Accent: brighter outline
                    let stroke_w = if step.accent { 2.0 } else { 1.0 };
                    let stroke_a = if step.accent {
                        alpha
                    } else {
                        (alpha as f32 * 0.5) as u8
                    };
                    painter.circle_stroke(
                        Pos2::new(x, y),
                        r,
                        Stroke::new(stroke_w, Color32::from_rgba_unmultiplied(0, 0, 0, stroke_a)),
                    );

                    // Slide: line to next note
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
    } // stream_bass_notes

    // ── Drum hits (small dots along the bottom) ─────────────────────────────
    if state.ui_prefs.stream_drums {
        use crate::state::DrumVoice;
        let drum_y_base = inner.max.y - 6.0;
        let drum_layers: &[(DrumVoice, f32, Color32, f32)] = &[
            (DrumVoice::Kick808, 0.0, Color32::from_gray(200), 2.5),
            (DrumVoice::Snare808, -3.5, Color32::from_gray(160), 2.0),
            (DrumVoice::HihatClosed808, -7.0, Color32::from_gray(80), 1.5),
            (DrumVoice::HihatOpen808, -7.0, Color32::from_gray(120), 1.8),
            (DrumVoice::Clap909, -3.5, Color32::from_gray(180), 2.0),
        ];
        for (voice, y_off, color, radius) in drum_layers {
            if let Some(pattern) = seq.drum_patterns.get(voice) {
                let y = drum_y_base + y_off;
                for (step_idx, step) in pattern.iter().enumerate().take(steps) {
                    if !step.active {
                        continue;
                    }
                    let step_offset = step_idx as f32 - pos_in_pattern;
                    let offsets = [
                        step_offset,
                        step_offset + steps as f32,
                        step_offset - steps as f32,
                    ];
                    for &off in &offsets {
                        let x = now_x + off * step_w;
                        if x > now_x + 2.0 || x < inner.min.x - 2.0 {
                            continue;
                        }
                        let dist = (off.abs() / display_steps).clamp(0.0, 1.0);
                        let a = ((1.0 - dist * 0.6) * 255.0) as u8;
                        painter.circle_filled(
                            Pos2::new(x, y),
                            *radius,
                            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a),
                        );
                    }
                }
            }
        }
    }

    // ── Active ramps ────────────────────────────────────────────────────────
    if !state.ui_prefs.stream_ramps {
        // Skip ramp display
    } else {
        let ramp_y = inner.max.y - 3.0;
        for ramp in &state.llm.active_ramps {
            if ramp.total_global_steps == 0 {
                continue;
            }
            let elapsed = state
                .global_step_count
                .saturating_sub(ramp.start_global_step);
            let remaining = ramp.total_global_steps.saturating_sub(elapsed);
            if remaining == 0 {
                continue;
            }
            let ramp_steps = remaining as f32;
            let x_start = now_x;
            let x_end = (now_x + ramp_steps * step_w).min(inner.max.x);
            let t = elapsed as f32 / ramp.total_global_steps as f32;
            let brightness = (100.0 + t * 80.0) as u8;
            painter.line_segment(
                [Pos2::new(x_start, ramp_y), Pos2::new(x_end, ramp_y)],
                Stroke::new(2.0, Color32::from_gray(brightness)),
            );
            let short_param = ramp.param.split('.').next_back().unwrap_or(&ramp.param);
            painter.text(
                Pos2::new(x_start + 2.0, ramp_y - 4.0),
                egui::Align2::LEFT_BOTTOM,
                short_param,
                egui::FontId::monospace(6.0),
                Color32::from_gray(60),
            );
        }
    } // stream_ramps

    // ── Labels ──────────────────────────────────────────────────────────────
    let font = egui::FontId::monospace(6.5);
    painter.text(
        inner.left_top() + Vec2::new(2.0, 1.0),
        egui::Align2::LEFT_TOP,
        format!("{:.0}bpm {}/{} #{}", bpm, time_sig, 4, current_step),
        font.clone(),
        Color32::from_gray(50),
    );
    // Note range indicator with Hz
    let lo_hz = 440.0 * 2.0_f32.powf((lo_note as f32 - 69.0) / 12.0);
    let hi_hz = 440.0 * 2.0_f32.powf((hi_note as f32 - 69.0) / 12.0);
    painter.text(
        inner.right_top() + Vec2::new(-2.0, 1.0),
        egui::Align2::RIGHT_TOP,
        format!(
            "{} {:.0}Hz – {} {:.0}Hz",
            note_name_short(lo_note),
            lo_hz,
            note_name_short(hi_note),
            hi_hz
        ),
        font,
        Color32::from_gray(40),
    );

    if seq.running {
        ui.ctx().request_repaint();
    }
}

/// Short note name from MIDI number (e.g. 60 → "C4")
fn note_name_short(midi: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = (midi / 12) as i8 - 1;
    let name = NAMES[(midi % 12) as usize];
    format!("{}{}", name, octave)
}
