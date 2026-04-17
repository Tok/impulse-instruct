// ─── ui/panels/lfo.rs ─────────────────────────────────────────────────────────
// LFO panel — 4 slots + Free EG.
// `draw_lfo`      — full panel (legacy tab path)
// `draw_lfo_slot` — single slot card (rack canvas path)

use crate::state::{FreeEg, LfoSlot, LfoTarget, LfoWaveform};
use crate::ui::{ImpulseApp, theme, widgets};

const TARGET_LABELS: &[(&str, LfoTarget)] = &[
    ("NONE", LfoTarget::None),
    ("CUTOFF", LfoTarget::BassCutoff),
    ("RESONANCE", LfoTarget::BassResonance),
    ("PITCH", LfoTarget::BassPitch),
    ("VOLUME", LfoTarget::BassVolume),
    ("REVERB", LfoTarget::ReverbMix),
    ("DELAY.TIME", LfoTarget::DelayTime),
    ("DELAY.FBK", LfoTarget::DelayFeedback),
    ("CHORUS.MIX", LfoTarget::ChorusMix),
    ("CHORUS.RATE", LfoTarget::ChorusRate),
    ("808.KICK.PITCH", LfoTarget::Kick808Pitch),
    ("PHASER.RATE", LfoTarget::PhaserRate),
    ("PHASER.DEPTH", LfoTarget::PhaserDepth),
    ("DRIVE", LfoTarget::DistortionDrive),
    ("MASTER", LfoTarget::MasterVolume),
    ("AN1X.CUTOFF", LfoTarget::An1xCutoff),
    ("AN1X.PITCH", LfoTarget::An1xPitch),
];

fn target_label(t: &LfoTarget) -> &'static str {
    TARGET_LABELS
        .iter()
        .find(|(_, v)| v == t)
        .map(|(l, _)| *l)
        .unwrap_or("NONE")
}

fn next_target(t: &LfoTarget) -> LfoTarget {
    let idx = TARGET_LABELS.iter().position(|(_, v)| v == t).unwrap_or(0);
    TARGET_LABELS[(idx + 1) % TARGET_LABELS.len()].1
}

// ─── Full panel (legacy) ──────────────────────────────────────────────────────

pub fn draw_lfo(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let slots: [LfoSlot; 4] = app.state.read().lfo;
    for (i, slot) in slots.iter().enumerate() {
        draw_lfo_slot_row(app, ui, i, slot);
        ui.add_space(2.0);
    }

    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Click target button to cycle through destinations. Drag RATE/DEPTH to adjust.",
        )
        .color(theme::IRON)
        .monospace()
        .size(8.5),
    );

    ui.add_space(10.0);
    draw_free_eg(app, ui);
}

// ─── Single slot row (shared by draw_lfo and draw_lfo_slot) ──────────────────

fn draw_lfo_slot_row(app: &mut ImpulseApp, ui: &mut egui::Ui, i: usize, slot: &LfoSlot) {
    let mut enabled = slot.enabled;
    let mut rate = slot.rate;
    let mut depth = slot.depth;

    let lfo_label = format!("LFO {}", i + 1);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&lfo_label)
                .color(if enabled { theme::CHALK } else { theme::IRON })
                .monospace()
                .size(9.5),
        );

        if ui.selectable_label(enabled, "ON").clicked() {
            enabled = !enabled;
            app.state.write().lfo[i].enabled = enabled;
            app.push_audio_params();
        }

        ui.add_space(4.0);

        for (label, wave) in [
            ("SIN", LfoWaveform::Sine),
            ("TRI", LfoWaveform::Triangle),
            ("SAW", LfoWaveform::Saw),
            ("~", LfoWaveform::InvSaw),
            ("SQR", LfoWaveform::Square),
            ("S&H", LfoWaveform::SampleAndHold),
        ] {
            let active = slot.waveform == wave;
            if ui.selectable_label(active, label).clicked() {
                app.state.write().lfo[i].waveform = wave;
                app.push_audio_params();
            }
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("RATE")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if ui
            .add(
                egui::DragValue::new(&mut rate)
                    .range(0.0..=1.0)
                    .speed(0.005)
                    .fixed_decimals(2),
            )
            .changed()
        {
            app.state.write().lfo[i].rate = rate;
            app.push_audio_params();
        }
        let hz = 0.01 + rate * 19.99;
        ui.label(
            egui::RichText::new(format!("{:.2}Hz", hz))
                .color(theme::IRON)
                .monospace()
                .size(9.0),
        );

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("DEPTH")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        if ui
            .add(
                egui::DragValue::new(&mut depth)
                    .range(0.0..=1.0)
                    .speed(0.005)
                    .fixed_decimals(2),
            )
            .changed()
        {
            app.state.write().lfo[i].depth = depth;
            app.push_audio_params();
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("->")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        let t_label = target_label(&slot.target);
        if ui
            .button(
                egui::RichText::new(t_label)
                    .color(if slot.target == LfoTarget::None {
                        theme::IRON
                    } else {
                        theme::CHALK
                    })
                    .monospace()
                    .size(9.5),
            )
            .clicked()
        {
            let next = next_target(&slot.target);
            app.state.write().lfo[i].target = next;
            app.push_audio_params();
        }

        // Animated LFO waveform preview
        let shape = match slot.waveform {
            LfoWaveform::Sine => 0u8,
            LfoWaveform::Triangle => 1,
            LfoWaveform::Saw => 2,
            LfoWaveform::InvSaw => 3,
            LfoWaveform::Square => 4,
            LfoWaveform::SampleAndHold => 5,
        };
        widgets::lfo_preview(ui, shape, rate, depth, t_label, 80.0, 28.0);
    });
}

// ─── Single slot card (rack canvas path) ─────────────────────────────────────

/// Draw one LFO slot as a compact vertical card for the rack canvas.
pub fn draw_lfo_slot(app: &mut ImpulseApp, ui: &mut egui::Ui, slot_idx: usize) {
    if slot_idx >= 4 {
        return;
    }
    let slot = app.state.read().lfo[slot_idx];
    let i = slot_idx;
    let mut enabled = slot.enabled;
    let mut rate = slot.rate;
    let mut depth = slot.depth;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let ctrl_lg = ctrl.phi_bigger();

    // Row 1: ON | → [TARGET] | waveform selectors
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui.selectable_label(enabled, "ON").clicked() {
            enabled = !enabled;
            app.state.write().lfo[i].enabled = enabled;
            app.push_audio_params();
        }
        ui.separator();
        // Target button — fixed width to prevent layout shift
        let t_label = target_label(&slot.target);
        if ui
            .add_sized(
                [60.0, 18.0],
                egui::Button::new(
                    egui::RichText::new(format!("→{}", t_label))
                        .color(if slot.target == LfoTarget::None {
                            theme::IRON
                        } else {
                            theme::CHALK
                        })
                        .monospace()
                        .size(8.5),
                ),
            )
            .clicked()
        {
            app.state.write().lfo[i].target = next_target(&slot.target);
            app.push_audio_params();
        }
        ui.separator();
        // Waveform selectors — tighter spacing
        for (label, wave) in [
            ("SIN", LfoWaveform::Sine),
            ("TRI", LfoWaveform::Triangle),
            ("SAW", LfoWaveform::Saw),
            ("SQR", LfoWaveform::Square),
        ] {
            let active = slot.waveform == wave;
            if ui.selectable_label(active, label).clicked() {
                app.state.write().lfo[i].waveform = wave;
                app.push_audio_params();
            }
        }
    });
    ui.add_space(2.0);

    // Row 2: RATE + DEPTH knobs in a glass pane (large, centered)
    {
        let gw = ui.available_width();
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.spacing_mut().item_spacing.x = super::KNOB_SPACING;
            widgets::centered_row(ui, |ui| {
                if widgets::param_control(
                    ui,
                    "RATE",
                    &mut rate,
                    crate::state::ParamMode::Free,
                    ctrl_lg,
                )
                .0
                {
                    app.state.write().lfo[i].rate = rate;
                    app.push_audio_params();
                }
                if widgets::param_control(
                    ui,
                    "DEPTH",
                    &mut depth,
                    crate::state::ParamMode::Free,
                    ctrl_lg,
                )
                .0
                {
                    app.state.write().lfo[i].depth = depth;
                    app.push_audio_params();
                }
                let hz = 0.01 + rate * 19.99;
                ui.label(
                    egui::RichText::new(format!("{:.1}Hz", hz))
                        .color(theme::IRON)
                        .monospace()
                        .size(8.0),
                );
            });
        });
    }

    // Waveform preview — full width, bottom-aligned
    let shape = match slot.waveform {
        LfoWaveform::Sine => 0u8,
        LfoWaveform::Triangle => 1,
        LfoWaveform::Saw => 2,
        LfoWaveform::InvSaw => 3,
        LfoWaveform::Square => 4,
        LfoWaveform::SampleAndHold => 5,
    };
    let t_label = target_label(&slot.target);
    let viz_w = (ui.available_width() - 4.0).max(40.0); // slight inset to avoid overflow
    let viz_h = ui.available_height().max(50.0);
    widgets::lfo_preview(ui, shape, rate, depth, t_label, viz_w, viz_h);
}

// ─── Free EG ─────────────────────────────────────────────────────────────────

fn draw_free_eg(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    widgets::section_header(ui, "FREE EG");

    let eg = app.state.read().free_eg.clone();

    ui.horizontal(|ui| {
        let mut enabled = eg.enabled;
        if widgets::toggle_button(ui, if enabled { "ON" } else { "OFF" }, &mut enabled) {
            app.state.write().free_eg.enabled = enabled;
        }
        ui.add_space(8.0);
        let mut looping = eg.loop_mode;
        if widgets::toggle_button(ui, if looping { "LOOP" } else { "1-SHOT" }, &mut looping) {
            app.state.write().free_eg.loop_mode = looping;
        }
        ui.add_space(8.0);
        let tgt_label = target_label(&eg.target);
        if ui.small_button(format!("→ {tgt_label}")).clicked() {
            app.state.write().free_eg.target = next_target(&eg.target);
        }
    });

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("PERIOD")
                .monospace()
                .size(8.5)
                .color(theme::FOG),
        );
        let mut period = eg.period;
        if ui
            .add(
                egui::Slider::new(&mut period, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            app.state.write().free_eg.period = period;
        }
        let secs = 0.5 * 64.0_f32.powf(eg.period);
        ui.label(
            egui::RichText::new(format!("{:.1}s", secs))
                .monospace()
                .size(8.0)
                .color(theme::IRON),
        );
    });
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("DEPTH ")
                .monospace()
                .size(8.5)
                .color(theme::FOG),
        );
        let mut depth = eg.depth;
        if ui
            .add(
                egui::Slider::new(&mut depth, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            )
            .changed()
        {
            app.state.write().free_eg.depth = depth;
        }
    });

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("SHAPE  (drag bars)")
            .monospace()
            .size(8.0)
            .color(theme::IRON),
    );
    ui.horizontal(|ui| {
        let bar_w = 14.0_f32;
        let bar_max_h = 36.0_f32;
        let mut changed_idx: Option<(usize, f32)> = None;
        for i in 0..8usize {
            let val = eg.values[i];
            let (rect, resp) =
                ui.allocate_exact_size(egui::vec2(bar_w, bar_max_h + 4.0), egui::Sense::drag());
            if ui.is_rect_visible(rect) {
                let fill_h = bar_max_h * val;
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + 1.0, rect.max.y - 4.0 - fill_h),
                    egui::vec2(bar_w - 2.0, fill_h.max(1.0)),
                );
                let col = if eg.enabled {
                    theme::ASH
                } else {
                    egui::Color32::from_gray(40)
                };
                ui.painter()
                    .rect_filled(bar_rect, egui::Rounding::ZERO, col);
                ui.painter().rect_stroke(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.min.x + 1.0, rect.max.y - 4.0 - bar_max_h),
                        egui::vec2(bar_w - 2.0, bar_max_h),
                    ),
                    egui::Rounding::ZERO,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(30)),
                );
            }
            if resp.dragged() {
                let delta = -resp.drag_delta().y / bar_max_h;
                let new_val = (val + delta).clamp(0.0, 1.0);
                changed_idx = Some((i, new_val));
            }
        }
        if let Some((idx, v)) = changed_idx {
            app.state.write().free_eg.values[idx] = v;
            app.push_audio_params();
        }
    });

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Draw any shape. Period sets loop speed. Depth scales modulation.")
            .color(theme::IRON)
            .monospace()
            .size(8.0),
    );
}

// Suppress unused import warning
const _: fn() = || {
    let _: FreeEg = FreeEg::default();
};
