// ─── ui/panels/sequencer.rs ───────────────────────────────────────────────────
// Step sequencer panel.

use crate::state::{DrumVoice, MAX_STEPS, toggle_bass_accent, toggle_bass_slide, toggle_drum_step};
use crate::ui::{ImpulseApp, SEQ_LABEL_H, SEQ_LABEL_W, SEQ_VOL_H, SEQ_VOL_W, theme, widgets};

pub fn draw_sequencer(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (current_step, running, seq_steps, time_sig_num) = {
        let s = app.state.read();
        (
            s.sequencer.current_step,
            s.sequencer.running,
            s.sequencer.steps,
            s.sequencer.time_sig_num as usize,
        )
    };
    // Only highlight the cursor when the sequencer is actually playing;
    // usize::MAX guarantees no step matches when stopped.
    let cursor = if running { current_step } else { usize::MAX };

    widgets::section_header(ui, "STEP SEQUENCER");

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
    });

    // BPM row
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("BPM  ")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let mut bpm = app.state.read().sequencer.bpm;
        let resp = ui.add(
            egui::Slider::new(&mut bpm, 40.0..=250.0)
                .show_value(false)
                .trailing_fill(true),
        );
        if resp.changed() {
            app.state.write().sequencer.bpm = bpm;
            app.push_audio_params();
        }
        ui.label(
            egui::RichText::new(format!("{:.0}", bpm))
                .color(theme::FOG)
                .monospace()
                .size(9.0),
        );
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

    let voices = DrumVoice::ALL;

    // Step grid — draw each row
    egui::ScrollArea::vertical().show(ui, |ui| {
        for voice in voices {
            ui.horizontal(|ui| {
                // Voice label
                let label = voice.label();
                ui.add_sized(
                    [SEQ_LABEL_W, SEQ_LABEL_H],
                    egui::Label::new(
                        egui::RichText::new(label)
                            .color(theme::SMOKE)
                            .monospace()
                            .size(8.5),
                    ),
                );

                // Per-voice volume slider
                let mut vol = voice.get_volume(&app.state.read());
                let vol_resp = ui.add_sized(
                    [SEQ_VOL_W, SEQ_VOL_H],
                    egui::Slider::new(&mut vol, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if vol_resp.changed() {
                    let s = app.state.read().clone();
                    *app.state.write() = voice.set_volume(s, vol);
                    app.push_audio_params();
                }

                // Snapshot pattern for this page
                let pattern: Vec<crate::state::Step> = {
                    let s = app.state.read();
                    s.sequencer
                        .drum_patterns
                        .get(voice)
                        .map(|p| p[page_start..(page_start + 16).min(p.len())].to_vec())
                        .unwrap_or_else(|| vec![crate::state::Step::default(); 16])
                };

                let mut toggled = None;
                for i in 0..16usize {
                    let abs = page_start + i;
                    // Beat dividers: wider gap at beat boundaries, narrow gap every 4
                    let beat_pos = (page_start + i) % time_sig_num;
                    if i > 0 && beat_pos == 0 {
                        ui.add_space(4.0); // beat boundary — wider
                    } else if i > 0 && i % 4 == 0 {
                        ui.add_space(2.0); // sub-group divider
                    }
                    let is_active = pattern.get(i).map(|s| s.active).unwrap_or(false);
                    let is_current = abs == cursor;
                    let vel = pattern.get(i).map(|s| s.velocity).unwrap_or(0.0);
                    let enabled = abs < seq_steps;

                    ui.add_enabled_ui(enabled, |ui| {
                        if widgets::step_button(ui, is_active, is_current, vel, None) {
                            toggled = Some(abs);
                        }
                    });
                }

                if let Some(step) = toggled {
                    let s = app.state.read().clone();
                    *app.state.write() = toggle_drum_step(s, *voice, step);
                }
            });
        }

        // Bass / Accent / Slide rows
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);

        let bass_page: Vec<crate::state::TB303Step> = {
            let s = app.state.read();
            let end = (page_start + 16).min(s.sequencer.bass_pattern.len());
            s.sequencer.bass_pattern[page_start..end].to_vec()
        };

        // Bass note row
        ui.horizontal(|ui| {
            ui.add_sized(
                [80.0, 22.0],
                egui::Label::new(
                    egui::RichText::new("BASS")
                        .color(theme::SMOKE)
                        .monospace()
                        .size(8.5),
                ),
            );
            for i in 0..16usize {
                let abs = page_start + i;
                let beat_pos = (page_start + i) % time_sig_num;
                if i > 0 && beat_pos == 0 {
                    ui.add_space(4.0);
                } else if i > 0 && i % 4 == 0 {
                    ui.add_space(2.0);
                }
                let is_active = bass_page.get(i).map(|s| s.active).unwrap_or(false);
                let is_current = abs == cursor;
                let note_col = bass_page
                    .get(i)
                    .map(|s| Some(theme::note_color(s.note)))
                    .unwrap_or(None);
                ui.add_enabled_ui(abs < seq_steps, |ui| {
                    if widgets::step_button(ui, is_active, is_current, 1.0, note_col) {
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

        // Accent row — small A buttons, lit when accent=true
        ui.horizontal(|ui| {
            ui.add_sized(
                [80.0, 14.0],
                egui::Label::new(
                    egui::RichText::new("ACCENT")
                        .color(theme::IRON)
                        .monospace()
                        .size(7.5),
                ),
            );
            for i in 0..16usize {
                let abs = page_start + i;
                let beat_pos = (page_start + i) % time_sig_num;
                if i > 0 && beat_pos == 0 {
                    ui.add_space(4.0);
                } else if i > 0 && i % 4 == 0 {
                    ui.add_space(2.0);
                }
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

        // Slide row — small S buttons, lit when slide=true
        ui.horizontal(|ui| {
            ui.add_sized(
                [80.0, 14.0],
                egui::Label::new(
                    egui::RichText::new("SLIDE")
                        .color(theme::IRON)
                        .monospace()
                        .size(7.5),
                ),
            );
            for i in 0..16usize {
                let abs = page_start + i;
                let beat_pos = (page_start + i) % time_sig_num;
                if i > 0 && beat_pos == 0 {
                    ui.add_space(4.0);
                } else if i > 0 && i % 4 == 0 {
                    ui.add_space(2.0);
                }
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
    });
}
