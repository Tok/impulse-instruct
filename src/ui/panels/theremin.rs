// ─── ui/panels/theremin.rs ────────────────────────────────────────────────────
// Theremin voice panel — XY pad as the primary instrument, four
// supporting knobs.  Absurd-queue UI; intentionally minimal so the
// pad dominates the card visually (the user's hands belong on it).
//
// Layout:
//   Row 1: ON/OFF toggle + small "X / Y" label hint
//   Row 2: XY pad (square, takes most of the card)
//   Row 3: PORTA / BRIGHT / VOL / PAN knobs

use crate::state::ParamMode;
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_theremin(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    // Row 1: ON/OFF + hint label.
    ui.horizontal(|ui| {
        let enabled = app.state.read().theremin.enabled;
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
            app.state.write().theremin.enabled = !enabled;
            app.push_audio_params();
        }
        ui.label(
            egui::RichText::new("X = pitch    Y = volume")
                .monospace()
                .size(8.0)
                .color(theme::ASH),
        );
    });

    ui.add_space(2.0);

    // Row 2: XY pad — pitch (X, log freq 50–2000 Hz) + volume (Y).
    let avail_w = ui.available_width();
    let pad_size = avail_w.clamp(80.0, 160.0);
    let mut x = app.state.read().theremin.x;
    let mut y = app.state.read().theremin.y;
    let (changed, _pair) = widgets::xy_pad(
        ui,
        "theremin_pad",
        "X",
        "Y",
        &mut x,
        &mut y,
        pad_size,
        false, // not locked
        1,     // single pair — pitch + volume only
    );
    if changed {
        let mut s = app.state.write();
        s.theremin.x = x.clamp(0.0, 1.0);
        s.theremin.y = y.clamp(0.0, 1.0);
        drop(s);
        app.push_audio_params();
    }

    ui.add_space(2.0);

    // Row 3: PORTA / BRIGHT / VOL / PAN.  Centred in a glass group
    // for visual consistency with the rest of the voice cards.
    let gw = ui.available_width().max(120.0);
    widgets::glass_group_fill(ui, gw, gw, |ui| {
        widgets::centered_row(ui, |ui| {
            {
                let mut v = app.state.read().theremin.portamento;
                if widgets::param_control(ui, "PORTA", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().theremin.portamento = v.clamp(0.0, 1.0);
                    app.push_audio_params();
                }
            }
            {
                let mut v = app.state.read().theremin.brightness;
                if widgets::param_control(ui, "BRIGHT", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().theremin.brightness = v.clamp(0.0, 1.0);
                    app.push_audio_params();
                }
            }
            {
                let mut v = app.state.read().theremin.volume;
                if widgets::param_control(ui, "VOL", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().theremin.volume = v.clamp(0.0, 1.5);
                    app.push_audio_params();
                }
            }
            {
                let raw = app.state.read().theremin.pan;
                let mut v = (raw + 1.0) * 0.5;
                if widgets::param_control(ui, "PAN", &mut v, ParamMode::Free, ctrl).0 {
                    app.state.write().theremin.pan = (v * 2.0 - 1.0).clamp(-1.0, 1.0);
                    app.push_audio_params();
                }
            }
        });
    });
}
