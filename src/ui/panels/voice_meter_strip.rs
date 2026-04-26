// ─── ui/panels/voice_meter_strip.rs ─────────────────────────────────────────
// VoiceMeterStrip — per-voice mini-meters for the rack viz module.
// Reads atomic per-voice envelope levels published once per audio
// callback by the engine into `app.voice_meters`; renders a vertical
// bar + tiny label per voice currently in the rack.
//
// Pure UI: no DSP, no audio path.  The strip filters its slot list
// to voices actually present in the rack so users see meters only
// for the voices that exist (rather than 20 always-empty bars).

use crate::audio::voice_meters::{VOICE_METER_SLOTS, voice_meter_idx, voice_meter_label};
use crate::state::Zone;
use crate::ui::{ImpulseApp, theme};

pub fn draw_voice_meter_strip(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    // Collect the slot indices of voices currently in the rack.  A
    // voice that isn't placed gets no meter bar.  We keep the list
    // sorted by slot (which is also the canonical voice order in
    // `voice_meters::voice_meter_idx`) so the strip stays stable when
    // voices are added / removed.
    let mut active: Vec<usize> = Vec::with_capacity(VOICE_METER_SLOTS);
    {
        let st = app.state.read();
        for m in st.rack.modules.iter() {
            if m.zone == Zone::Voice
                && let Some(idx) = voice_meter_idx(m.kind)
                && !active.contains(&idx)
            {
                active.push(idx);
            }
        }
    }
    active.sort_unstable();

    if active.is_empty() {
        ui.label(
            egui::RichText::new("no voices in rack")
                .color(theme::SMOKE)
                .monospace()
                .size(9.0),
        );
        return;
    }

    let avail_w = ui.available_width();
    let avail_h = ui.available_height().max(40.0);
    // Each cell hosts the bar + a 9 px label below.  Reserve ~16 px
    // for the label row.
    let label_h = 12.0;
    let bar_h = (avail_h - label_h - 4.0).max(20.0);
    let cell_w = (avail_w / active.len() as f32).clamp(10.0, 60.0);

    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for &idx in &active {
                let level = app.voice_meters.read(idx).clamp(0.0, 1.5);
                draw_meter_bar(ui, level, cell_w, bar_h);
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for &idx in &active {
                let lbl = voice_meter_label(idx);
                let (rect, _) =
                    ui.allocate_exact_size(egui::Vec2::new(cell_w, label_h), egui::Sense::hover());
                if ui.is_rect_visible(rect) {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        lbl,
                        egui::FontId::monospace(8.0),
                        theme::FOG,
                    );
                }
            }
        });
    });
}

/// Draws a single vertical level bar.  `level` is the raw envelope
/// value (typically 0..1, can briefly exceed under heavy mix).  Maps
/// linearly for the bottom portion + log-perceptual taper at the top
/// so quiet signals are still visible on the bar.
fn draw_meter_bar(ui: &mut egui::Ui, level: f32, w: f32, h: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::new(w, h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    // Background well — same dark fill as the stereo meter.
    painter.rect_filled(
        rect,
        egui::Rounding::same(2.0),
        egui::Color32::from_gray(10),
    );
    // Level mapping: hybrid linear-then-log.  The bar tops out at
    // 1.0 visually but still keeps the bar visible above unity at
    // ≥80% height (compressed range so the meter doesn't disappear
    // on overshoot).
    let visual = if level <= 1.0 {
        level
    } else {
        1.0 + (level - 1.0).log10().clamp(-1.0, 0.5) * 0.1
    };
    let visual = visual.clamp(0.0, 1.0);
    let bar_top = rect.bottom() - visual * h;
    let bar_rect = egui::Rect::from_min_max(
        egui::Pos2::new(rect.left() + 1.0, bar_top.max(rect.top() + 1.0)),
        egui::Pos2::new(rect.right() - 1.0, rect.bottom() - 1.0),
    );
    // Brightness scales with level — bright when hot, dim when quiet.
    let g = (60.0 + visual * 180.0).min(240.0) as u8;
    painter.rect_filled(bar_rect, egui::Rounding::ZERO, egui::Color32::from_gray(g));
    // Top tick at the unity (1.0) line so users have a visual
    // reference for "at 0 dBFS-ish".
    let unity_y = rect.bottom() - h * 0.95;
    painter.line_segment(
        [
            egui::Pos2::new(rect.left(), unity_y),
            egui::Pos2::new(rect.right(), unity_y),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
    );
}
