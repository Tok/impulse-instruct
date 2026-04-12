// ─── ui/panels/tts.rs ─────────────────────────────────────────────────────────
// Rack module panel for NeuTts voice cards.
// Settings are per-module (stored in AppState.tts_modules).

use crate::state::TtsModuleState;
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
    let (voice_ref, temperature, top_k, top_p, pitch_snap, enabled) = {
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
            t.voice_ref.clone(),
            t.temperature,
            t.top_k,
            t.top_p,
            t.pitch_snap,
            mod_enabled,
        )
    };

    // Force left alignment so section headers don't center
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
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
        widgets::section_header(ui, "NeuTTS Air");

        let small_label = |text: &str| {
            egui::RichText::new(text)
                .monospace()
                .size(8.0)
                .color(theme::SMOKE)
        };

        // ── Voice reference ────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(small_label("VOICE"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut v = voice_ref.clone();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut v)
                        .desired_width(80.0)
                        .font(egui::TextStyle::Monospace),
                );
                if resp.changed() {
                    let val = v.clone();
                    with_tts(app, module_id, |t| t.voice_ref = val);
                }
            });
        });
        ui.label(
            egui::RichText::new("voice reference stem (voices/{ref}.wav)")
                .monospace()
                .size(7.0)
                .color(theme::PIT),
        );

        // ── Temperature ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(small_label("TEMP"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut v = temperature;
                if ui
                    .add(egui::DragValue::new(&mut v).range(0.1..=2.0).speed(0.01))
                    .changed()
                {
                    with_tts(app, module_id, |t| t.temperature = v);
                }
            });
        });
        ui.label(
            egui::RichText::new("sampling temperature (0.5-1.5)")
                .monospace()
                .size(7.0)
                .color(theme::PIT),
        );

        // ── Top-K ──────────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(small_label("TOP-K"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut v = top_k as i32;
                if ui
                    .add(egui::DragValue::new(&mut v).range(1..=200).speed(1))
                    .changed()
                {
                    let val = v as u16;
                    with_tts(app, module_id, |t| t.top_k = val);
                }
            });
        });

        // ── Top-P ──────────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(small_label("TOP-P"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut v = top_p;
                if ui
                    .add(egui::DragValue::new(&mut v).range(0.1..=1.0).speed(0.01))
                    .changed()
                {
                    with_tts(app, module_id, |t| t.top_p = v);
                }
            });
        });

        ui.add_space(3.0);
        widgets::section_header(ui, "FX");

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
    }); // end left-aligned layout
}
