// ─── ui/panels/additive.rs ────────────────────────────────────────────────────
// Additive synth voice panel.  Header row (ON/OFF + ADSR + VOLUME
// + PAN) above a 16-bar harmonic histogram where each vertical
// bar is the level of one partial.  Click + drag inside the
// histogram to set or sweep a partial's amplitude — the
// "drawing the spectrum" UX the voice description called for.

use crate::state::{ADDITIVE_HARMONICS, ParamMode};
use crate::ui::{ImpulseApp, theme, widgets};

pub fn draw_additive(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);

    ui.horizontal(|ui| {
        let enabled = app.state.read().additive.enabled;
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
            app.state.write().additive.enabled = !enabled;
            app.push_audio_params();
        }

        // Voice volume + pan.
        let mut vol = app.state.read().additive.volume;
        if widgets::param_control(ui, "VOLUME", &mut vol, ParamMode::Free, ctrl).0 {
            app.state.write().additive.volume = vol.clamp(0.0, 1.5);
            app.push_audio_params();
        }
        let raw_pan = app.state.read().additive.pan;
        let mut pan = (raw_pan + 1.0) * 0.5;
        if widgets::param_control(ui, "PAN", &mut pan, ParamMode::Free, ctrl).0 {
            app.state.write().additive.pan = (pan * 2.0 - 1.0).clamp(-1.0, 1.0);
            app.push_audio_params();
        }

        // ADSR — single voice-wide envelope.  Same map as the FM /
        // SAMPLER+ ADSRs so users get consistent feel.
        let (mut a, mut d, mut sus, mut r) = {
            let s = app.state.read();
            (
                s.additive.attack,
                s.additive.decay,
                s.additive.sustain,
                s.additive.release,
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
            s.additive.attack = a.clamp(0.0, 1.0);
            s.additive.decay = d.clamp(0.0, 1.0);
            s.additive.sustain = sus.clamp(0.0, 1.0);
            s.additive.release = r.clamp(0.0, 1.0);
            drop(s);
            app.push_audio_params();
        }
    });

    ui.add_space(2.0);

    // Harmonic histogram — 16 vertical bars in a single glass
    // pane.  Drag inside the pane to set / sweep partial levels.
    let avail = ui.available_width();
    widgets::glass_group_fill(ui, avail, avail, |ui| {
        let mut levels = app.state.read().additive.levels;
        if draw_harmonic_histogram(ui, &mut levels, avail - 14.0, 90.0) {
            let mut s = app.state.write();
            s.additive.levels = levels;
            drop(s);
            app.push_audio_params();
        }
    });
}

/// Render an interactive 16-bar harmonic histogram at the given
/// dimensions.  Returns `true` if any level changed this frame.
/// Click-and-drag inside a column writes the new level
/// (1.0 at the top, 0.0 at the bottom).  Released drags carry the
/// cursor across multiple columns so the user can "draw" a curve
/// across the spectrum in a single sweep.
fn draw_harmonic_histogram(
    ui: &mut egui::Ui,
    levels: &mut [f32; ADDITIVE_HARMONICS],
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

    let n = ADDITIVE_HARMONICS as f32;
    let col_w = rect.width() / n;
    let bar_w = (col_w - 2.0).max(1.0);
    let mut changed = false;

    // Drag write path: when the cursor is inside the rect and the
    // primary button is down, set the column under the cursor's
    // level from the cursor's y position.  Sweeping across columns
    // updates each one in turn.
    if response.dragged_by(egui::PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
        && rect.contains(pos)
    {
        let col = ((pos.x - rect.min.x) / col_w).floor() as usize;
        if col < ADDITIVE_HARMONICS {
            let frac = ((rect.max.y - pos.y) / rect.height()).clamp(0.0, 1.0);
            if (levels[col] - frac).abs() > 1e-4 {
                levels[col] = frac;
                changed = true;
            }
        }
    }
    // Single-click path — `clicked` fires after a click that
    // didn't trigger a drag, so users who want a precise level
    // (rather than a sweep) get instant feedback.
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && rect.contains(pos)
    {
        let col = ((pos.x - rect.min.x) / col_w).floor() as usize;
        if col < ADDITIVE_HARMONICS {
            let frac = ((rect.max.y - pos.y) / rect.height()).clamp(0.0, 1.0);
            levels[col] = frac;
            changed = true;
        }
    }

    // Paint bars.  Fundamental (index 0) gets a slightly brighter
    // shade so the user knows where the played-note pitch sits.
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
        // Tiny harmonic-number tick label above bar 0, 4, 8, 12.
        // Keeps the strip readable without crowding every column.
        if i.is_multiple_of(4) {
            painter.text(
                egui::Pos2::new(x_lo + bar_w * 0.5, rect.min.y + 1.0),
                egui::Align2::CENTER_TOP,
                format!("{}", i + 1),
                egui::FontId::monospace(7.0),
                theme::ASH,
            );
        }
    }

    changed
}
