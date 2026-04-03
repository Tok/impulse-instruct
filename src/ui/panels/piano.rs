// ─── ui/panels/piano.rs ───────────────────────────────────────────────────────
// Piano keyboard widget.

use crate::audio::AudioCommand;
use crate::sequencer::TriggerEvent;
use crate::ui::{ImpulseApp, note_name, theme};

pub fn draw_piano(app: &mut ImpulseApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

    // Range: C1 (MIDI 24) → C6 (MIDI 84) = 5 octaves + high C
    const START_NOTE: u8 = 24;
    const END_NOTE: u8 = 84; // inclusive
    const N_OCTAVES: usize = 5;
    const N_WHITE: usize = N_OCTAVES * 7 + 1; // 36 white keys (including C6)

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
    let wk_w = (available_w / N_WHITE as f32).max(8.0);
    let wk_h = 74.0_f32;
    let bk_w = wk_w * 0.55;
    let bk_h = wk_h * 0.60;
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

    let use_color = app.piano_show_colors;
    let show_label = app.piano_show_labels;

    // Classic (non-Farbige) base colors
    let classic_white_inactive = Color32::from_rgb(58, 58, 58);
    let classic_black_inactive = Color32::from_rgb(18, 18, 18);
    let classic_active = Color32::from_rgb(200, 200, 200);

    // ── White keys ────────────────────────────────────────────────────────────
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

        let fill: Color32 = if use_color {
            let huth = theme::note_color(note);
            if pressed {
                theme::lerp_color(huth, theme::CHALK, 0.25)
            } else if seq_active {
                theme::lerp_color(huth, theme::SMOKE, 0.35)
            } else {
                theme::lerp_color(huth, Color32::from_rgb(62, 62, 62), 0.80)
            }
        } else {
            if active {
                classic_active
            } else {
                classic_white_inactive
            }
        };

        painter.rect_filled(key_rect, egui::Rounding::same(1.0), fill);
        painter.rect_stroke(
            key_rect,
            egui::Rounding::same(1.0),
            Stroke::new(0.5, theme::SLATE),
        );

        // Label — all white keys when labels are on
        if show_label {
            let lbl = note_name(note);
            // For non-C notes, trim the octave number to save space
            let lbl_short = if semi == 0 {
                lbl
            } else {
                &lbl[..lbl.len() - 1]
            };
            let label_color = if active {
                if use_color { theme::VOID } else { theme::DEEP }
            } else {
                theme::ASH
            };
            painter.text(
                Pos2::new(x + wk_w * 0.5, oy + wk_h - 9.0),
                egui::Align2::CENTER_CENTER,
                lbl_short,
                egui::FontId::monospace(7.0),
                label_color,
            );
        }

        // Click detection — white keys
        if let Some(cp) = click_pos
            && key_rect.contains(cp)
        {
            clicked_note = Some(note);
        }

        white_idx += 1;
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

            let fill: Color32 = if use_color {
                let huth = theme::note_color(note);
                if pressed {
                    theme::lerp_color(huth, theme::CHALK, 0.15)
                } else if seq_active {
                    huth
                } else {
                    theme::lerp_color(huth, theme::PIT, 0.82)
                }
            } else {
                if active {
                    classic_active
                } else {
                    classic_black_inactive
                }
            };

            painter.rect_filled(key_rect, egui::Rounding::same(1.0), fill);
            painter.rect_stroke(
                key_rect,
                egui::Rounding::same(1.0),
                Stroke::new(0.5, theme::SLATE),
            );

            // Label — sharp name only (no octave number, key is too narrow)
            if show_label {
                // e.g. "C#" from "C#3"
                let full = note_name(note);
                let sharp = &full[..full.len() - 1]; // strip octave digit
                let label_color = if active { theme::VOID } else { theme::IRON };
                painter.text(
                    Pos2::new(x + bk_w * 0.5, oy + bk_h - 8.0),
                    egui::Align2::CENTER_CENTER,
                    sharp,
                    egui::FontId::monospace(6.0),
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
        if !app.pressed_notes.contains(&note) {
            app.pressed_notes.insert(note);
            let _ = app
                .audio_tx
                .push(AudioCommand::Trigger(TriggerEvent::BassTrigger {
                    note,
                    accent: false,
                    slide: false,
                    gate_samples: 22050,
                }));
        }
    } else if response.drag_stopped()
        || (!response.is_pointer_button_down_on() && !app.pressed_notes.is_empty())
    {
        // Release all click-triggered notes when pointer lifts
        // (MIDI notes are managed by their own NoteOff messages)
        // Only clear notes that aren't from MIDI (we track MIDI separately)
        // Simple heuristic: clear on pointer release
        let _ = app
            .audio_tx
            .push(AudioCommand::Trigger(TriggerEvent::BassGateOff));
    }
}
