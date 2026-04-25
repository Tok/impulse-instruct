// ─── ui/panels/pluck.rs ──────────────────────────────────────────────────────
// Karplus-Strong pluck voice panel.

use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_pluck(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().pluck.enabled;
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
            app.state.write().pluck.enabled = !enabled;
            app.push_audio_params();
        }

        ui.label(
            egui::RichText::new("Karplus-Strong string")
                .color(theme::PIT)
                .monospace()
                .size(7.5),
        );
    });

    ui.add_space(2.0);

    // Two glass groups: TONE (damping / brightness) and MIX (volume / pan /
    // pitch offset).  Pitch offset is displayed as a ±24 st knob via a
    // 0..1 normalised mapping (0.5 = zero offset detent) so the stock
    // knob widget handles it without a bipolar variant.
    let gw = widgets::even_group_width(ui, 2);
    let group_h = widgets::glass_group_height(ctrl, 60.0);
    ui.horizontal(|ui| {
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("TONE")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().pluck.damping;
                    if widgets::param_control(ui, "DAMPING", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().pluck.damping = v;
                        app.push_audio_params();
                    }
                }
                {
                    let mut v = app.state.read().pluck.brightness;
                    if widgets::param_control(ui, "BRIGHT", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().pluck.brightness = v;
                        app.push_audio_params();
                    }
                }
            });
        });
        widgets::glass_group_fill(ui, gw, gw, |ui| {
            ui.set_min_height(group_h);
            ui.label(
                egui::RichText::new("MIX")
                    .color(theme::FOG)
                    .monospace()
                    .size(9.5),
            );
            widgets::centered_row(ui, |ui| {
                {
                    let mut v = app.state.read().pluck.volume;
                    if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().pluck.volume = v;
                        app.push_audio_params();
                    }
                }
                {
                    let raw = app.state.read().pluck.pan;
                    let mut v = (raw + 1.0) * 0.5;
                    if widgets::param_control(ui, "PAN", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().pluck.pan = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                        app.push_audio_params();
                    }
                }
                {
                    // ±24 semitone pitch offset displayed via the 0..1
                    // knob with 0.5 at the zero-offset detent.
                    let raw = app.state.read().pluck.pitch_offset_semi;
                    let mut v = (raw / 48.0) + 0.5;
                    if widgets::param_control(ui, "PITCH", &mut v, ParamMode::Free, ctrl).0 {
                        app.state.write().pluck.pitch_offset_semi =
                            ((v - 0.5) * 48.0).clamp(-24.0, 24.0);
                        app.push_audio_params();
                    }
                }
            });
        });
    });
}
