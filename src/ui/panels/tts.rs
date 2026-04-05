// ─── ui/panels/tts.rs ─────────────────────────────────────────────────────────
// Rack module panel for EspeakNgTts and CoquiTts voice cards.
// Both engines share the same controls; the engine is toggled inline.

use crate::state::McVoiceChar;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_tts(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // ── snapshot ──────────────────────────────────────────────────────────────
    let (enabled, engine, pitch, speed, amplitude, voice_char, randomise, pitch_snap) = {
        let s = app.state.read();
        (
            s.llm.tts_enabled,
            s.llm.tts_engine.clone(),
            s.llm.tts_pitch,
            s.llm.tts_speed,
            s.llm.tts_amplitude,
            s.llm.tts_voice_char.clone(),
            s.llm.tts_randomise,
            s.llm.tts_pitch_snap,
        )
    };

    // ── enable / engine row ───────────────────────────────────────────────────
    ui.horizontal(|ui| {
        let btn_text = if enabled { "ON" } else { "OFF" };
        let btn_color = if enabled { theme::CHALK } else { theme::IRON };
        let btn_fill = if enabled {
            egui::Color32::from_gray(55)
        } else {
            egui::Color32::from_gray(22)
        };
        if ui
            .add_sized(
                [32.0, 18.0],
                egui::Button::new(
                    egui::RichText::new(btn_text)
                        .monospace()
                        .size(8.5)
                        .color(btn_color),
                )
                .fill(btn_fill),
            )
            .clicked()
        {
            app.state.write().llm.tts_enabled = !enabled;
        }

        ui.add_space(4.0);

        // Engine toggle: espeak / coqui
        use crate::state::TtsEngine;
        for (label, variant) in &[("ESP", TtsEngine::EspeakNg), ("COQ", TtsEngine::CoquiTts)] {
            let active = engine == *variant;
            let fill = if active {
                egui::Color32::from_gray(55)
            } else {
                egui::Color32::from_gray(22)
            };
            let color = if active { theme::CHALK } else { theme::IRON };
            let resp = ui.add_sized(
                [28.0, 18.0],
                egui::Button::new(
                    egui::RichText::new(*label)
                        .monospace()
                        .size(8.0)
                        .color(color),
                )
                .fill(fill),
            );
            if resp.clicked() {
                app.state.write().llm.tts_engine = variant.clone();
            }
            let hover = match variant {
                TtsEngine::EspeakNg => "espeak-ng: always available, robotic character",
                TtsEngine::CoquiTts => "Coqui TTS: neural voice, requires `tts` CLI in PATH",
            };
            resp.on_hover_text(hover);
        }
    });

    if !enabled {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("TTS off - enable to configure")
                .monospace()
                .size(7.5)
                .color(theme::PIT),
        );
        return;
    }

    ui.add_space(3.0);
    widgets::section_header(ui, "VOICE");

    // ── voice character cycle button ──────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("CHAR")
                .monospace()
                .size(8.0)
                .color(theme::SMOKE),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let chars = [
                (McVoiceChar::Auto, "AUTO"),
                (McVoiceChar::JungleMc, "JUNGLE MC"),
                (McVoiceChar::RaveAnnouncer, "RAVE"),
                (McVoiceChar::Robot, "ROBOT"),
                (McVoiceChar::SmoothDj, "SMOOTH DJ"),
            ];
            let cur_idx = chars
                .iter()
                .position(|(c, _)| *c == voice_char)
                .unwrap_or(0);
            let next_idx = (cur_idx + 1) % chars.len();
            let resp = ui.small_button(
                egui::RichText::new(chars[cur_idx].1)
                    .monospace()
                    .size(8.0)
                    .color(theme::FOG),
            );
            if resp.clicked() {
                app.state.write().llm.tts_voice_char = chars[next_idx].0.clone();
            }
            resp.on_hover_text("Click to cycle voice character");
        });
    });

    ui.add_space(2.0);
    widgets::section_header(ui, "PARAMS  (0 = mode default)");

    // ── pitch / speed / volume ────────────────────────────────────────────────
    let small_label = |text: &str| {
        egui::RichText::new(text)
            .monospace()
            .size(8.0)
            .color(theme::SMOKE)
    };

    ui.horizontal(|ui| {
        ui.label(small_label("PITCH"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = pitch as i32;
            if ui
                .add(egui::DragValue::new(&mut v).range(0..=99).speed(1))
                .changed()
            {
                app.state.write().llm.tts_pitch = v as u8;
            }
        });
    });
    ui.label(
        egui::RichText::new("1-99; 0 = mode default")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    ui.horizontal(|ui| {
        ui.label(small_label("SPEED"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = speed as i32;
            if ui
                .add(egui::DragValue::new(&mut v).range(0..=500).speed(2))
                .changed()
            {
                app.state.write().llm.tts_speed = v as u16;
            }
        });
    });
    ui.label(
        egui::RichText::new("words/min; 0 = mode default")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    ui.horizontal(|ui| {
        ui.label(small_label("VOL"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = amplitude as i32;
            if ui
                .add(egui::DragValue::new(&mut v).range(0..=200).speed(2))
                .changed()
            {
                app.state.write().llm.tts_amplitude = v as u8;
            }
        });
    });
    ui.label(
        egui::RichText::new("0-200; 0 = default (100)")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    ui.add_space(3.0);
    widgets::section_header(ui, "FX");

    // ── jitter + pitch snap ───────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("JITTER"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut r = randomise;
            if widgets::toggle_button(ui, if r { "ON" } else { "OFF" }, &mut r) {
                app.state.write().llm.tts_randomise = r;
            }
        });
    });
    ui.label(
        egui::RichText::new("+-10% pitch/speed per utterance")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    ui.horizontal(|ui| {
        ui.label(small_label("PITCH SNAP"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut ps = pitch_snap;
            if widgets::toggle_button(ui, if ps { "ON" } else { "OFF" }, &mut ps) {
                app.state.write().llm.tts_pitch_snap = ps;
            }
        });
    });
    ui.label(
        egui::RichText::new("snap voice to nearest in-key note")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );
}
