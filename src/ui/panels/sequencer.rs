// ─── ui/panels/sequencer.rs ───────────────────────────────────────────────────
// Step sequencer panel.

use super::sequencer_chain::draw_pattern_chain;
use super::sequencer_drums::draw_drum_rows;
use crate::state::{
    DrumVoice, MAX_STEPS, ROOT_NAMES, Scale, set_an1x_step, set_hoover_step, set_root_note,
    set_scale, set_scale_snap, toggle_bass_accent, toggle_bass_slide,
};
use crate::ui::{ImpulseApp, SEQ_LABEL_H, SEQ_LABEL_W, SEQ_VOL_W, theme, widgets};

pub fn draw_sequencer(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (
        current_step,
        running,
        seq_steps,
        time_sig_num,
        pad_px,
        root_note,
        scale,
        scale_snap,
        huth_full,
    ) = {
        let s = app.state.read();
        (
            s.sequencer.current_step,
            s.sequencer.running,
            s.sequencer.steps,
            s.sequencer.time_sig_num as usize,
            s.ui_prefs.effective_pad_px(),
            s.sequencer.root_note,
            s.sequencer.scale,
            s.sequencer.scale_snap,
            s.ui_prefs.huth_style == crate::state::HuthStyle::Full,
        )
    };
    // Only highlight the cursor when the sequencer is actually playing;
    // usize::MAX guarantees no step matches when stopped.
    let cursor = if running { current_step } else { usize::MAX };

    draw_pattern_chain(app, ui);

    // Steps counter control
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("STEPS")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let mut steps = app.state.read().sequencer.steps;
        if ui.small_button("−").clicked() && steps > 1 {
            steps -= 1;
            app.state.write().sequencer.steps = steps; // shrink: no tiling needed
        }
        ui.label(
            egui::RichText::new(format!("{:02}", steps))
                .color(theme::FOG)
                .monospace(),
        );
        if ui.small_button("+").clicked() && steps < MAX_STEPS {
            steps += 1;
            let new_state = crate::state::expand_sequencer_steps(app.state.read().clone(), steps);
            *app.state.write() = new_state;
        }

        // Preset step count buttons
        for &preset in &[8usize, 16, 32, 64] {
            if ui.small_button(format!("[{}]", preset)).clicked() {
                let new_state =
                    crate::state::expand_sequencer_steps(app.state.read().clone(), preset);
                *app.state.write() = new_state;
            }
        }

        // Step length indicator — shows the note value of each step
        let steps_per_beat = (seq_steps as f32 / time_sig_num as f32).max(0.5);
        let step_label = if (steps_per_beat - steps_per_beat.round()).abs() < 0.01 {
            let spb = steps_per_beat.round() as u32;
            match spb {
                1 => "1/4",
                2 => "1/8",
                4 => "1/16",
                8 => "1/32",
                _ => "",
            }
        } else {
            ""
        };
        if !step_label.is_empty() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(step_label)
                        .monospace()
                        .size(8.5)
                        .color(theme::IRON),
                );
            });
        }
    });

    // BPM row
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("BPM  ")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let (mut bpm, sync_on) = {
            let s = app.state.read();
            (s.sequencer.bpm, s.sequencer.midi_clock_sync)
        };
        // Dim the slider when MIDI sync is active (BPM is externally controlled).
        let slider_col = if sync_on { theme::FOG } else { theme::IRON };
        ui.visuals_mut().selection.bg_fill = slider_col;
        let resp = ui.add(
            egui::Slider::new(&mut bpm, 40.0..=300.0)
                .show_value(false)
                .trailing_fill(true),
        );
        if resp.changed() && !sync_on {
            app.state.write().sequencer.bpm = bpm;
            app.push_audio_params();
        }
        ui.label(
            egui::RichText::new(format!("{:.0}", bpm))
                .color(if sync_on { theme::IRON } else { theme::FOG })
                .monospace()
                .size(9.0),
        );
        // MIDI clock sync toggle
        let mut sync = sync_on;
        let sync_color = if sync_on {
            egui::Color32::from_rgb(80, 180, 80)
        } else {
            theme::SMOKE
        };
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("SYNC")
                        .monospace()
                        .size(8.5)
                        .color(sync_color),
                )
                .min_size(egui::Vec2::new(34.0, 16.0)),
            )
            .clicked()
        {
            sync = !sync;
            app.state.write().sequencer.midi_clock_sync = sync;
        }
    });

    // Swing row
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SWING")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let mut swing = app.state.read().sequencer.swing;
        let resp = ui.add(
            egui::Slider::new(&mut swing, 0.0..=1.0)
                .show_value(false)
                .trailing_fill(true),
        );
        if resp.changed() {
            app.state.write().sequencer.swing = swing;
        }
        ui.label(
            egui::RichText::new(format!("{:.2}", swing))
                .color(theme::FOG)
                .monospace()
                .size(9.0),
        );
    });

    // Time signature row
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("TIME SIG:")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let current_ts = app.state.read().sequencer.time_sig_num;
        for n in 2u8..=9 {
            let active = n == current_ts;
            let color = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            let text = egui::RichText::new(format!("{}", n))
                .monospace()
                .size(9.0)
                .color(color);
            if ui
                .add_sized([18.0, 16.0], egui::Button::new(text).fill(fill))
                .clicked()
            {
                app.state.write().sequencer.time_sig_num = n;
            }
        }
    });

    // ── Key / scale row ───────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("KEY  ")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        for note in 0u8..12 {
            let active = note == root_note;
            let color = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            let text = egui::RichText::new(ROOT_NAMES[note as usize])
                .monospace()
                .size(8.0)
                .color(color);
            if ui
                .add_sized([22.0, 16.0], egui::Button::new(text).fill(fill))
                .clicked()
            {
                let s = app.state.read().clone();
                *app.state.write() = set_root_note(s, note);
            }
        }
    });

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("SCALE")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        for &sc in Scale::all() {
            let active = sc == scale;
            let color = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            let text = egui::RichText::new(sc.name())
                .monospace()
                .size(7.5)
                .color(color);
            if ui
                .add_sized([46.0, 16.0], egui::Button::new(text).fill(fill))
                .clicked()
            {
                let s = app.state.read().clone();
                *app.state.write() = set_scale(s, sc);
            }
        }

        ui.add_space(8.0);
        // Scale snap toggle
        let snap_color = if scale_snap { theme::CHALK } else { theme::PIT };
        let snap_fill = if scale_snap {
            egui::Color32::from_gray(50)
        } else {
            egui::Color32::TRANSPARENT
        };
        let snap_text = egui::RichText::new("SNAP")
            .monospace()
            .size(7.5)
            .color(snap_color);
        if ui
            .add_sized([36.0, 16.0], egui::Button::new(snap_text).fill(snap_fill))
            .on_hover_text("Snap LLM bass notes to the active scale")
            .clicked()
        {
            let s = app.state.read().clone();
            *app.state.write() = set_scale_snap(s, !scale_snap);
        }
    });

    ui.add_space(4.0);

    let page_start = app.seq_page * 16;
    let total_pages = seq_steps.div_ceil(16);

    // Auto-follow cursor when playing
    if running && cursor != usize::MAX {
        let cursor_page = cursor / 16;
        if app.seq_page != cursor_page {
            app.seq_page = cursor_page;
        }
    }

    // Page nav (only shown when steps > 16)
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
    let beat_div = |ui: &mut egui::Ui, i: usize| {
        let beat_pos = (page_start + i) % time_sig_num;
        if i > 0 && beat_pos == 0 {
            ui.add_space(4.0);
        } else if i > 0 && i.is_multiple_of(4) {
            ui.add_space(2.0);
        }
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        // ── Bass rows — only shown when AcidBass is in the rack ───────────────
        if rack_has_bass {
            let bass_page: Vec<crate::state::TB303Step> = {
                let s = app.state.read();
                let end = (page_start + 16).min(s.sequencer.bass_pattern.len());
                s.sequencer.bass_pattern[page_start..end].to_vec()
            };

            // Bass note row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [
                        10.0 + 10.0 + (SEQ_LABEL_W - 20.0) + SEQ_VOL_W + 18.0 + 4.0 * 8.0,
                        SEQ_LABEL_H,
                    ],
                    egui::Label::new(
                        egui::RichText::new("BASS")
                            .color(theme::SMOKE)
                            .monospace()
                            .size(8.5),
                    ),
                );
                for i in 0..16usize {
                    let abs = page_start + i;
                    beat_div(ui, i);
                    let step = bass_page.get(i).copied();
                    let is_active = step.map(|s| s.active).unwrap_or(false);
                    let is_current = abs == cursor;
                    ui.add_enabled_ui(abs < seq_steps, |ui| {
                        let clicked = if huth_full {
                            let note = step.map(|s| s.note).unwrap_or(36);
                            let gate = step.map(|s| s.gate).unwrap_or(0.5);
                            widgets::huth_note_cell(ui, note, gate, is_active, is_current, pad_px)
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
                            *app.state.write() = crate::state::set_bass_step(s, abs, note, !was);
                        }
                    });
                }
            });

            // Accent row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [
                        10.0 + 10.0 + (SEQ_LABEL_W - 20.0) + SEQ_VOL_W + 18.0 + 4.0 * 8.0,
                        14.0,
                    ],
                    egui::Label::new(
                        egui::RichText::new("ACCENT")
                            .color(theme::IRON)
                            .monospace()
                            .size(7.5),
                    ),
                );
                for i in 0..16usize {
                    let abs = page_start + i;
                    beat_div(ui, i);
                    let is_accent = bass_page.get(i).map(|s| s.accent).unwrap_or(false);
                    ui.add_enabled_ui(abs < seq_steps, |ui| {
                        let color = if is_accent { theme::CHALK } else { theme::PIT };
                        let text = egui::RichText::new("A").monospace().size(7.5).color(color);
                        if ui
                            .add_sized(
                                [14.0, 14.0],
                                egui::Button::new(text).fill(egui::Color32::TRANSPARENT),
                            )
                            .clicked()
                        {
                            let s = app.state.read().clone();
                            *app.state.write() = toggle_bass_accent(s, abs);
                        }
                    });
                }
            });

            // Slide row
            ui.horizontal(|ui| {
                ui.add_sized(
                    [
                        10.0 + 10.0 + (SEQ_LABEL_W - 20.0) + SEQ_VOL_W + 18.0 + 4.0 * 8.0,
                        14.0,
                    ],
                    egui::Label::new(
                        egui::RichText::new("SLIDE")
                            .color(theme::IRON)
                            .monospace()
                            .size(7.5),
                    ),
                );
                for i in 0..16usize {
                    let abs = page_start + i;
                    beat_div(ui, i);
                    let is_slide = bass_page.get(i).map(|s| s.slide).unwrap_or(false);
                    ui.add_enabled_ui(abs < seq_steps, |ui| {
                        let color = if is_slide { theme::CHALK } else { theme::PIT };
                        let text = egui::RichText::new("S").monospace().size(7.5).color(color);
                        if ui
                            .add_sized(
                                [14.0, 14.0],
                                egui::Button::new(text).fill(egui::Color32::TRANSPARENT),
                            )
                            .clicked()
                        {
                            let s = app.state.read().clone();
                            *app.state.write() = toggle_bass_slide(s, abs);
                        }
                    });
                }
            });
        } // end if rack_has_bass

        // ── Hoover row — only shown when HooverLead is in the rack ────────────
        if rack_has_hoover {
            let hoover_enabled = app.state.read().hoover.enabled;
            let hoover_page: Vec<crate::state::TB303Step> = {
                let s = app.state.read();
                let end = (page_start + 16).min(s.sequencer.hoover_pattern.len());
                s.sequencer.hoover_pattern[page_start..end].to_vec()
            };
            ui.horizontal(|ui| {
                let label_color = if hoover_enabled {
                    theme::SMOKE
                } else {
                    theme::PIT
                };
                ui.add_sized(
                    [
                        10.0 + 10.0 + (SEQ_LABEL_W - 20.0) + SEQ_VOL_W + 18.0 + 4.0 * 8.0,
                        SEQ_LABEL_H,
                    ],
                    egui::Label::new(
                        egui::RichText::new("HOOVER")
                            .color(label_color)
                            .monospace()
                            .size(8.5),
                    ),
                );
                for i in 0..16usize {
                    let abs = page_start + i;
                    beat_div(ui, i);
                    let step = hoover_page.get(i).copied();
                    let is_active = step.map(|s| s.active).unwrap_or(false);
                    let is_current = abs == cursor;
                    ui.add_enabled_ui(abs < seq_steps, |ui| {
                        let clicked = if huth_full {
                            let note = step.map(|s| s.note).unwrap_or(57);
                            let gate = step.map(|s| s.gate).unwrap_or(0.5);
                            widgets::huth_note_cell(ui, note, gate, is_active, is_current, pad_px)
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
        } // end if rack_has_hoover

        // ── AN1X row — only shown when An1xVoice is in the rack ───────────────
        if rack_has_an1x {
            let an1x_enabled = app.state.read().an1x.enabled;
            let an1x_page: Vec<crate::state::TB303Step> = {
                let s = app.state.read();
                let end = (page_start + 16).min(s.sequencer.an1x_pattern.len());
                s.sequencer.an1x_pattern[page_start..end].to_vec()
            };
            ui.horizontal(|ui| {
                let label_color = if an1x_enabled {
                    theme::SMOKE
                } else {
                    theme::PIT
                };
                ui.add_sized(
                    [
                        10.0 + 10.0 + (SEQ_LABEL_W - 20.0) + SEQ_VOL_W + 18.0 + 4.0 * 8.0,
                        SEQ_LABEL_H,
                    ],
                    egui::Label::new(
                        egui::RichText::new("AN1X")
                            .color(label_color)
                            .monospace()
                            .size(8.5),
                    ),
                );
                for i in 0..16usize {
                    let abs = page_start + i;
                    beat_div(ui, i);
                    let step = an1x_page.get(i).copied();
                    let is_active = step.map(|s| s.active).unwrap_or(false);
                    let is_current = abs == cursor;
                    ui.add_enabled_ui(abs < seq_steps, |ui| {
                        let clicked = if huth_full {
                            let note = step.map(|s| s.note).unwrap_or(57);
                            let gate = step.map(|s| s.gate).unwrap_or(0.5);
                            widgets::huth_note_cell(ui, note, gate, is_active, is_current, pad_px)
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
        } // end if rack_has_an1x

        draw_drum_rows(
            app,
            ui,
            voices,
            page_start,
            pad_px,
            seq_steps,
            running,
            current_step,
        );
    });
}
