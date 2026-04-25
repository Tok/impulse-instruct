// ─── ui/panels/sequencer_sample_lane.rs ─────────────────────────────────────
// Per-voice sequencer lane renders extracted from `sequencer.rs` to
// keep that file under the 1000-line cap.  Currently hosts the WTBL
// (Wavetable) and SAMP (SampleInstrument) lanes; both share the same
// shape (label + volume slider + click-to-toggle steps) so adding more
// per-voice lanes here in future is a tractable next refactor.

use super::sequencer::{STEPS_PER_PAGE, STEPS_PER_ROW, fixed_label, fixed_slider, fixed_space};
use crate::state::{set_sample_step, set_wavetable_step};
use crate::ui::{ImpulseApp, SEQ_LABEL_H, SEQ_LABEL_W, SEQ_VOL_H, SEQ_VOL_W, theme, widgets};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_wavetable_lane(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    current_step: usize,
    running: bool,
    sub_rows: usize,
    seq_steps: usize,
    page_start: usize,
    row_spacer: f32,
    pad_px: f32,
    steps_per_beat: usize,
    steps_per_bar: usize,
) {
    let (wavetable_enabled, wavetable_steps) = {
        let s = app.state.read();
        (s.wavetable.enabled, s.sequencer.wavetable_steps)
    };
    let wavetable_cursor = if running {
        current_step % wavetable_steps.max(1)
    } else {
        usize::MAX
    };
    let wavetable_page: Vec<crate::state::TB303Step> = {
        let s = app.state.read();
        let end = (page_start + STEPS_PER_PAGE).min(s.sequencer.wavetable_pattern.len());
        s.sequencer.wavetable_pattern[page_start..end].to_vec()
    };
    let beat_div = |ui: &mut egui::Ui, local_i: usize, abs_i: usize| {
        if local_i == 0 {
            return;
        }
        if steps_per_bar > 0 && abs_i.is_multiple_of(steps_per_bar) {
            ui.add_space(4.0);
        } else if steps_per_beat > 0 && abs_i.is_multiple_of(steps_per_beat) {
            ui.add_space(2.0);
        }
    };
    for sub in 0..sub_rows {
        ui.horizontal(|ui| {
            let label_color = if wavetable_enabled {
                theme::SMOKE
            } else {
                theme::PIT
            };
            fixed_space(ui, 10.0);
            fixed_space(ui, 10.0);
            if sub == 0 {
                fixed_label(
                    ui,
                    SEQ_LABEL_W - 20.0,
                    SEQ_LABEL_H,
                    "WTBL",
                    label_color,
                    8.5,
                );
                let mut vol = app.state.read().wavetable.volume;
                if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut vol, 0.0..=1.0).changed() {
                    app.state.write().wavetable.volume = vol;
                    app.push_audio_params();
                }
            } else {
                fixed_space(ui, SEQ_LABEL_W - 20.0);
                fixed_space(ui, SEQ_VOL_W);
            }
            fixed_space(ui, 18.0);
            if row_spacer > 0.0 {
                ui.add_space(row_spacer);
            }
            let base = sub * STEPS_PER_ROW;
            for j in 0..STEPS_PER_ROW {
                let i = base + j;
                if i >= seq_steps {
                    break;
                }
                let abs = page_start + i;
                beat_div(ui, j, abs);
                let step = wavetable_page.get(i).copied();
                let is_active = step.map(|s| s.active).unwrap_or(false);
                let is_current = abs == wavetable_cursor;
                ui.add_enabled_ui(abs < wavetable_steps, |ui| {
                    let note = step.map(|s| s.note).unwrap_or(57);
                    let cond = step.map(|s| s.cond).unwrap_or(0);
                    let note_col = if is_active {
                        Some(theme::note_color(note))
                    } else {
                        None
                    };
                    let label = if is_active {
                        Some(crate::ui::note_name(note))
                    } else {
                        None
                    };
                    if let Some(new_active) = widgets::step_button(
                        ui, is_active, is_current, 1.0, 1.0, cond, note_col, label, pad_px,
                    ) {
                        let s = app.state.read().clone();
                        let note = s
                            .sequencer
                            .wavetable_pattern
                            .get(abs)
                            .map(|b| b.note)
                            .unwrap_or(57);
                        *app.state.write() = set_wavetable_step(s, abs, note, new_active);
                    }
                });
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_sample_lane(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    current_step: usize,
    running: bool,
    sub_rows: usize,
    seq_steps: usize,
    page_start: usize,
    row_spacer: f32,
    pad_px: f32,
    steps_per_beat: usize,
    steps_per_bar: usize,
) {
    let (sample_enabled, sample_steps) = {
        let s = app.state.read();
        (s.sample_instrument.enabled, s.sequencer.sample_steps)
    };
    let sample_cursor = if running {
        current_step % sample_steps.max(1)
    } else {
        usize::MAX
    };
    let sample_page: Vec<crate::state::TB303Step> = {
        let s = app.state.read();
        let end = (page_start + STEPS_PER_PAGE).min(s.sequencer.sample_pattern.len());
        s.sequencer.sample_pattern[page_start..end].to_vec()
    };
    let beat_div = |ui: &mut egui::Ui, local_i: usize, abs_i: usize| {
        if local_i == 0 {
            return;
        }
        if steps_per_bar > 0 && abs_i.is_multiple_of(steps_per_bar) {
            ui.add_space(4.0);
        } else if steps_per_beat > 0 && abs_i.is_multiple_of(steps_per_beat) {
            ui.add_space(2.0);
        }
    };
    for sub in 0..sub_rows {
        ui.horizontal(|ui| {
            let label_color = if sample_enabled {
                theme::SMOKE
            } else {
                theme::PIT
            };
            fixed_space(ui, 10.0);
            fixed_space(ui, 10.0);
            if sub == 0 {
                fixed_label(
                    ui,
                    SEQ_LABEL_W - 20.0,
                    SEQ_LABEL_H,
                    "SAMP",
                    label_color,
                    8.5,
                );
                let mut vol = app.state.read().sample_instrument.volume;
                if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut vol, 0.0..=1.5).changed() {
                    app.state.write().sample_instrument.volume = vol;
                    app.push_audio_params();
                }
            } else {
                fixed_space(ui, SEQ_LABEL_W - 20.0);
                fixed_space(ui, SEQ_VOL_W);
            }
            fixed_space(ui, 18.0);
            if row_spacer > 0.0 {
                ui.add_space(row_spacer);
            }
            let base = sub * STEPS_PER_ROW;
            for j in 0..STEPS_PER_ROW {
                let i = base + j;
                if i >= seq_steps {
                    break;
                }
                let abs = page_start + i;
                beat_div(ui, j, abs);
                let step = sample_page.get(i).copied();
                let is_active = step.map(|s| s.active).unwrap_or(false);
                let is_current = abs == sample_cursor;
                ui.add_enabled_ui(abs < sample_steps, |ui| {
                    let note = step.map(|s| s.note).unwrap_or(60);
                    let cond = step.map(|s| s.cond).unwrap_or(0);
                    let note_col = if is_active {
                        Some(theme::note_color(note))
                    } else {
                        None
                    };
                    let label = if is_active {
                        Some(crate::ui::note_name(note))
                    } else {
                        None
                    };
                    if let Some(new_active) = widgets::step_button(
                        ui, is_active, is_current, 1.0, 1.0, cond, note_col, label, pad_px,
                    ) {
                        let s = app.state.read().clone();
                        let note = s
                            .sequencer
                            .sample_pattern
                            .get(abs)
                            .map(|b| b.note)
                            .unwrap_or(60);
                        *app.state.write() = set_sample_step(s, abs, note, new_active);
                    }
                });
            }
        });
    }
}
