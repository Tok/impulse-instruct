// ─── ui/panels/lfo.rs ─────────────────────────────────────────────────────────
// Global LFO panel — 4 wireable LFOs.

use crate::state::{LfoSlot, LfoTarget, LfoWaveform};
use crate::ui::{ImpulseApp, theme, widgets};

const TARGET_LABELS: &[(&str, LfoTarget)] = &[
    ("NONE", LfoTarget::None),
    ("CUT", LfoTarget::BassCutoff),
    ("RES", LfoTarget::BassResonance),
    ("PITCH", LfoTarget::BassPitch),
    ("VOL", LfoTarget::BassVolume),
    ("REV", LfoTarget::ReverbMix),
    ("DLY.T", LfoTarget::DelayTime),
    ("DLY.FB", LfoTarget::DelayFeedback),
    ("CHR.MX", LfoTarget::ChorusMix),
    ("CHR.RT", LfoTarget::ChorusRate),
    ("K808P", LfoTarget::Kick808Pitch),
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

pub fn draw_lfo(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    widgets::section_header(ui, "LFO — GLOBAL MODULATION");

    // Snapshot all 4 LFO slots
    let slots: [LfoSlot; 4] = app.state.read().lfo;

    for (i, &slot) in slots.iter().enumerate() {
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

            // Enable toggle
            if ui.selectable_label(enabled, "ON").clicked() {
                enabled = !enabled;
                app.state.write().lfo[i].enabled = enabled;
                app.push_audio_params();
            }

            ui.add_space(4.0);

            // Waveform buttons
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

            // Rate
            ui.label(
                egui::RichText::new("RATE")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            let resp = ui.add(
                egui::DragValue::new(&mut rate)
                    .range(0.0..=1.0)
                    .speed(0.005)
                    .fixed_decimals(2),
            );
            if resp.changed() {
                app.state.write().lfo[i].rate = rate;
                app.push_audio_params();
            }

            // Rate Hz label
            let hz = 0.01 + rate * 19.99;
            ui.label(
                egui::RichText::new(format!("{:.2}Hz", hz))
                    .color(theme::IRON)
                    .monospace()
                    .size(9.0),
            );

            ui.add_space(4.0);

            // Depth
            ui.label(
                egui::RichText::new("DEPTH")
                    .color(theme::SMOKE)
                    .monospace()
                    .size(9.0),
            );
            let resp = ui.add(
                egui::DragValue::new(&mut depth)
                    .range(0.0..=1.0)
                    .speed(0.005)
                    .fixed_decimals(2),
            );
            if resp.changed() {
                app.state.write().lfo[i].depth = depth;
                app.push_audio_params();
            }

            ui.add_space(4.0);

            // Target cycle button
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
        });

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
}
