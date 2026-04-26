// ─── ui/panels/modal.rs ───────────────────────────────────────────────────────
// Modal / struck physical-model voice panel.  Header row
// (ON/OFF + VOLUME + PAN + BRIGHTNESS + DECAY + preset cycle)
// above an 8-bar drawable mode-amplitude histogram (mirrors the
// Additive panel's UX so users land on familiar controls when
// switching between the two spectrum-shaping voices).

use crate::state::{MODAL_MODES, MODAL_RATIO_PRESETS, ParamMode};
use crate::ui::{ImpulseApp, theme, widgets};

const PRESET_LABELS: [&str; MODAL_RATIO_PRESETS as usize] =
    ["HARMONIC", "BELL", "TUBULAR", "METAL"];

pub fn draw_modal(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().modal.enabled;
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
            app.state.write().modal.enabled = !enabled;
            app.push_audio_params();
        }

        // Voice volume + pan + brightness + decay scale.
        let mut vol = app.state.read().modal.volume;
        if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
            app.state.write().modal.volume = vol.clamp(0.0, 1.5);
            app.push_audio_params();
        }
        let raw_pan = app.state.read().modal.pan;
        let mut pan = (raw_pan + 1.0) * 0.5;
        if widgets::param_control(ui, "PAN", &mut pan, ParamMode::Free, ctrl).0 {
            app.state.write().modal.pan = (pan * 2.0 - 1.0).clamp(-1.0, 1.0);
            app.push_audio_params();
        }
        let mut bright = app.state.read().modal.brightness;
        if widgets::param_control(ui, "BRIGHT", &mut bright, ParamMode::Free, ctrl).0 {
            app.state.write().modal.brightness = bright.clamp(0.0, 1.0);
            app.push_audio_params();
        }
        let mut decay = app.state.read().modal.decay_scale;
        if widgets::param_control(ui, "DECAY", &mut decay, ParamMode::Free, ctrl).0 {
            app.state.write().modal.decay_scale = decay.clamp(0.0, 1.0);
            app.push_audio_params();
        }

        // Preset cycle button — single button rotates through
        // HARMONIC → BELL → TUBULAR → METAL with the active
        // preset's name displayed.  More compact than four
        // chips for the same scant 4-option choice.
        let cur = app
            .state
            .read()
            .modal
            .ratio_preset
            .min(MODAL_RATIO_PRESETS - 1);
        let label = PRESET_LABELS[cur as usize];
        if ui
            .add_sized(
                [76.0, 20.0],
                egui::Button::new(
                    egui::RichText::new(label)
                        .monospace()
                        .size(9.0)
                        .color(theme::CHALK),
                )
                .fill(egui::Color32::from_gray(55)),
            )
            .on_hover_text("Mode ratio preset.  Click to cycle: HARMONIC → BELL → TUBULAR → METAL.")
            .clicked()
        {
            app.state.write().modal.ratio_preset = (cur + 1) % MODAL_RATIO_PRESETS;
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

    // Mode-amplitude histogram — same drawable UX as the
    // additive panel.  8 bars instead of 16; fundamental brighter
    // than the rest so the user knows where the played-note
    // strike tone sits.
    let avail = ui.available_width();
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        let mut levels = app.state.read().modal.levels;
        if draw_mode_histogram(ui, &mut levels, avail - 14.0, 90.0) {
            let mut s = app.state.write();
            s.modal.levels = levels;
            drop(s);
            app.push_audio_params();
        }
    });
}

fn draw_mode_histogram(
    ui: &mut egui::Ui,
    levels: &mut [f32; MODAL_MODES],
    width: f32,
    height: f32,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::Vec2::new(width, height),
        egui::Sense::click_and_drag(),
    );
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let painter = ui.painter_at(rect);
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));

    let n = MODAL_MODES as f32;
    let col_w = rect.width() / n;
    let bar_w = (col_w - 2.0).max(1.0);
    let mut changed = false;

    if response.dragged_by(egui::PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
        && rect.contains(pos)
    {
        let col = ((pos.x - rect.min.x) / col_w).floor() as usize;
        if col < MODAL_MODES {
            let frac = ((rect.max.y - pos.y) / rect.height()).clamp(0.0, 1.0);
            if (levels[col] - frac).abs() > 1e-4 {
                levels[col] = frac;
                changed = true;
            }
        }
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && rect.contains(pos)
    {
        let col = ((pos.x - rect.min.x) / col_w).floor() as usize;
        if col < MODAL_MODES {
            let frac = ((rect.max.y - pos.y) / rect.height()).clamp(0.0, 1.0);
            levels[col] = frac;
            changed = true;
        }
    }

    for (i, level) in levels.iter().enumerate() {
        let x_lo = rect.min.x + i as f32 * col_w + 1.0;
        let h = (level.clamp(0.0, 1.0) * rect.height()).max(0.5);
        let bar = egui::Rect::from_min_max(
            egui::Pos2::new(x_lo, rect.max.y - h),
            egui::Pos2::new(x_lo + bar_w, rect.max.y),
        );
        let shade = if i == 0 {
            theme::FOG
        } else {
            egui::Color32::from_gray(140)
        };
        painter.rect_filled(bar, egui::Rounding::same(1.0), shade);
        // Label every column — only 8 modes, all comfortably fit.
        painter.text(
            egui::Pos2::new(x_lo + bar_w * 0.5, rect.min.y + 1.0),
            egui::Align2::CENTER_TOP,
            format!("{}", i + 1),
            egui::FontId::monospace(7.0),
            theme::ASH,
        );
    }

    changed
}
