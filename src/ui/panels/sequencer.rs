// ─── ui/panels/sequencer.rs ───────────────────────────────────────────────────
// Step sequencer panel.

use super::sequencer_drums::draw_drum_rows;
use crate::state::{
    DrumVoice, set_an1x_step, set_hoover_step, toggle_bass_accent, toggle_bass_slide,
};
use crate::ui::{ImpulseApp, SEQ_LABEL_H, SEQ_LABEL_W, SEQ_VOL_H, SEQ_VOL_W, theme, widgets};

/// Maximum step buttons visible (MAX_STEPS = 64 fits 2 rows of 32).
pub(super) const STEPS_PER_PAGE: usize = crate::state::MAX_STEPS;
/// Number of step buttons per physical row — rows wrap vertically when >32.
pub(super) const STEPS_PER_ROW: usize = 32;

/// Number of sub-rows (wrapped 32-step rows) needed for `seq_steps`.
/// 1..=32 → 1 row, 33..=64 → 2 rows.
pub(super) fn sub_rows_for(seq_steps: usize) -> usize {
    seq_steps.min(STEPS_PER_PAGE).div_ceil(STEPS_PER_ROW).max(1)
}

/// Step count rendered in a given sub-row (last sub-row may be partial).
pub(super) fn steps_in_sub(seq_steps: usize, sub: usize) -> usize {
    let base = sub * STEPS_PER_ROW;
    seq_steps.saturating_sub(base).min(STEPS_PER_ROW)
}

/// Estimate total width of the widest step row (buttons + beat dividers).
/// Used to right-justify step rows by inserting a spacer after the prefix.
/// `row_steps` is the cell count for the widest sub-row (first one).
/// Reserve a fixed-width zero-height slot. Unlike `ui.add_space(w)` (which
/// just increments the cursor), this goes through `allocate_exact_size` and
/// therefore adds `item_spacing.x` trailing the slot — matching the
/// behaviour of label/slider widgets so sub-row prefixes share one x anchor
/// when slots mix placeholder space and real widgets.
pub(super) fn fixed_space(ui: &mut egui::Ui, w: f32) {
    ui.allocate_exact_size(egui::vec2(w, 0.0), egui::Sense::hover());
}

/// Allocate an exact-sized rect and render a left-aligned monospace label
/// inside it via the painter. Unlike `ui.add_sized([w,h], Label::new(...))`,
/// this guarantees the cursor advances by exactly `w` pixels regardless of
/// the text's intrinsic width, which keeps sequencer lane prefixes aligned.
pub(super) fn fixed_label(
    ui: &mut egui::Ui,
    w: f32,
    h: f32,
    text: &str,
    color: egui::Color32,
    size: f32,
) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter().text(
            egui::pos2(rect.min.x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::monospace(size),
            color,
        );
    }
}

/// Render a Slider that's exactly `w` pixels wide. Overrides
/// `style.spacing.slider_width` so the Slider's intrinsic layout matches the
/// reserved space — without this `ui.add_sized([w,h], Slider)` would advance
/// the cursor by the Slider's default ~100 px width, not `w`.
pub(super) fn fixed_slider(
    ui: &mut egui::Ui,
    w: f32,
    h: f32,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
) -> egui::Response {
    let prev = ui.style().spacing.slider_width;
    ui.style_mut().spacing.slider_width = w;
    let resp = ui.add_sized(
        [w, h],
        egui::Slider::new(value, range)
            .show_value(false)
            .trailing_fill(true),
    );
    ui.style_mut().spacing.slider_width = prev;
    resp
}

pub(super) fn step_grid_width(
    pad_px: f32,
    time_sig_num: usize,
    page_start: usize,
    item_spacing_x: f32,
    row_steps: usize,
) -> f32 {
    let mut w = 0.0f32;
    for i in 0..row_steps {
        let beat_pos = (page_start + i) % time_sig_num;
        if i > 0 && beat_pos == 0 {
            w += 4.0;
        } else if i > 0 && i.is_multiple_of(4) {
            w += 2.0;
        }
        w += pad_px + item_spacing_x;
    }
    w
}

pub fn draw_sequencer(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Dynamic height: count actual visible rows (bass + expanded drums + hoover + an1x)
    {
        let s = app.state.read();
        let pad = s.ui_prefs.effective_pad_px();
        let step_row = pad + 22.0;
        let marker_row = 16.0;
        let drum_sub = 14.0;
        let header_h = 55.0;

        let has_bass = s
            .rack
            .modules
            .iter()
            .any(|m| m.kind == crate::state::ModuleKind::AcidBass);
        let has_hoover = s
            .rack
            .modules
            .iter()
            .any(|m| m.kind == crate::state::ModuleKind::HooverLead);
        let has_an1x = s
            .rack
            .modules
            .iter()
            .any(|m| m.kind == crate::state::ModuleKind::An1xVoice);

        // Drum voices with at least one active step or currently expanded.
        let drum_rows: usize = s
            .sequencer
            .drum_patterns
            .iter()
            .filter(|(_, p)| p.iter().any(|st| st.active))
            .count()
            + app.expanded_seq_voices.len();

        let sub_rows = s.sequencer.steps.min(64).div_ceil(32).max(1) as f32;
        let bass_h = if has_bass {
            sub_rows * (step_row + marker_row + marker_row)
        } else {
            0.0
        };
        let hoover_h = if has_hoover { sub_rows * step_row } else { 0.0 };
        let an1x_h = if has_an1x { sub_rows * step_row } else { 0.0 };
        let drums_h = drum_rows as f32 * sub_rows * (step_row + drum_sub);

        let h = (header_h + bass_h + hoover_h + an1x_h + drums_h).max(120.0);
        ui.set_min_height(h);
    }
    let (current_step, running, seq_steps, time_sig_num, pad_px, huth_full) = {
        let s = app.state.read();
        (
            s.sequencer.current_step,
            s.sequencer.running,
            s.sequencer.steps,
            s.sequencer.time_sig_num as usize,
            s.ui_prefs.effective_pad_px(),
            s.ui_prefs.huth_style == crate::state::HuthStyle::Full,
        )
    };
    super::sequencer_header::draw_line_1(app, ui);
    ui.add_space(2.0);
    super::sequencer_header::draw_line_2(app, ui);
    ui.add_space(4.0);

    let page_start = app.seq_page * STEPS_PER_PAGE;
    let total_pages = seq_steps.div_ceil(STEPS_PER_PAGE);

    // Auto-follow cursor when playing
    if running {
        let wrapped = current_step % seq_steps.max(1);
        let cursor_page = wrapped / STEPS_PER_PAGE;
        if app.seq_page != cursor_page {
            app.seq_page = cursor_page;
        }
    }

    // Page nav (only shown when steps > page size)
    if total_pages > 1 {
        ui.horizontal(|ui| {
            if ui.small_button("<").clicked() && app.seq_page > 0 {
                app.seq_page -= 1;
            }
            ui.label(
                egui::RichText::new(format!("PAGE {}/{}", app.seq_page + 1, total_pages))
                    .monospace()
                    .size(9.0)
                    .color(theme::SMOKE),
            );
            if ui.small_button(">").clicked() && app.seq_page < total_pages - 1 {
                app.seq_page += 1;
            }
        });
    }

    ui.add_space(2.0);

    // ── Rack-presence flags — only show rows for modules that are in the rack ──
    let (rack_has_bass, rack_has_hoover, rack_has_an1x, active_drum_voices) = {
        use crate::state::ModuleKind;
        let s = app.state.read();
        let has = |k: ModuleKind| s.rack.modules.iter().any(|m| m.kind == k && m.enabled);
        let filtered: Vec<DrumVoice> = DrumVoice::ALL
            .iter()
            .filter(|v| has(v.module_kind()))
            .copied()
            .collect();
        (
            has(ModuleKind::AcidBass),
            has(ModuleKind::HooverLead),
            has(ModuleKind::An1xVoice),
            filtered,
        )
    };
    let voices: &[DrumVoice] = &active_drum_voices;

    // ── Helper closure: emit beat dividers ────────────────────────────────────
    // `local_i` is the position within the sub-row (0..STEPS_PER_ROW); no leading
    // spacer at local_i==0. `abs_i` is the absolute step index for beat calculation.
    let beat_div = |ui: &mut egui::Ui, local_i: usize, abs_i: usize| {
        if local_i == 0 {
            return;
        }
        let beat_pos = abs_i % time_sig_num;
        if beat_pos == 0 {
            ui.add_space(4.0);
        } else if abs_i.is_multiple_of(4) {
            ui.add_space(2.0);
        }
    };

    let sub_rows = sub_rows_for(seq_steps);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Prefix width computed statically from the fixed 5-widget prefix
        // structure (M + S + label + vol + steps, separated by item_spacing).
        // Every row — drums, bass, hoover, an1x, accent, slide — emits the
        // same 5 widgets so cells share one x anchor regardless of lane count.
        let item_spacing_x = ui.spacing().item_spacing.x;
        let prefix_w = 10.0 + 10.0 + (SEQ_LABEL_W - 20.0) + SEQ_VOL_W + 18.0 + 5.0 * item_spacing_x;
        app.seq_prefix_width = prefix_w;
        // Right-justify spacer: push step grids to the right edge. Use the
        // widest sub-row's step count so all sub-rows share a common right
        // anchor (partial last sub-rows left-align under the fixed anchor).
        let widest_row_steps = steps_in_sub(seq_steps, 0);
        let grid_w = step_grid_width(
            pad_px,
            time_sig_num,
            page_start,
            ui.spacing().item_spacing.x,
            widest_row_steps,
        );
        let row_spacer = (ui.available_width() - prefix_w - grid_w - 8.0).max(0.0);

        // ── Bass rows — only shown when AcidBass is in the rack ───────────────
        if rack_has_bass {
            let bass_steps = app.state.read().sequencer.bass_steps;
            let bass_cursor = if running {
                current_step % bass_steps.max(1)
            } else {
                usize::MAX
            };
            let bass_page: Vec<crate::state::TB303Step> = {
                let s = app.state.read();
                let end = (page_start + STEPS_PER_PAGE).min(s.sequencer.bass_pattern.len());
                s.sequencer.bass_pattern[page_start..end].to_vec()
            };
            for sub in 0..sub_rows {
                ui.horizontal(|ui| {
                    fixed_space(ui, 10.0);
                    fixed_space(ui, 10.0);
                    if sub == 0 {
                        fixed_label(
                            ui,
                            SEQ_LABEL_W - 20.0,
                            SEQ_LABEL_H,
                            "BASS",
                            theme::SMOKE,
                            8.5,
                        );
                        let mut vol = app.state.read().bass_voices[0].synth.volume;
                        if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut vol, 0.0..=1.0).changed() {
                            app.state.write().bass_voices[0].synth.volume = vol;
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
                        let step = bass_page.get(i).copied();
                        let is_active = step.map(|s| s.active).unwrap_or(false);
                        let is_current = abs == bass_cursor;
                        ui.add_enabled_ui(abs < bass_steps, |ui| {
                            let clicked = if huth_full {
                                let note = step.map(|s| s.note).unwrap_or(36);
                                let gate = step.map(|s| s.gate).unwrap_or(0.5);
                                widgets::huth_note_cell(
                                    ui, note, gate, is_active, is_current, pad_px,
                                )
                            } else {
                                let note = step.map(|s| s.note).unwrap_or(36);
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
                                widgets::step_button(
                                    ui, is_active, is_current, 1.0, 1.0, note_col, label, pad_px,
                                )
                            };
                            if clicked {
                                let s = app.state.read().clone();
                                let note = s
                                    .sequencer
                                    .bass_pattern
                                    .get(abs)
                                    .map(|b| b.note)
                                    .unwrap_or(36);
                                let was = s
                                    .sequencer
                                    .bass_pattern
                                    .get(abs)
                                    .map(|b| b.active)
                                    .unwrap_or(false);
                                *app.state.write() =
                                    crate::state::set_bass_step(s, abs, note, !was);
                            }
                        });
                    }
                });
            }

            // Accent + slide rows — compact vertical spacing
            let marker_h = (pad_px * 0.35).clamp(10.0, 16.0);
            let row_h = marker_h.max(SEQ_VOL_H);
            let prev_spacing = ui.spacing().item_spacing.y;
            ui.spacing_mut().item_spacing.y = 0.0;
            for sub in 0..sub_rows {
                ui.horizontal(|ui| {
                    fixed_space(ui, 10.0);
                    fixed_space(ui, 10.0);
                    if sub == 0 {
                        fixed_label(ui, SEQ_LABEL_W - 20.0, row_h, "ACCENT", theme::IRON, 7.0);
                        let mut accent = app.state.read().bass_voices[0].synth.accent_level;
                        if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut accent, 0.0..=1.0).changed()
                        {
                            app.state.write().bass_voices[0].synth.accent_level = accent;
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
                        let is_accent = bass_page.get(i).map(|s| s.accent).unwrap_or(false);
                        let enabled = abs < bass_steps;
                        let color = if !enabled {
                            theme::PIT
                        } else if is_accent {
                            theme::CHALK
                        } else {
                            theme::PIT
                        };
                        let text = egui::RichText::new("A").monospace().size(7.0).color(color);
                        if ui
                            .add_sized(
                                [pad_px, row_h],
                                egui::Button::new(text).fill(egui::Color32::TRANSPARENT),
                            )
                            .clicked()
                            && enabled
                        {
                            let s = app.state.read().clone();
                            *app.state.write() = toggle_bass_accent(s, abs);
                        }
                    }
                });
            }

            // Slide row
            for sub in 0..sub_rows {
                ui.horizontal(|ui| {
                    fixed_space(ui, 10.0);
                    fixed_space(ui, 10.0);
                    if sub == 0 {
                        fixed_label(ui, SEQ_LABEL_W - 20.0, row_h, "SLIDE", theme::IRON, 7.0);
                        let mut slide = app.state.read().bass_voices[0].synth.portamento_time;
                        if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut slide, 0.0..=1.0).changed() {
                            app.state.write().bass_voices[0].synth.portamento_time = slide;
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
                        let is_slide = bass_page.get(i).map(|s| s.slide).unwrap_or(false);
                        let enabled = abs < bass_steps;
                        let color = if !enabled {
                            theme::PIT
                        } else if is_slide {
                            theme::CHALK
                        } else {
                            theme::PIT
                        };
                        let text = egui::RichText::new("S").monospace().size(7.0).color(color);
                        if ui
                            .add_sized(
                                [pad_px, row_h],
                                egui::Button::new(text).fill(egui::Color32::TRANSPARENT),
                            )
                            .clicked()
                            && enabled
                        {
                            let s = app.state.read().clone();
                            *app.state.write() = toggle_bass_slide(s, abs);
                        }
                    }
                });
            }

            // PAN row — drag horizontally to set, right-click to centre.
            for sub in 0..sub_rows {
                ui.horizontal(|ui| {
                    fixed_space(ui, 20.0);
                    if sub == 0 {
                        fixed_label(ui, SEQ_LABEL_W - 20.0, row_h, "PAN", theme::IRON, 7.0);
                        let reset_w = 14.0;
                        let resp = ui
                            .add_sized(
                                [reset_w, row_h],
                                egui::Button::new(
                                    egui::RichText::new("○")
                                        .monospace()
                                        .size(9.0)
                                        .color(theme::IRON),
                                )
                                .fill(egui::Color32::TRANSPARENT),
                            )
                            .on_hover_text("Reset all step pans to centre");
                        if resp.clicked() {
                            let mut s = app.state.write();
                            for step in s.sequencer.bass_pattern.iter_mut() {
                                step.pan = 0.0;
                            }
                        }
                        let item_x = ui.spacing().item_spacing.x;
                        fixed_space(ui, SEQ_VOL_W + 18.0 - reset_w - item_x);
                    } else {
                        fixed_space(ui, SEQ_LABEL_W - 20.0);
                        fixed_space(ui, SEQ_VOL_W + 18.0);
                    }
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
                        let enabled = abs < bass_steps;
                        let pan = bass_page.get(i).map(|s| s.pan).unwrap_or(0.0);
                        super::sequencer_chain::pan_cell(ui, app, abs, pan, enabled, pad_px, row_h);
                    }
                });
            }
            ui.spacing_mut().item_spacing.y = prev_spacing;
        } // end if rack_has_bass

        // ── Hoover row — only shown when HooverLead is in the rack ────────────
        if rack_has_hoover {
            let (hoover_enabled, hoover_steps) = {
                let s = app.state.read();
                (s.hoover.enabled, s.sequencer.hoover_steps)
            };
            let hoover_cursor = if running {
                current_step % hoover_steps.max(1)
            } else {
                usize::MAX
            };
            let hoover_page: Vec<crate::state::TB303Step> = {
                let s = app.state.read();
                let end = (page_start + STEPS_PER_PAGE).min(s.sequencer.hoover_pattern.len());
                s.sequencer.hoover_pattern[page_start..end].to_vec()
            };
            for sub in 0..sub_rows {
                ui.horizontal(|ui| {
                    let label_color = if hoover_enabled {
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
                            "HOOVER",
                            label_color,
                            8.5,
                        );
                        let mut vol = app.state.read().hoover.volume;
                        if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut vol, 0.0..=1.0).changed() {
                            app.state.write().hoover.volume = vol;
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
                        let step = hoover_page.get(i).copied();
                        let is_active = step.map(|s| s.active).unwrap_or(false);
                        let is_current = abs == hoover_cursor;
                        ui.add_enabled_ui(abs < hoover_steps, |ui| {
                            let clicked = if huth_full {
                                let note = step.map(|s| s.note).unwrap_or(57);
                                let gate = step.map(|s| s.gate).unwrap_or(0.5);
                                widgets::huth_note_cell(
                                    ui, note, gate, is_active, is_current, pad_px,
                                )
                            } else {
                                let note = step.map(|s| s.note).unwrap_or(36);
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
                                widgets::step_button(
                                    ui, is_active, is_current, 1.0, 1.0, note_col, label, pad_px,
                                )
                            };
                            if clicked {
                                let s = app.state.read().clone();
                                let note = s
                                    .sequencer
                                    .hoover_pattern
                                    .get(abs)
                                    .map(|b| b.note)
                                    .unwrap_or(57);
                                let was = s
                                    .sequencer
                                    .hoover_pattern
                                    .get(abs)
                                    .map(|b| b.active)
                                    .unwrap_or(false);
                                *app.state.write() = set_hoover_step(s, abs, note, !was);
                            }
                        });
                    }
                });
            }
        } // end if rack_has_hoover

        // ── AN1X row — only shown when An1xVoice is in the rack ───────────────
        if rack_has_an1x {
            let (an1x_enabled, an1x_steps) = {
                let s = app.state.read();
                (s.an1x.enabled, s.sequencer.an1x_steps)
            };
            let an1x_cursor = if running {
                current_step % an1x_steps.max(1)
            } else {
                usize::MAX
            };
            let an1x_page: Vec<crate::state::TB303Step> = {
                let s = app.state.read();
                let end = (page_start + STEPS_PER_PAGE).min(s.sequencer.an1x_pattern.len());
                s.sequencer.an1x_pattern[page_start..end].to_vec()
            };
            for sub in 0..sub_rows {
                ui.horizontal(|ui| {
                    let label_color = if an1x_enabled {
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
                            "AN1X",
                            label_color,
                            8.5,
                        );
                        let mut vol = app.state.read().an1x.volume;
                        if fixed_slider(ui, SEQ_VOL_W, SEQ_VOL_H, &mut vol, 0.0..=1.0).changed() {
                            app.state.write().an1x.volume = vol;
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
                        let step = an1x_page.get(i).copied();
                        let is_active = step.map(|s| s.active).unwrap_or(false);
                        let is_current = abs == an1x_cursor;
                        ui.add_enabled_ui(abs < an1x_steps, |ui| {
                            let clicked = if huth_full {
                                let note = step.map(|s| s.note).unwrap_or(57);
                                let gate = step.map(|s| s.gate).unwrap_or(0.5);
                                widgets::huth_note_cell(
                                    ui, note, gate, is_active, is_current, pad_px,
                                )
                            } else {
                                let note = step.map(|s| s.note).unwrap_or(36);
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
                                widgets::step_button(
                                    ui, is_active, is_current, 1.0, 1.0, note_col, label, pad_px,
                                )
                            };
                            if clicked {
                                let s = app.state.read().clone();
                                let note = s
                                    .sequencer
                                    .an1x_pattern
                                    .get(abs)
                                    .map(|b| b.note)
                                    .unwrap_or(57);
                                let was = s
                                    .sequencer
                                    .an1x_pattern
                                    .get(abs)
                                    .map(|b| b.active)
                                    .unwrap_or(false);
                                *app.state.write() = set_an1x_step(s, abs, note, !was);
                            }
                        });
                    }
                });
            }
        } // end if rack_has_an1x

        draw_drum_rows(
            app,
            ui,
            voices,
            page_start,
            pad_px,
            seq_steps,
            running,
            row_spacer,
            current_step,
            sub_rows,
            time_sig_num,
        );
    });

    // Pre-echo row lives at the bottom of the panel below the lane
    // ScrollArea.  The module is sized (cols, 3) specifically so this
    // row has room to sit without being clipped by the scroll area's
    // greedy vertical fill.
    super::sequencer_preecho::draw_preecho_row(app, ui);
}
