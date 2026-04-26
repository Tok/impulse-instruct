// ─── ui/panels/pattern_heatmap.rs ─────────────────────────────────────────────
// Pattern density heatmap module — at-a-glance grid view of every
// active sequencer voice in the current pattern.
//
// Rows = each voice with at least one active step (drums + every
// melodic lane).  Columns = the canonical 16-step window.  Cell
// brightness reflects the step's active state and velocity.
// Empty rows are suppressed so the user only sees voices that
// matter to the current pattern.
//
// Pure UI: no DSP, no audio I/O, no state writes.  Reads the
// sequencer state under a single read lock.  The current playhead
// step is highlighted in every row to give a global "where's the
// playhead" indicator across all voices.

use crate::state::DrumVoice;
use crate::state::sequencer_state::{Step, TB303Step};
use crate::ui::{ImpulseApp, theme};

/// Maximum step columns we render — matches the canonical 16-step
/// window every voice locks to in the rack-level overview.  Voices
/// with longer per-voice step counts get truncated to the first 16
/// for the heatmap display; the focused per-voice panel still
/// shows every step.
const STEP_COLS: usize = 16;

/// One pre-computed row for rendering — label plus the per-step
/// intensities (already collapsed from the source pattern under
/// the read lock).  Owning the data this way lets us drop the
/// state lock before drawing, so the painter doesn't hold onto
/// borrowed sequencer fields across the layout pass.
type Row = (String, [f32; STEP_COLS]);

pub fn draw_pattern_heatmap(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (rows, playhead) = collect_rows(app);

    if rows.is_empty() {
        ui.label(
            egui::RichText::new("No active steps")
                .color(theme::IRON)
                .monospace()
                .size(8.5),
        );
        return;
    }

    let avail = ui.available_width();
    let label_w = 56.0_f32;
    let cell_w = ((avail - label_w - 8.0) / STEP_COLS as f32).max(6.0);
    let cell_h = 9.0_f32;
    let row_gap = 2.0_f32;

    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            for (label, intensities) in &rows {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [label_w, cell_h],
                        egui::Label::new(
                            egui::RichText::new(label.as_str())
                                .monospace()
                                .size(8.0)
                                .color(theme::ASH),
                        ),
                    );
                    for (i, intensity) in intensities.iter().enumerate() {
                        let intensity = intensity.clamp(0.0, 1.0);
                        let is_playhead = i == playhead;
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(cell_w, cell_h), egui::Sense::hover());
                        // Background cell — dim grey grid.  Beat
                        // markers (every 4 steps) get a slight
                        // brightness lift so the user reads bar
                        // structure at a glance.
                        let beat_lift = if i % 4 == 0 { 8 } else { 0 };
                        let bg = egui::Color32::from_gray(28 + beat_lift);
                        ui.painter().rect_filled(rect, 1.0, bg);
                        // Active cell — brightness scaled by the
                        // step's velocity.
                        if intensity > 0.01 {
                            // 32 base + up to 200 = 232 max, comfortably
                            // under 255.  Cast inside the addition stays
                            // u8 — no overflow possible.
                            let v = 32_u8.saturating_add((intensity * 200.0) as u8);
                            let active = egui::Color32::from_gray(v);
                            ui.painter().rect_filled(rect.shrink(0.5), 1.0, active);
                        }
                        // Playhead column highlight — a thin band
                        // top + bottom so it's legible even on an
                        // active cell.
                        if is_playhead {
                            let stroke = egui::Stroke::new(1.0, theme::CHALK);
                            ui.painter()
                                .line_segment([rect.left_top(), rect.right_top()], stroke);
                            ui.painter()
                                .line_segment([rect.left_bottom(), rect.right_bottom()], stroke);
                        }
                    }
                });
                ui.add_space(row_gap);
            }
        });
}

/// Snapshot the current sequencer state into a list of pre-computed
/// rows.  Acquires + releases the state read lock once; the
/// returned data is fully owned so the renderer doesn't carry any
/// borrows across the layout pass.
fn collect_rows(app: &ImpulseApp) -> (Vec<Row>, usize) {
    let state = app.state.read();
    let seq = &state.sequencer;
    let playhead = seq.current_step.min(STEP_COLS - 1);
    let mut rows: Vec<Row> = Vec::new();

    // Drum voices first (rhythmic backbone).
    for d in DrumVoice::ALL {
        if let Some(pattern) = seq.drum_patterns.get(d)
            && let Some(intensities) = drum_intensities(pattern)
        {
            rows.push((drum_short_label(*d).to_string(), intensities));
        }
    }

    // Single-voice melodic lanes.
    push_tb303_row(&mut rows, "BASS", &seq.bass_pattern);
    for (vi, voice_pat) in seq.bass_patterns.iter().enumerate().skip(1) {
        if seq.bass_voice_enabled.get(vi).copied().unwrap_or(false) {
            push_tb303_row(&mut rows, &format!("BASS {}", vi + 1), voice_pat);
        }
    }
    push_tb303_row(&mut rows, "HOOVER", &seq.hoover_pattern);
    push_tb303_row(&mut rows, "AN1X", &seq.an1x_pattern);
    push_tb303_row(&mut rows, "PLUCK", &seq.pluck_pattern);
    push_tb303_row(&mut rows, "WAVE", &seq.wavetable_pattern);
    push_tb303_row(&mut rows, "SAMPLE", &seq.sample_pattern);
    push_tb303_row(&mut rows, "FM OPS", &seq.fm_ops_pattern);
    push_tb303_row(&mut rows, "ADD", &seq.additive_pattern);
    push_tb303_row(&mut rows, "MODAL", &seq.modal_pattern);
    push_tb303_row(&mut rows, "CHIP", &seq.chiptune_pattern);
    push_tb303_row(&mut rows, "VOCAL", &seq.vocal_pattern);

    (rows, playhead)
}

fn drum_intensities(pattern: &[Step]) -> Option<[f32; STEP_COLS]> {
    if pattern.iter().take(STEP_COLS).all(|s| !s.active) {
        return None;
    }
    let mut out = [0.0_f32; STEP_COLS];
    for (i, slot) in out.iter_mut().enumerate() {
        if let Some(s) = pattern.get(i)
            && s.active
        {
            *slot = s.velocity.clamp(0.0, 1.0);
        }
    }
    Some(out)
}

fn push_tb303_row(rows: &mut Vec<Row>, label: &str, pattern: &[TB303Step]) {
    if pattern.iter().take(STEP_COLS).all(|s| !s.active) {
        return;
    }
    let mut out = [0.0_f32; STEP_COLS];
    for (i, slot) in out.iter_mut().enumerate() {
        if let Some(s) = pattern.get(i)
            && s.active
        {
            // Accent bumps brightness a touch; gate trims it.
            *slot = (0.7 + s.accent.clamp(0.0, 1.0) * 0.3) * s.gate.clamp(0.2, 1.0);
        }
    }
    rows.push((label.to_string(), out));
}

fn drum_short_label(d: DrumVoice) -> &'static str {
    match d {
        DrumVoice::Kick808 => "808 KK",
        DrumVoice::Snare808 => "808 SN",
        DrumVoice::HihatClosed808 => "808 CH",
        DrumVoice::HihatOpen808 => "808 OH",
        DrumVoice::TomHi808 => "808 TH",
        DrumVoice::TomMid808 => "808 TM",
        DrumVoice::TomLo808 => "808 TL",
        DrumVoice::Kick909 => "909 KK",
        DrumVoice::Snare909 => "909 SN",
        DrumVoice::HihatClosed909 => "909 CH",
        DrumVoice::HihatOpen909 => "909 OH",
        DrumVoice::Clap909 => "909 CL",
        DrumVoice::Rim909 => "909 RM",
        DrumVoice::Amen => "AMEN",
        DrumVoice::GabberKick => "GABBER",
    }
}
