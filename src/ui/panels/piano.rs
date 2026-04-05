// ─── ui/panels/piano.rs ───────────────────────────────────────────────────────
// Piano keyboard widget.

use crate::audio::AudioCommand;
use crate::sequencer::TriggerEvent;
use crate::state::{record_bass_note, scale_degree};
use crate::ui::{ImpulseApp, note_name, theme};

pub fn draw_piano(app: &mut ImpulseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

    // Range: C1 (MIDI 24) → C8 (MIDI 108) = 7 octaves + high C
    const START_NOTE: u8 = 24;
    const END_NOTE: u8 = 108; // inclusive
    const N_OCTAVES: usize = 7;
    const N_WHITE: usize = N_OCTAVES * 7 + 1; // 50 white keys (including C8)

    // Which semitones are white keys?
    const fn is_white(semitone: u8) -> bool {
        matches!(semitone, 0 | 2 | 4 | 5 | 7 | 9 | 11)
    }

    // Black key center positions in white-key units from octave start (C=0).
    // Each value sits at the boundary between two adjacent white keys so the
    // black key straddles the gap, matching a real piano layout.
    const BLACK_KEYS: &[(u8, f32)] = &[
        (1, 1.0),  // C# — between C and D
        (3, 2.0),  // D# — between D and E
        (6, 4.0),  // F# — between F and G
        (8, 5.0),  // G# — between G and A
        (10, 6.0), // A# — between A and B
    ];

    let available_w = ui.available_width();
    // Keys fill available width; clamp min so very narrow windows stay usable.
    // Target ~6:1 height:width — real piano keys are ≈23 mm wide × 150 mm tall.
    let wk_w = (available_w / N_WHITE as f32).max(7.0);
    let wk_h = (wk_w * 6.2).clamp(52.0, 88.0);
    let bk_w = wk_w * 0.58;
    let bk_h = wk_h * 0.62;
    let total_w = wk_w * N_WHITE as f32;

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(total_w, wk_h), Sense::click_and_drag());

    // Detect click/drag position for interactive playing
    let click_pos: Option<Pos2> = if response.is_pointer_button_down_on() || response.dragged() {
        ctx.input(|i| i.pointer.interact_pos())
    } else {
        None
    };
    let mut clicked_note: Option<u8> = None;

    // Key / scale for degree highlighting
    let (root_note, seq_scale, huth_on) = {
        let s = app.state.read();
        let huth_on = s.ui_prefs.huth_style != crate::state::HuthStyle::Off;
        (s.sequencer.root_note, s.sequencer.scale, huth_on)
    };

    // Sequencer cursor note (for highlighting)
    let (seq_note, seq_running) = {
        let s = app.state.read();
        let step = s.sequencer.current_step;
        let running = s.sequencer.running;
        if running
            && s.sequencer
                .bass_pattern
                .get(step)
                .map(|b| b.active)
                .unwrap_or(false)
        {
            (Some(s.sequencer.bass_pattern[step].note), true)
        } else {
            (None, running)
        }
    };
    let _ = seq_running;

    if !ui.is_rect_visible(rect) {
        return;
    }

    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, theme::VOID);

    let ox = rect.min.x; // origin x
    let oy = rect.min.y;

    let show_label = app.piano_show_labels;

    // ── White keys ────────────────────────────────────────────────────────────
    // Collect active white key bloom data so we can draw blooms AFTER all white
    // key fills — otherwise the rightward bloom gets covered by the next key's fill.
    let mut white_blooms: Vec<(Rect, Color32)> = Vec::new();

    let wk_rounding = egui::Rounding {
        nw: 1.0,
        ne: 1.0,
        sw: 3.0,
        se: 3.0,
    };

    let mut white_idx = 0usize;
    for note in START_NOTE..=END_NOTE {
        let semi = note % 12;
        if !is_white(semi) {
            continue;
        }

        let x = ox + white_idx as f32 * wk_w;
        let key_rect = Rect::from_min_size(Pos2::new(x + 0.5, oy), Vec2::new(wk_w - 1.0, wk_h));

        let pressed = app.pressed_notes.contains(&note);
        let seq_active = seq_note == Some(note);
        let active = pressed || seq_active;

        let hue = if huth_on {
            theme::note_color(note)
        } else {
            Color32::from_gray(200)
        };
        let fill = if active {
            if huth_on {
                theme::lerp_color(hue, theme::CHALK, 0.35)
            } else {
                Color32::from_gray(240)
            }
        } else {
            if huth_on {
                theme::lerp_color(hue, Color32::from_rgb(48, 48, 48), 0.60)
            } else {
                Color32::from_gray(200)
            }
        };

        if active {
            white_blooms.push((key_rect, hue));
        }

        painter.rect_filled(key_rect, wk_rounding, fill);

        // Neumorphic edges
        let (hi, sh) = if active {
            (Color32::from_gray(130), Color32::from_gray(10))
        } else {
            (Color32::from_gray(85), Color32::from_gray(22))
        };
        painter.line_segment(
            [key_rect.left_top(), key_rect.right_top()],
            Stroke::new(1.0, hi),
        );
        painter.line_segment(
            [key_rect.left_top(), key_rect.left_bottom()],
            Stroke::new(1.0, hi),
        );
        painter.line_segment(
            [key_rect.left_bottom(), key_rect.right_bottom()],
            Stroke::new(1.0, sh),
        );
        painter.line_segment(
            [key_rect.right_top(), key_rect.right_bottom()],
            Stroke::new(0.5, sh),
        );
        let groove_y = key_rect.min.y + wk_h * 0.08;
        painter.line_segment(
            [
                Pos2::new(key_rect.min.x + 1.5, groove_y),
                Pos2::new(key_rect.max.x - 1.0, groove_y),
            ],
            Stroke::new(0.5, Color32::from_gray(if active { 170 } else { 72 })),
        );

        // Scale degree dot — small marker near the top of scale-tone keys
        if !active && let Some(degree) = scale_degree(note, root_note, seq_scale) {
            let dot_y = key_rect.min.y + wk_h * 0.18;
            let dot_x = key_rect.center().x;
            let (dot_r, dot_alpha) = if degree == 0 {
                (2.8, 200u8) // tonic — brighter, larger
            } else if degree == 2 || degree == 4 {
                (2.0, 120u8) // 3rd / 5th — medium
            } else {
                (1.5, 70u8) // other degrees — subtle
            };
            painter.circle_filled(
                Pos2::new(dot_x, dot_y),
                dot_r,
                Color32::from_rgba_unmultiplied(200, 200, 200, dot_alpha),
            );
        }

        // Labels: C notes always visible; all notes when show_label is on
        let is_c = semi == 0;
        if is_c || show_label {
            let lbl = note_name(note);
            let lbl_text = if is_c { lbl } else { &lbl[..lbl.len() - 1] };
            let font_size = if is_c { 8.5 } else { 7.5 };
            let label_pos = Pos2::new(x + wk_w * 0.5, oy + wk_h - 10.0);
            let pill = Rect::from_center_size(
                label_pos,
                egui::Vec2::new(lbl_text.len() as f32 * 5.5 + 4.0, 11.0),
            );
            painter.rect_filled(
                pill,
                egui::Rounding::same(2.0),
                Color32::from_black_alpha(140),
            );
            let label_color = if active { theme::CHALK } else { theme::SMOKE };
            painter.text(
                label_pos,
                egui::Align2::CENTER_CENTER,
                lbl_text,
                egui::FontId::monospace(font_size),
                label_color,
            );
        }

        if let Some(cp) = click_pos
            && key_rect.contains(cp)
        {
            clicked_note = Some(note);
        }

        white_idx += 1;
    }

    // Draw white key blooms now — after all fills, so glow is symmetric on both sides.
    // Black keys (drawn next) will naturally sit on top, which is correct visually.
    for (key_rect, hue) in &white_blooms {
        let [r, g, b, _] = hue.to_array();
        for i in 1..=4u8 {
            let expand = i as f32 * 2.2;
            let alpha = 52u8.saturating_sub(i * 11);
            painter.rect_filled(
                key_rect.expand(expand),
                wk_rounding,
                Color32::from_rgba_unmultiplied(r, g, b, alpha),
            );
        }
    }

    // ── Black keys (drawn on top) ─────────────────────────────────────────────
    for oct in 0..N_OCTAVES {
        for &(semi, wk_off) in BLACK_KEYS {
            let note = START_NOTE + oct as u8 * 12 + semi;
            if note > END_NOTE {
                continue;
            }

            let white_oct_start = ox + oct as f32 * 7.0 * wk_w;
            let x = white_oct_start + wk_off * wk_w - bk_w * 0.5;
            let key_rect = Rect::from_min_size(Pos2::new(x, oy), Vec2::new(bk_w, bk_h));

            let pressed = app.pressed_notes.contains(&note);
            let seq_active = seq_note == Some(note);
            let active = pressed || seq_active;

            let bk_hue = if huth_on {
                theme::note_color(note)
            } else {
                Color32::from_gray(30)
            };
            let fill = if active {
                if huth_on {
                    theme::lerp_color(bk_hue, theme::CHALK, 0.22)
                } else {
                    Color32::from_gray(80)
                }
            } else {
                if huth_on {
                    theme::lerp_color(bk_hue, theme::PIT, 0.72)
                } else {
                    Color32::from_gray(22)
                }
            };

            let bk_rounding = egui::Rounding {
                nw: 1.0,
                ne: 1.0,
                sw: 3.0,
                se: 3.0,
            };

            // Bloom for active black keys
            if active {
                let [r, g, b, _] = bk_hue.to_array();
                for i in 1..=3u8 {
                    let expand = i as f32 * 2.0;
                    let alpha = 45 - i * 13;
                    painter.rect_filled(
                        key_rect.expand(expand),
                        bk_rounding,
                        Color32::from_rgba_unmultiplied(r, g, b, alpha),
                    );
                }
            }

            painter.rect_filled(key_rect, bk_rounding, fill);

            // Neumorphic edges
            let (bhi, bsh) = if active {
                (Color32::from_gray(100), Color32::from_gray(8))
            } else {
                (Color32::from_gray(52), Color32::from_gray(8))
            };
            painter.line_segment(
                [key_rect.left_top(), key_rect.right_top()],
                Stroke::new(2.0, bhi),
            );
            painter.line_segment(
                [key_rect.left_top(), key_rect.left_bottom()],
                Stroke::new(1.0, bhi),
            );
            painter.line_segment(
                [key_rect.right_top(), key_rect.right_bottom()],
                Stroke::new(1.0, bsh),
            );
            painter.line_segment(
                [key_rect.left_bottom(), key_rect.right_bottom()],
                Stroke::new(1.0, bsh),
            );

            // Scale degree dot for black keys
            if !active && let Some(degree) = scale_degree(note, root_note, seq_scale) {
                let dot_alpha: u8 = if degree == 0 { 200 } else { 100 };
                let dot_r = if degree == 0 { 2.2 } else { 1.5 };
                painter.circle_filled(
                    Pos2::new(x + bk_w * 0.5, key_rect.min.y + bk_h * 0.22),
                    dot_r,
                    Color32::from_rgba_unmultiplied(200, 200, 200, dot_alpha),
                );
            }

            // Label when show_label is on
            if show_label {
                let full = note_name(note);
                let sharp = &full[..full.len() - 1];
                let label_pos = Pos2::new(x + bk_w * 0.5, oy + bk_h - 9.0);
                let pill = Rect::from_center_size(
                    label_pos,
                    egui::Vec2::new(sharp.len() as f32 * 5.0 + 3.0, 10.0),
                );
                painter.rect_filled(
                    pill,
                    egui::Rounding::same(2.0),
                    Color32::from_black_alpha(160),
                );
                let label_color = if active { theme::CHALK } else { theme::FOG };
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_CENTER,
                    sharp,
                    egui::FontId::monospace(7.0),
                    label_color,
                );
            }

            // Click detection — black keys take priority
            if let Some(cp) = click_pos
                && key_rect.contains(cp)
            {
                clicked_note = Some(note);
            }
        }
    }

    // ── Click-to-play ─────────────────────────────────────────────────────────
    if let Some(note) = clicked_note {
        // New note pressed (or dragged onto a different key)
        if app.piano_mouse_note != Some(note) {
            // Release previous mouse note if any
            if let Some(prev) = app.piano_mouse_note {
                app.pressed_notes.remove(&prev);
            }
            app.piano_mouse_note = Some(note);
            app.pressed_notes.insert(note);
            let _ = app
                .audio_tx
                .push(AudioCommand::Trigger(TriggerEvent::BassTrigger {
                    note,
                    accent: false,
                    slide: false,
                    gate_samples: 22050,
                }));
            // Live record: write note into the bass pattern at the current step.
            let (live_record, running, current_step) = {
                let s = app.state.read();
                (s.live_record, s.sequencer.running, s.sequencer.current_step)
            };
            if live_record && running {
                let snap = app.state.read().clone();
                *app.state.write() = record_bass_note(snap, current_step, note);
            }
        }
    } else if app.piano_mouse_note.is_some()
        && (response.drag_stopped() || !response.is_pointer_button_down_on())
    {
        // Mouse released — remove only the mouse-held note, leave MIDI notes alone
        if let Some(prev) = app.piano_mouse_note.take() {
            app.pressed_notes.remove(&prev);
        }
        let _ = app
            .audio_tx
            .push(AudioCommand::Trigger(TriggerEvent::BassGateOff));
    }
}
