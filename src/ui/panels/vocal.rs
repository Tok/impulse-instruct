// ─── ui/panels/vocal.rs ───────────────────────────────────────────────────────
// Vocal formant synth panel.  Header (ON/OFF + VOLUME + PAN) +
// ADSR row + control row (VOWEL cycle button + MORPH +
// BRIGHTNESS + FORMANT SHIFT).  Compact 5×3 — vocal only has a
// handful of controls compared to FM ops / chiptune.

use crate::state::{ParamMode, VOCAL_VOWEL_PRESETS};
use crate::ui::{ImpulseApp, theme, widgets};

const VOWEL_LABELS: [&str; VOCAL_VOWEL_PRESETS as usize] = ["A", "E", "I", "O", "U"];

pub fn draw_vocal(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // ── Header row: ON/OFF + global volume / pan
    ui.horizontal(|ui| {
        let enabled = app.state.read().vocal.enabled;
        let btn_text = if enabled { "ON" } else { "OFF" };
        let btn_color = if enabled { theme::CHALK } else { theme::IRON };
        let btn_fill = if enabled {
            egui::Color32::from_gray(55)
        } else {
            egui::Color32::from_gray(22)
        };
        if ui
            .add_sized(
                [36.0, 20.0],
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
            app.state.write().vocal.enabled = !enabled;
            app.push_audio_params();
        }

        let mut vol = app.state.read().vocal.volume;
        if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
            app.state.write().vocal.volume = vol.clamp(0.0, 1.5);
            app.push_audio_params();
        }
        let raw_pan = app.state.read().vocal.pan;
        let mut pan = (raw_pan + 1.0) * 0.5;
        if widgets::param_control(ui, "PAN", &mut pan, ParamMode::Free, ctrl).0 {
            app.state.write().vocal.pan = (pan * 2.0 - 1.0).clamp(-1.0, 1.0);
            app.push_audio_params();
        }

        // Vowel cycle button — single button rotates A → E → I →
        // O → U with the active label displayed.  Compact for the
        // 5-option choice.
        let cur = app.state.read().vocal.vowel.min(VOCAL_VOWEL_PRESETS - 1);
        let label = VOWEL_LABELS[cur as usize];
        if ui
            .add_sized(
                [36.0, 20.0],
                egui::Button::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(11.0)
                        .color(theme::CHALK),
                )
                .fill(egui::Color32::from_gray(55)),
            )
            .on_hover_text("Vowel preset.  Click to cycle: A → E → I → O → U.")
            .clicked()
        {
            app.state.write().vocal.vowel = (cur + 1) % VOCAL_VOWEL_PRESETS;
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

    // ── Voice control row: MORPH + BRIGHTNESS + FORMANT SHIFT +
    // ADSR.  Glass-grouped so the row reads as a coherent block.
    let avail = ui.available_width();
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        ui.horizontal(|ui| {
            let mut morph = app.state.read().vocal.morph;
            if widgets::param_control(ui, "MORPH", &mut morph, ParamMode::Free, ctrl).0 {
                app.state.write().vocal.morph = morph.clamp(0.0, 1.0);
                app.push_audio_params();
            }
            let mut bright = app.state.read().vocal.brightness;
            if widgets::param_control(ui, "BRIGHT", &mut bright, ParamMode::Free, ctrl).0 {
                app.state.write().vocal.brightness = bright.clamp(0.0, 1.0);
                app.push_audio_params();
            }
            let mut shift = app.state.read().vocal.formant_shift;
            if widgets::param_control(ui, "SHIFT", &mut shift, ParamMode::Free, ctrl).0 {
                app.state.write().vocal.formant_shift = shift.clamp(0.0, 1.0);
                app.push_audio_params();
            }

            // ADSR — same map as FM ops / SAMPLER+.
            let (mut a, mut d, mut sus, mut r) = {
                let s = app.state.read();
                (
                    s.vocal.attack,
                    s.vocal.decay,
                    s.vocal.sustain,
                    s.vocal.release,
                )
            };
            let mut adsr_changed = false;
            if widgets::param_control(ui, "ATTACK", &mut a, ParamMode::Free, ctrl).0 {
                adsr_changed = true;
            }
            if widgets::param_control(ui, "DECAY", &mut d, ParamMode::Free, ctrl).0 {
                adsr_changed = true;
            }
            if widgets::param_control(ui, "SUSTAIN", &mut sus, ParamMode::Free, ctrl).0 {
                adsr_changed = true;
            }
            if widgets::param_control(ui, "RELEASE", &mut r, ParamMode::Free, ctrl).0 {
                adsr_changed = true;
            }
            if adsr_changed {
                let mut s = app.state.write();
                s.vocal.attack = a.clamp(0.0, 1.0);
                s.vocal.decay = d.clamp(0.0, 1.0);
                s.vocal.sustain = sus.clamp(0.0, 1.0);
                s.vocal.release = r.clamp(0.0, 1.0);
                drop(s);
                app.push_audio_params();
            }
        });
    });
}
