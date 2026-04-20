// ─── ui/panels/sequencer_header.rs ───────────────────────────────────────────
// Two-line header strip above the step grid:
//   Line 1 — Bank / Chain / Steps / (right) BPM + SYNC
//   Line 2 — Time Sig / Key / Scale / SNAP / (right) Swing
// Extracted from sequencer.rs to stay under the line limit.

use super::sequencer::{fixed_label, fixed_slider};
use super::sequencer_chain::draw_pattern_chain;
use crate::state::{
    BPM_MAX, BPM_MIN, MAX_STEPS, ROOT_NAMES, Scale, set_root_note, set_scale, set_scale_snap,
};
use crate::ui::{ImpulseApp, theme};

/// Bank | Chain | Steps | BPM + SYNC row.
pub(super) fn draw_line_1(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        draw_pattern_chain(app, ui);

        ui.separator();

        // Steps
        ui.label(
            egui::RichText::new("STEPS")
                .color(theme::SMOKE)
                .monospace()
                .size(8.0),
        );
        let mut steps = app.state.read().sequencer.steps;
        if ui.small_button("−").clicked() && steps > 1 {
            steps -= 1;
            app.state.write().sequencer.steps = steps;
        }
        ui.label(
            egui::RichText::new(format!("{:02}", steps))
                .color(theme::FOG)
                .monospace()
                .size(9.0),
        );
        if ui.small_button("+").clicked() && steps < MAX_STEPS {
            steps += 1;
            let new_state = crate::state::expand_sequencer_steps(app.state.read().clone(), steps);
            *app.state.write() = new_state;
        }
        for &preset in &[16usize, 32, 64] {
            if ui
                .add_sized(
                    [20.0, 14.0],
                    egui::Button::new(
                        egui::RichText::new(format!("{}", preset))
                            .monospace()
                            .size(7.5)
                            .color(theme::IRON),
                    ),
                )
                .clicked()
            {
                let new_state =
                    crate::state::expand_sequencer_steps(app.state.read().clone(), preset);
                *app.state.write() = new_state;
            }
        }

        // Right-justified: SYNC | (pad) | BPM label | slider | value
        // Label slot width is fixed so BPM and SWING labels left-align vertically.
        const HDR_LABEL_W: f32 = 40.0;
        const HDR_SLIDER_W: f32 = 600.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (mut bpm, sync_on) = {
                let s = app.state.read();
                (s.sequencer.bpm, s.sequencer.midi_clock_sync)
            };
            // RTL order: rightmost first
            ui.label(
                egui::RichText::new(format!("{:.0}", bpm))
                    .color(if sync_on { theme::IRON } else { theme::FOG })
                    .monospace()
                    .size(9.0),
            );
            let slider_col = if sync_on { theme::FOG } else { theme::IRON };
            ui.visuals_mut().selection.bg_fill = slider_col;
            if fixed_slider(ui, HDR_SLIDER_W, 14.0, &mut bpm, BPM_MIN..=BPM_MAX).changed()
                && !sync_on
            {
                app.state.write().sequencer.bpm = bpm;
                app.push_audio_params();
            }
            fixed_label(ui, HDR_LABEL_W, 14.0, "BPM", theme::SMOKE, 8.0);
            // Padding between BPM label and SYNC button.
            ui.add_space(14.0);
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
                            .size(7.5)
                            .color(sync_color),
                    )
                    .min_size(egui::Vec2::new(34.0, 14.0)),
                )
                .clicked()
            {
                app.state.write().sequencer.midi_clock_sync = !sync_on;
            }
        });
    });
}

/// Time Sig | Key | Scale | SNAP | Swing row.
pub(super) fn draw_line_2(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let (root_note, scale, scale_snap) = {
        let s = app.state.read();
        (
            s.sequencer.root_note,
            s.sequencer.scale,
            s.sequencer.scale_snap,
        )
    };

    ui.horizontal(|ui| {
        // Time signature
        let current_ts = app.state.read().sequencer.time_sig_num;
        ui.label(
            egui::RichText::new("TIME SIG.")
                .color(theme::SMOKE)
                .monospace()
                .size(8.0),
        );
        for n in [3u8, 4, 5, 6, 7] {
            let active = n == current_ts;
            let color = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            if ui
                .add_sized(
                    [14.0, 14.0],
                    egui::Button::new(
                        egui::RichText::new(format!("{}", n))
                            .monospace()
                            .size(8.0)
                            .color(color),
                    )
                    .fill(fill),
                )
                .clicked()
            {
                app.state.write().sequencer.time_sig_num = n;
            }
        }

        ui.separator();

        // Key
        ui.label(
            egui::RichText::new("KEY")
                .color(theme::SMOKE)
                .monospace()
                .size(8.0),
        );
        for note in 0u8..12 {
            let active = note == root_note;
            let color = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            if ui
                .add_sized(
                    [18.0, 14.0],
                    egui::Button::new(
                        egui::RichText::new(ROOT_NAMES[note as usize])
                            .monospace()
                            .size(7.5)
                            .color(color),
                    )
                    .fill(fill),
                )
                .clicked()
            {
                let s = app.state.read().clone();
                *app.state.write() = set_root_note(s, note);
            }
        }

        ui.separator();

        // Scale + SNAP
        for &sc in Scale::all() {
            let active = sc == scale;
            let color = if active { theme::CHALK } else { theme::IRON };
            let fill = if active {
                egui::Color32::from_gray(50)
            } else {
                egui::Color32::TRANSPARENT
            };
            if ui
                .add_sized(
                    [38.0, 14.0],
                    egui::Button::new(
                        egui::RichText::new(sc.name())
                            .monospace()
                            .size(7.0)
                            .color(color),
                    )
                    .fill(fill),
                )
                .clicked()
            {
                let s = app.state.read().clone();
                *app.state.write() = set_scale(s, sc);
            }
        }
        let snap_color = if scale_snap { theme::CHALK } else { theme::PIT };
        let snap_fill = if scale_snap {
            egui::Color32::from_gray(50)
        } else {
            egui::Color32::TRANSPARENT
        };
        if ui
            .add_sized(
                [36.0, 14.0],
                egui::Button::new(
                    egui::RichText::new("SNAP")
                        .monospace()
                        .size(7.0)
                        .color(snap_color),
                )
                .fill(snap_fill),
            )
            .on_hover_text("Snap LLM bass notes to the active scale")
            .clicked()
        {
            let s = app.state.read().clone();
            *app.state.write() = set_scale_snap(s, !scale_snap);
        }

        // Right-justified: Swing — same slot widths as BPM row so SWING and
        // BPM labels left-align vertically.
        const HDR_LABEL_W: f32 = 40.0;
        const HDR_SLIDER_W: f32 = 600.0;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut swing = app.state.read().sequencer.swing;
            ui.label(
                egui::RichText::new(format!("{:.2}", swing))
                    .color(theme::FOG)
                    .monospace()
                    .size(8.0),
            );
            if fixed_slider(ui, HDR_SLIDER_W, 14.0, &mut swing, 0.0..=1.0).changed() {
                app.state.write().sequencer.swing = swing;
            }
            fixed_label(ui, HDR_LABEL_W, 14.0, "SWING", theme::SMOKE, 8.0);
        });
    });
}
