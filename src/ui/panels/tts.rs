// ─── ui/panels/tts.rs ─────────────────────────────────────────────────────────
// Rack module panel for EspeakNgTts and CoquiTts voice cards.
// Settings are per-module (stored in AppState.tts_modules).

use crate::state::{McVoiceChar, TtsModuleState};
use crate::ui::{ImpulseApp, theme, widgets};

/// Mutate the TtsModuleState for `mid` inside `app`.
fn with_tts(app: &mut ImpulseApp, mid: u32, f: impl FnOnce(&mut TtsModuleState)) {
    if let Some(t) = app
        .state
        .write()
        .tts_modules
        .iter_mut()
        .find(|t| t.id == mid)
    {
        f(t);
    }
}

pub fn draw_tts(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // ── Ensure module state exists ──────────────────────────────────────────
    {
        let s = app.state.read();
        if !s.tts_modules.iter().any(|t| t.id == module_id) {
            drop(s);
            let tts = TtsModuleState::new(module_id);
            app.state.write().tts_modules.push(tts);
        }
    }

    // ── Snapshot ─────────────────────────────────────────────────────────────
    let (engine, pitch, speed, amplitude, voice_char, randomise, pitch_snap, enabled) = {
        let s = app.state.read();
        let t = s.tts_modules.iter().find(|t| t.id == module_id).unwrap();
        let mod_enabled = s
            .rack
            .modules
            .iter()
            .find(|m| m.id == module_id)
            .map(|m| m.enabled)
            .unwrap_or(false);
        (
            t.engine.clone(),
            t.pitch,
            t.speed,
            t.amplitude,
            t.voice_char.clone(),
            t.randomise,
            t.pitch_snap,
            mod_enabled,
        )
    };

    // ── Engine toggle ───────────────────────────────────────────────────────
    ui.horizontal(|ui| {
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
                let v = variant.clone();
                with_tts(app, module_id, |t| t.engine = v);
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
            egui::RichText::new("Module disabled")
                .monospace()
                .size(7.5)
                .color(theme::PIT),
        );
        return;
    }

    ui.add_space(3.0);
    widgets::section_header(ui, "VOICE");

    // ── Voice character cycle button ────────────────────────────────────────
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
                let vc = chars[next_idx].0.clone();
                with_tts(app, module_id, |t| t.voice_char = vc);
            }
            resp.on_hover_text("Click to cycle voice character");
        });
    });

    ui.add_space(2.0);
    widgets::section_header(ui, "PARAMS  (0 = mode default)");

    let small_label = |text: &str| {
        egui::RichText::new(text)
            .monospace()
            .size(8.0)
            .color(theme::SMOKE)
    };

    // ── Pitch ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("PITCH"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = pitch as i32;
            if ui
                .add(egui::DragValue::new(&mut v).range(0..=99).speed(1))
                .changed()
            {
                let val = v as u8;
                with_tts(app, module_id, |t| t.pitch = val);
            }
        });
    });
    ui.label(
        egui::RichText::new("1-99; 0 = mode default")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    // ── Speed ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("SPEED"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = speed as i32;
            if ui
                .add(egui::DragValue::new(&mut v).range(0..=500).speed(2))
                .changed()
            {
                let val = v as u16;
                with_tts(app, module_id, |t| t.speed = val);
            }
        });
    });
    ui.label(
        egui::RichText::new("words/min; 0 = mode default")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    // ── Volume ──────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("VOL"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut v = amplitude as i32;
            if ui
                .add(egui::DragValue::new(&mut v).range(0..=200).speed(2))
                .changed()
            {
                let val = v as u8;
                with_tts(app, module_id, |t| t.amplitude = val);
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

    // ── Jitter ──────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("JITTER"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut r = randomise;
            if widgets::toggle_button(ui, if r { "ON" } else { "OFF" }, &mut r) {
                with_tts(app, module_id, |t| t.randomise = r);
            }
        });
    });
    ui.label(
        egui::RichText::new("+-10% pitch/speed per utterance")
            .monospace()
            .size(7.0)
            .color(theme::PIT),
    );

    // ── Pitch snap ──────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label(small_label("PITCH SNAP"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut ps = pitch_snap;
            if widgets::toggle_button(ui, if ps { "ON" } else { "OFF" }, &mut ps) {
                with_tts(app, module_id, |t| t.pitch_snap = ps);
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
