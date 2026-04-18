// ─── ui/widgets/event_stream.rs ──────────────────────────────────────────────
// Real-time event stream / note history visualization.
// Scrolls right-to-left at BPM speed showing bass notes as Huth-colored circles,
// active ramps as gradient bars, and beat/bar grid lines.

use crate::state::AppState;
use crate::state::sequencer_state::pattern_temperature_acc;
use crate::ui::theme;
use crate::ui::{DrumLogEntry, MelodicLogEntry};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

/// Sum the Huth warm/cold value across all enabled melodic voices in `state`,
/// weighted by gate × accent.  Returns NaN if no active note is found.
fn bank_temperature(state: &AppState) -> f32 {
    let seq = &state.sequencer;
    let mut sum = 0.0_f32;
    let mut wsum = 0.0_f32;
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
        let len = seq
            .bass_voice_steps
            .get(vi)
            .copied()
            .unwrap_or(seq.steps)
            .min(pattern.len());
        let (s, w) = pattern_temperature_acc(pattern, len, &theme::NOTE_TEMP);
        sum += s;
        wsum += w;
    }
    if state.an1x.enabled {
        let len = seq.an1x_steps.min(seq.an1x_pattern.len());
        let (s, w) = pattern_temperature_acc(&seq.an1x_pattern, len, &theme::NOTE_TEMP);
        sum += s;
        wsum += w;
    }
    if wsum > 0.0 {
        (sum / wsum).clamp(-1.0, 1.0)
    } else {
        f32::NAN
    }
}

/// Draw the event stream visualization.
/// Shows recent and upcoming note events scrolling right-to-left at tempo.
/// `smooth_global_step`: monotonic `AppState.global_step_count` plus the
/// fractional sub-step time, so the past-side log can be positioned
/// relative to absolute time instead of the per-cycle step counter.
/// `melodic_log`: frozen history of fired notes (rendered for the past
/// half of the strip — the future half still reads from the live pattern).
/// `temperature`: live Huth warm/cold value in [-1, +1] (NaN = unavailable).
pub fn event_stream(
    ui: &mut Ui,
    state: &AppState,
    smooth_global_step: f64,
    melodic_log: &std::collections::VecDeque<MelodicLogEntry>,
    drum_log: &std::collections::VecDeque<DrumLogEntry>,
    width: f32,
    height: f32,
    temperature: f32,
) {
    let size = Vec2::new(width, height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect); // clip to rect bounds

    // Recessed-screen background (matches oscilloscopes for visual continuity)
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));

    let pad = 2.0_f32;
    let inner = Rect::from_min_max(rect.min + Vec2::splat(pad), rect.max - Vec2::splat(pad));
    let inner_w = inner.width();
    let inner_h = inner.height();

    let seq = &state.sequencer;
    let bpm = seq.bpm;
    let steps = seq.steps;
    let time_sig = seq.time_sig_num as usize;
    let current_step = seq.current_step;

    // Show notes only while the sequencer is actively running.
    // When stopped, show grid but no notes (prevents stale pattern display).
    let show_notes = seq.running;

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
    // Also fold AN1X notes into the auto-range so the stream has something
    // to paint when the 303 bass voices are off (e.g. jump-up D&B style
    // with AN1X as the lead melodic voice).
    if state.an1x.enabled {
        let an1x_steps = seq.an1x_steps.min(seq.an1x_pattern.len());
        for step in seq.an1x_pattern.iter().take(an1x_steps) {
            if step.active {
                lo_note = lo_note.min(step.note);
                hi_note = hi_note.max(step.note);
            }
        }
    }
    // Hoover lead notes — same idea: melodic voice, contributes to the
    // displayed Hz range so its dots aren't clipped off the top/bottom.
    if state.hoover.enabled {
        let hoover_steps = seq.hoover_steps.min(seq.hoover_pattern.len());
        for step in seq.hoover_pattern.iter().take(hoover_steps) {
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
                let hz = crate::audio::dsp::midi_to_hz(note);
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
                    Stroke::new(0.3, theme::DEEP),
                );
            }
        }
    }

    // ── Beat / bar grid ─────────────────────────────────────────────────────
    let steps_per_beat = (steps as f32 / time_sig as f32).max(1.0);
    // Position within the current pattern cycle (0..steps, fractional).
    // Both global_step_count and current_step increment in lock-step, so
    // (global % steps) == (current_step % steps); using the global form
    // lets us share the same time base with the past-note log.
    let pos_in_pattern = if seq.running {
        smooth_global_step.rem_euclid(steps as f64) as f32
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
    if !show_notes {
        if seq.running {
            ui.ctx().request_repaint();
        }
        return;
    }
    let circle_r = (inner_h * 0.08).clamp(3.0, 8.0);
    // Helper: render a note dot at a given offset (in step units).
    let render_dot = |off: f32, note: u8, gate: f32, accent: bool| {
        let x = now_x + off * step_w;
        if x < inner.min.x - circle_r || x > inner.max.x + circle_r {
            return;
        }
        let y = note_y(note);
        let dist = (off.abs() / display_steps).clamp(0.0, 1.0);
        let alpha = ((1.0 - dist * 0.7) * 255.0) as u8;
        let color = theme::note_color(note);
        let gate_scale = 0.7 + gate * 0.3;
        let r = circle_r * gate_scale * if accent { 1.4 } else { 1.0 };
        theme::led_flat(&painter, Pos2::new(x, y), r, color, alpha as f32 / 255.0);
    };
    if state.ui_prefs.stream_bass_notes {
        // ── PAST: render fired-note history from the log (offset ≤ 0).
        // The log freezes what actually fired — pattern mutations after
        // the fact don't erase or shift visible past notes.
        for entry in melodic_log.iter() {
            let off = entry.fired_at as f64 - smooth_global_step;
            if off > 0.0 {
                continue;
            }
            let off = off as f32;
            if off < -display_steps {
                continue;
            }
            render_dot(off, entry.note, entry.gate, entry.accent);
        }

        // ── FUTURE: render upcoming notes from each voice's live pattern.
        // For each step we compute the *next-fire* offset (always > 0) so
        // we don't overlap with the log's past dots.
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
            let voice_steps = seq
                .bass_voice_steps
                .get(vi)
                .copied()
                .unwrap_or(steps)
                .max(1);
            let local_pos = pos_in_pattern.rem_euclid(voice_steps as f32);
            for (step_idx, step) in pattern.iter().enumerate().take(voice_steps) {
                if !step.active {
                    continue;
                }
                // "Next fire" offset — always > 0.
                let mut off = step_idx as f32 - local_pos;
                if off <= 0.0 {
                    off += voice_steps as f32;
                }
                if off > display_steps {
                    continue;
                }
                render_dot(off, step.note, step.gate, step.accent);

                // Slide: line to next active step in the same cycle.
                if step.slide
                    && let Some(next) = pattern.get(step_idx + 1)
                    && next.active
                {
                    let x = now_x + off * step_w;
                    let nx = x + step_w;
                    let y = note_y(step.note);
                    let ny = note_y(next.note);
                    let dist = (off / display_steps).clamp(0.0, 1.0);
                    let alpha = ((1.0 - dist * 0.7) * 255.0) as u8;
                    let color = theme::note_color(step.note);
                    let r =
                        circle_r * (0.7 + step.gate * 0.3) * if step.accent { 1.4 } else { 1.0 };
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

        // ── AN1X future notes (past comes from melodic_log).
        if state.an1x.enabled {
            let an1x_steps = seq.an1x_steps.min(seq.an1x_pattern.len()).max(1);
            let local_pos = pos_in_pattern.rem_euclid(an1x_steps as f32);
            for (step_idx, step) in seq.an1x_pattern.iter().enumerate().take(an1x_steps) {
                if !step.active {
                    continue;
                }
                let mut off = step_idx as f32 - local_pos;
                if off <= 0.0 {
                    off += an1x_steps as f32;
                }
                if off > display_steps {
                    continue;
                }
                render_dot(off, step.note, step.gate, step.accent);
            }
        }
        // ── Hoover future notes — same treatment.
        if state.hoover.enabled {
            let hoover_steps = seq.hoover_steps.min(seq.hoover_pattern.len()).max(1);
            let local_pos = pos_in_pattern.rem_euclid(hoover_steps as f32);
            for (step_idx, step) in seq.hoover_pattern.iter().enumerate().take(hoover_steps) {
                if !step.active {
                    continue;
                }
                let mut off = step_idx as f32 - local_pos;
                if off <= 0.0 {
                    off += hoover_steps as f32;
                }
                if off > display_steps {
                    continue;
                }
                render_dot(off, step.note, step.gate, step.accent);
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
        let layer_for = |voice: &DrumVoice| -> Option<&(DrumVoice, f32, Color32, f32)> {
            drum_layers.iter().find(|(v, _, _, _)| v == voice)
        };
        let draw_dot = |off: f32, y: f32, color: Color32, radius: f32| {
            let x = now_x + off * step_w;
            if x < inner.min.x - 2.0 || x > inner.max.x + 2.0 {
                return;
            }
            let dist = (off.abs() / display_steps).clamp(0.0, 1.0);
            let a = ((1.0 - dist * 0.6) * 255.0) as u8;
            theme::led_flat(&painter, Pos2::new(x, y), radius, color, a as f32 / 255.0);
        };
        // ── PAST: render fired-hit history from the log (off ≤ 0).
        for entry in drum_log.iter() {
            let off = entry.fired_at as f64 - smooth_global_step;
            if off > 0.0 {
                continue;
            }
            let off = off as f32;
            if off < -display_steps {
                continue;
            }
            if let Some((_, y_off, color, radius)) = layer_for(&entry.voice) {
                draw_dot(off, drum_y_base + *y_off, *color, *radius);
            }
        }
        // ── FUTURE: next-fire offset from the live pattern (off > 0).
        for (voice, y_off, color, radius) in drum_layers {
            if let Some(pattern) = seq.drum_patterns.get(voice) {
                let y = drum_y_base + *y_off;
                for (step_idx, step) in pattern.iter().enumerate().take(steps) {
                    if !step.active {
                        continue;
                    }
                    let mut off = step_idx as f32 - pos_in_pattern;
                    if off <= 0.0 {
                        off += steps as f32;
                    }
                    if off > display_steps {
                        continue;
                    }
                    draw_dot(off, y, *color, *radius);
                }
            }
        }
    }

    // ── Stereo/pan position indicators ────────────────────────────────────
    if state.ui_prefs.stream_stereo {
        let band_h = 8.0_f32;
        let band_top = inner.max.y - 16.0;
        let band_mid = band_top + band_h * 0.5;
        let center_x = (inner.min.x + inner.max.x) * 0.5;
        // Center line (faint)
        painter.line_segment(
            [
                Pos2::new(center_x, band_top),
                Pos2::new(center_x, band_top + band_h),
            ],
            Stroke::new(0.5, Color32::from_gray(30)),
        );
        // Bass voice pans
        for voice in &state.bass_voices {
            if !voice.enabled {
                continue;
            }
            let pan_x = center_x + voice.synth.pan * (inner.width() * 0.4);
            painter.circle_filled(
                Pos2::new(pan_x, band_mid - 1.5),
                1.5,
                Color32::from_gray(70),
            );
        }
        // Drum kit pans (kick and snare only for readability)
        let kit_pan = |p: f32, y_off: f32| {
            let px = center_x + p * (inner.width() * 0.4);
            painter.circle_filled(Pos2::new(px, band_mid + y_off), 1.0, Color32::from_gray(50));
        };
        kit_pan(state.kit_a.kick.pan, 1.5);
        kit_pan(state.kit_a.snare.pan, 1.5);
        // L/R labels
        painter.text(
            Pos2::new(inner.min.x + 2.0, band_top),
            egui::Align2::LEFT_TOP,
            "L",
            egui::FontId::monospace(5.5),
            Color32::from_gray(35),
        );
        painter.text(
            Pos2::new(inner.max.x - 2.0, band_top),
            egui::Align2::RIGHT_TOP,
            "R",
            egui::FontId::monospace(5.5),
            Color32::from_gray(35),
        );
    }

    // ── Active ramps ────────────────────────────────────────────────────────
    // Drawn as a sloped line across the ramp band so the shape of the
    // transition is visible (rising / falling / amount). Param value 0 maps
    // to the bottom of the band, 1 to the top.
    if !state.ui_prefs.stream_ramps {
        // Skip ramp display
    } else {
        let band_bot = inner.max.y - 2.0;
        let band_top = inner.max.y - 16.0;
        let band_h = band_bot - band_top;
        let val_to_y = |v: f32| band_bot - v.clamp(0.0, 1.0) * band_h;
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
            let t = elapsed as f32 / ramp.total_global_steps as f32;
            let current_val = ramp.from + (ramp.target - ramp.from) * t;

            let x_start = now_x;
            let x_end = (now_x + remaining as f32 * step_w).min(inner.max.x);
            let y_start = val_to_y(current_val);
            let y_end = val_to_y(ramp.target);

            let brightness = (120.0 + t * 80.0) as u8;
            let stroke_color = Color32::from_gray(brightness);

            // Faint guide line at the target value so the ramp reads as going "to" it.
            painter.line_segment(
                [Pos2::new(x_start, y_end), Pos2::new(x_end, y_end)],
                Stroke::new(0.5, Color32::from_gray(50)),
            );
            // The ramp itself.
            painter.line_segment(
                [Pos2::new(x_start, y_start), Pos2::new(x_end, y_end)],
                Stroke::new(1.5, stroke_color),
            );
            // Current value dot + target dot.
            painter.circle_filled(Pos2::new(x_start, y_start), 1.8, stroke_color);
            painter.circle_filled(Pos2::new(x_end, y_end), 1.6, Color32::from_gray(150));

            let short_param = ramp.param.split('.').next_back().unwrap_or(&ramp.param);
            painter.text(
                Pos2::new(x_start + 3.0, band_top - 1.0),
                egui::Align2::LEFT_BOTTOM,
                short_param,
                egui::FontId::monospace(6.0),
                Color32::from_gray(80),
            );
        }
    } // stream_ramps

    // ── Labels ──────────────────────────────────────────────────────────────
    let font = egui::FontId::monospace(6.5);
    // Fixed-width formatting so the TEMP strip doesn't jitter when the step
    // counter rolls past 9 or BPM digits change.
    let bpm_label = format!("{:>3.0}bpm {}/{} #{:>2}", bpm, time_sig, 4, current_step);
    // Reserve space for the Hz scale column on the left when it's enabled.
    let header_x_pad = if state.ui_prefs.stream_hz_scale {
        26.0
    } else {
        2.0
    };
    painter.text(
        inner.left_top() + Vec2::new(header_x_pad, 1.0),
        egui::Align2::LEFT_TOP,
        &bpm_label,
        font.clone(),
        Color32::from_gray(50),
    );

    // ── Huth temperature strip (Warme/Kalte Töne) ─────────────────────────
    // Live needle (top) = master audio out (spectrum-derived).
    // Static tick (below) = melody bank intent (pattern notes × gate × accent).
    let bank_temp = bank_temperature(state);
    if temperature.is_finite() || bank_temp.is_finite() {
        let bpm_w = painter
            .layout_no_wrap(bpm_label.clone(), font.clone(), Color32::WHITE)
            .size()
            .x;
        // Reserve space for "TEMP" label + numeric value + right-side note range.
        let label_text = "TEMP";
        let label_w = painter
            .layout_no_wrap(label_text.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x;
        let strip_w =
            70.0_f32.min((inner.width() - bpm_w - label_w - header_x_pad - 230.0).max(0.0));
        if strip_w > 12.0 {
            let strip_h = 5.0_f32;
            let label_x = inner.min.x + header_x_pad + bpm_w + 8.0;
            let strip_x = label_x + label_w + 4.0;
            let strip_y = inner.min.y + 2.0;
            let strip_rect =
                Rect::from_min_size(Pos2::new(strip_x, strip_y), Vec2::new(strip_w, strip_h));
            // "TEMP" label
            painter.text(
                Pos2::new(label_x, strip_y - 1.0),
                egui::Align2::LEFT_TOP,
                label_text,
                font.clone(),
                Color32::from_gray(70),
            );
            // Gradient: cold-blue → neutral → warm-orange.
            let band_count = 14;
            for i in 0..band_count {
                let t = -1.0 + 2.0 * (i as f32 / (band_count - 1) as f32);
                let band_x = strip_x + (i as f32 / band_count as f32) * strip_w;
                let band_w = (strip_w / band_count as f32) + 0.5;
                let mut col = theme::temperature_color(t);
                col = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 180);
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(band_x, strip_y), Vec2::new(band_w, strip_h)),
                    egui::Rounding::ZERO,
                    col,
                );
            }
            // Frame
            painter.rect_stroke(
                strip_rect,
                egui::Rounding::ZERO,
                Stroke::new(0.5, Color32::from_gray(60)),
            );
            // Static (bank) tick below the strip — small filled triangle.
            if bank_temp.is_finite() {
                let bx = strip_x + (bank_temp.clamp(-1.0, 1.0) * 0.5 + 0.5) * strip_w;
                let by = strip_y + strip_h + 0.5;
                let tri = vec![
                    Pos2::new(bx, by),
                    Pos2::new(bx - 2.5, by + 3.0),
                    Pos2::new(bx + 2.5, by + 3.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    tri,
                    Color32::from_gray(180),
                    Stroke::NONE,
                ));
            }
            // Live needle at spectrum temperature position.
            if temperature.is_finite() {
                let n_t = temperature.clamp(-1.0, 1.0);
                let nx = strip_x + (n_t * 0.5 + 0.5) * strip_w;
                painter.line_segment(
                    [
                        Pos2::new(nx, strip_y - 1.0),
                        Pos2::new(nx, strip_y + strip_h + 1.0),
                    ],
                    Stroke::new(1.2, Color32::from_gray(235)),
                );
            }
            // Numeric value (live if available, else bank).
            let display_t = if temperature.is_finite() {
                temperature.clamp(-1.0, 1.0)
            } else {
                bank_temp.clamp(-1.0, 1.0)
            };
            painter.text(
                Pos2::new(strip_x + strip_w + 4.0, strip_y - 1.0),
                egui::Align2::LEFT_TOP,
                format!("{:+.2}", display_t),
                font.clone(),
                theme::temperature_color(display_t),
            );
            // Hover tooltip explaining the two markers.
            let hit_rect = Rect::from_min_max(
                Pos2::new(label_x, strip_y - 2.0),
                Pos2::new(strip_x + strip_w + 32.0, strip_y + strip_h + 5.0),
            );
            let hit = ui.interact(
                hit_rect,
                egui::Id::new("event_stream_temp_strip"),
                Sense::hover(),
            );
            if hit.hovered() {
                hit.on_hover_ui(|ui| {
                    ui.label("Huth Warme / Kalte Töne");
                    ui.separator();
                    ui.label("Needle: live master-out timbre");
                    ui.label("▲ tick: melody-bank intent (pattern notes)");
                    let live_str = if temperature.is_finite() {
                        format!("{:+.2}", temperature)
                    } else {
                        "—".to_string()
                    };
                    let bank_str = if bank_temp.is_finite() {
                        format!("{:+.2}", bank_temp)
                    } else {
                        "—".to_string()
                    };
                    ui.small(format!("live {live_str}   bank {bank_str}"));
                });
            }
        }
    }

    // Note range indicator with Hz
    let lo_hz = crate::audio::dsp::midi_to_hz(lo_note);
    let hi_hz = crate::audio::dsp::midi_to_hz(hi_note);
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
