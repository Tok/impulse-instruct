// ─── ui/panels/sequencer_drums.rs ────────────────────────────────────────────
// Drum row rendering — split from sequencer.rs to stay under the 1000-line limit.

use crate::state::{
    DrumVoice, MAX_STEPS, Step, set_drum_step_ratchet, set_drum_step_velocity,
    set_drum_voice_steps, toggle_drum_step,
};
use crate::ui::{ImpulseApp, SEQ_LABEL_H, SEQ_LABEL_W, SEQ_VOL_H, SEQ_VOL_W, theme, widgets};

/// Draw the drum section (separator + collapsed chip strip + expanded rows).
/// `voices` — all DrumVoice values present in the rack.
/// `page_start`, `pad_px`, `seq_steps`, `running`, `current_step` — from draw_sequencer.
pub(super) fn draw_drum_rows(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    voices: &[DrumVoice],
    page_start: usize,
    pad_px: f32,
    seq_steps: usize,
    running: bool,
    current_step: usize,
) {
    if !voices.is_empty() {
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);
    }

    // Inactive + not-expanded voices → one compact horizontal chip strip.
    let collapsed: Vec<&DrumVoice> = voices
        .iter()
        .filter(|v| {
            let has_active = app
                .state
                .read()
                .sequencer
                .drum_patterns
                .get(v)
                .map(|p| p.iter().any(|s| s.active))
                .unwrap_or(false);
            !has_active && !app.expanded_seq_voices.contains(v)
        })
        .collect();

    if !collapsed.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("+ ")
                    .color(theme::PIT)
                    .monospace()
                    .size(7.5),
            );
            for voice in &collapsed {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(voice.label())
                                .color(theme::PIT)
                                .monospace()
                                .size(7.5),
                        )
                        .fill(egui::Color32::TRANSPARENT)
                        .min_size(egui::vec2(0.0, 13.0)),
                    )
                    .clicked()
                {
                    app.expanded_seq_voices.insert(**voice);
                }
            }
        });
    }

    // Helper: beat division spacing (same as sequencer.rs)
    let beat_div = |ui: &mut egui::Ui, i: usize| {
        let spaces = [4.0, 2.0, 2.0, 2.0];
        ui.add_space(spaces[i % 4]);
    };

    // Active / expanded drum rows
    for voice in voices {
        let has_active = {
            let s = app.state.read();
            s.sequencer
                .drum_patterns
                .get(voice)
                .map(|p| p.iter().any(|s| s.active))
                .unwrap_or(false)
        };
        if !has_active && !app.expanded_seq_voices.contains(voice) {
            continue;
        }

        let voice_steps = {
            let s = app.state.read();
            s.sequencer
                .drum_steps
                .get(voice)
                .copied()
                .unwrap_or(seq_steps)
        };
        let voice_cursor = if running {
            current_step % voice_steps.max(1)
        } else {
            usize::MAX
        };
        let pattern: Vec<Step> = {
            let s = app.state.read();
            s.sequencer
                .drum_patterns
                .get(voice)
                .map(|p| p[page_start..(page_start + 16).min(p.len())].to_vec())
                .unwrap_or_else(|| vec![Step::default(); 16])
        };

        // Tight vertical spacing for the entire voice block
        let outer_spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = 0.0;

        // ── Step buttons row ──────────────────────────────────────────────────
        let mut steps_x = 0.0f32;
        ui.horizontal(|ui| {
            let (is_muted, is_soloed) = {
                let s = app.state.read();
                (
                    s.sequencer.muted_drums.contains(voice),
                    s.sequencer.soloed_drums.contains(voice),
                )
            };
            let m_col = if is_muted {
                egui::Color32::from_gray(200)
            } else {
                egui::Color32::from_gray(50)
            };
            let s_col = if is_soloed {
                egui::Color32::from_gray(200)
            } else {
                egui::Color32::from_gray(50)
            };
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("M").monospace().size(7.5).color(m_col))
                        .min_size(egui::vec2(10.0, SEQ_LABEL_H))
                        .fill(egui::Color32::TRANSPARENT),
                )
                .clicked()
            {
                let mut s = app.state.write();
                if is_muted {
                    s.sequencer.muted_drums.remove(voice);
                } else {
                    s.sequencer.muted_drums.insert(*voice);
                }
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("S").monospace().size(7.5).color(s_col))
                        .min_size(egui::vec2(10.0, SEQ_LABEL_H))
                        .fill(egui::Color32::TRANSPARENT),
                )
                .clicked()
            {
                let mut s = app.state.write();
                if is_soloed {
                    s.sequencer.soloed_drums.remove(voice);
                } else {
                    s.sequencer.soloed_drums.insert(*voice);
                }
            }

            let label_resp = ui.add_sized(
                [SEQ_LABEL_W - 20.0, SEQ_LABEL_H],
                egui::Label::new(
                    egui::RichText::new(voice.label())
                        .color(if is_muted { theme::PIT } else { theme::SMOKE })
                        .monospace()
                        .size(8.5),
                )
                .sense(egui::Sense::click()),
            );
            label_resp.context_menu(|ui| {
                if ui.button("Copy pattern").clicked() {
                    let pattern = app
                        .state
                        .read()
                        .sequencer
                        .drum_patterns
                        .get(voice)
                        .cloned()
                        .unwrap_or_default();
                    app.drum_clipboard = Some((*voice, pattern));
                    ui.close_menu();
                }
                let can_paste = app.drum_clipboard.is_some();
                if ui
                    .add_enabled(can_paste, egui::Button::new("Paste pattern"))
                    .clicked()
                {
                    if let Some((_, ref steps)) = app.drum_clipboard.clone() {
                        let mut s = app.state.write();
                        if let Some(p) = s.sequencer.drum_patterns.get_mut(voice) {
                            let copy_len = steps.len().min(p.len());
                            p[..copy_len].clone_from_slice(&steps[..copy_len]);
                        }
                    }
                    ui.close_menu();
                }
                if ui.button("Clear pattern").clicked() {
                    let mut s = app.state.write();
                    if let Some(p) = s.sequencer.drum_patterns.get_mut(voice) {
                        for step in p.iter_mut() {
                            *step = Step::default();
                        }
                    }
                    ui.close_menu();
                }
            });

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

            let steps_resp = ui.add_sized(
                [18.0, SEQ_LABEL_H],
                egui::Label::new(
                    egui::RichText::new(format!("{:02}", voice_steps))
                        .color(if voice_steps == seq_steps {
                            theme::PIT
                        } else {
                            theme::FOG
                        })
                        .monospace()
                        .size(7.5),
                )
                .sense(egui::Sense::click_and_drag()),
            );
            if steps_resp.dragged() {
                let delta = steps_resp.drag_delta().x;
                if delta.abs() > 2.0 {
                    let new_n = ((voice_steps as i32 + delta.signum() as i32)
                        .clamp(1, MAX_STEPS as i32)) as usize;
                    let s = app.state.read().clone();
                    *app.state.write() = set_drum_voice_steps(s, *voice, new_n);
                }
            }
            if steps_resp.double_clicked() {
                let s = app.state.read().clone();
                *app.state.write() = set_drum_voice_steps(s, *voice, seq_steps);
            }

            let mut toggled = None;
            steps_x = ui.cursor().min.x; // track for sub-lane alignment
            for i in 0..16usize {
                let abs = page_start + i;
                beat_div(ui, i);
                let is_active = pattern.get(i).map(|s| s.active).unwrap_or(false);
                let is_current = abs == voice_cursor;
                let vel = pattern.get(i).map(|s| s.velocity).unwrap_or(0.0);
                ui.add_enabled_ui(abs < voice_steps, |ui| {
                    let prob = pattern.get(i).map(|s| s.probability).unwrap_or(1.0);
                    if widgets::step_button(
                        ui, is_active, is_current, vel, prob, None, None, pad_px,
                    ) {
                        toggled = Some(abs);
                    }
                });
            }
            if let Some(step) = toggled {
                let s = app.state.read().clone();
                *app.state.write() = toggle_drum_step(s, *voice, step);
            }
        });

        // ── Vel / Prob / Ratchet — single combined row (no vertical gaps) ──
        // All three indicators painted into one horizontal allocation per step.
        let vel_h = 5.0_f32;
        let prob_h = 3.0_f32;
        let ratch_h = 4.0_f32;
        let gap = 2.0_f32;
        let combined_h = vel_h + gap + prob_h + gap + ratch_h;
        let mut vel_changed: Option<(usize, f32)> = None;
        let mut ratchet_changed: Option<(usize, u8)> = None;
        ui.horizontal(|ui| {
            let row_left = ui.cursor().min.x;
            let spacer = (steps_x - row_left).max(0.0);
            if spacer > 0.0 {
                ui.add_space(spacer);
            }
            for i in 0..16usize {
                beat_div(ui, i);
                let abs = page_start + i;
                let vel = pattern.get(i).map(|s| s.velocity).unwrap_or(1.0);
                let prob = pattern.get(i).map(|s| s.probability).unwrap_or(1.0);
                let ratchet = pattern.get(i).map(|s| s.ratchet).unwrap_or(1);
                let is_active = pattern.get(i).map(|s| s.active).unwrap_or(false);
                let (rect, resp) = ui
                    .add_enabled_ui(abs < voice_steps, |ui| {
                        ui.allocate_exact_size(
                            egui::vec2(pad_px, combined_h),
                            egui::Sense::click_and_drag(),
                        )
                    })
                    .inner;
                if ui.is_rect_visible(rect) {
                    let p = ui.painter();
                    let x = rect.min.x;
                    let w = pad_px - 1.0;
                    // Velocity bar (top)
                    let vy = rect.min.y;
                    let vcol = if is_active { 70u8 } else { 28 };
                    p.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x, vy + vel_h * (1.0 - vel)),
                            egui::vec2(w, vel_h * vel),
                        ),
                        egui::Rounding::ZERO,
                        egui::Color32::from_gray(vcol),
                    );
                    // Probability bar (middle)
                    let py = vy + vel_h + gap;
                    let pcol = if is_active { 50u8 } else { 20 };
                    p.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x, py + prob_h * (1.0 - prob)),
                            egui::vec2(w, prob_h * prob),
                        ),
                        egui::Rounding::ZERO,
                        egui::Color32::from_gray(pcol),
                    );
                    // Ratchet ticks (bottom)
                    let ry = py + prob_h + gap;
                    let tick_w = w / 4.0;
                    for t in 0..4u8 {
                        let lit = t < ratchet;
                        let col = if lit && is_active {
                            90u8
                        } else if lit {
                            40
                        } else {
                            15
                        };
                        p.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(x + t as f32 * tick_w, ry),
                                egui::vec2((tick_w - 1.0).max(1.0), ratch_h),
                            ),
                            egui::Rounding::ZERO,
                            egui::Color32::from_gray(col),
                        );
                    }
                }
                // Drag = velocity, click = ratchet cycle
                if resp.dragged() && abs < voice_steps {
                    let delta = -resp.drag_delta().y / (combined_h * 4.0);
                    vel_changed = Some((abs, (vel + delta).clamp(0.05, 1.0)));
                }
                if resp.clicked() && abs < voice_steps {
                    let next = if ratchet >= 4 { 1 } else { ratchet + 1 };
                    ratchet_changed = Some((abs, next));
                }
            }
        });
        if let Some((step, v)) = vel_changed {
            let s = app.state.read().clone();
            *app.state.write() = set_drum_step_velocity(s, *voice, step, v);
        }
        if let Some((step, r)) = ratchet_changed {
            let s = app.state.read().clone();
            *app.state.write() = set_drum_step_ratchet(s, *voice, step, r);
        }
        // Restore normal vertical spacing between voices
        ui.spacing_mut().item_spacing = outer_spacing;
    }
}
