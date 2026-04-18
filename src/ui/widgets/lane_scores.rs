// ─── ui/widgets/lane_scores.rs ────────────────────────────────────────────────
// Compact score readout for the Phase 1 lane-eval scores.  Layout-stable: the
// set of cells is fixed to the live-rack lanes (bass voices enabled, drum
// kits present, etc.), so new scores arriving each pipeline tick overwrite
// values in place rather than reflowing the widget.  Bottom-of-cell mini-bar
// shows the 0..1 score; cells are dimmed until scored; the current in-flight
// lane pulses so the user can follow the pipeline live.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::llm::lanes::LaneKind;
use crate::state::{AppState, ModuleKind};
use crate::ui::theme;

/// The ordered list of lanes this widget shows.  Mirrors `default_plan`'s
/// fan-out but also includes `Modulation` for completeness.  Rack lane is
/// intentionally absent — scheduler never picks it anyway.
fn visible_lanes(state: &AppState) -> Vec<LaneKind> {
    let rack_has = |kind: ModuleKind| -> bool {
        state
            .rack
            .modules
            .iter()
            .any(|m| m.kind == kind && m.enabled && state.rack.reaches_master(m.id))
    };
    let mut lanes = vec![LaneKind::Settings];
    if rack_has(ModuleKind::DrumKit808) {
        lanes.push(LaneKind::KitA);
    }
    if rack_has(ModuleKind::DrumKit909) {
        lanes.push(LaneKind::KitB);
    }
    if rack_has(ModuleKind::AmenSampler) {
        lanes.push(LaneKind::Amen);
    }
    if rack_has(ModuleKind::AcidBass) {
        for (idx, voice) in state.bass_voices.iter().enumerate() {
            if voice.enabled {
                lanes.push(LaneKind::Bass(idx));
            }
        }
    }
    if rack_has(ModuleKind::HooverLead) {
        lanes.push(LaneKind::Hoover);
    }
    if rack_has(ModuleKind::An1xVoice) {
        lanes.push(LaneKind::An1x);
    }
    lanes.push(LaneKind::Fx);
    lanes
}

/// Render the lane-score strip.  Fills the given rect with a fixed number
/// of equal-width cells, one per live lane.  Returns silently if the rect
/// is off-screen or has zero lanes to show.
pub fn lane_scores(ui: &mut Ui, state: &AppState, rect: Rect) {
    let painter = ui.painter_at(rect);
    // Background: match the ring oscilloscope's recessed screen bezel
    // so the score strip reads as part of the same "screen" surface.
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));

    let lanes = visible_lanes(state);
    if lanes.is_empty() {
        return;
    }

    let pulse_t = ui.ctx().input(|i| i.time) as f32;
    let in_flight = state
        .llm
        .pipeline_progress
        .as_ref()
        .and_then(|p| p.current_lane.clone());

    let n = lanes.len() as f32;
    let cell_w = rect.width() / n;
    let pad_x = 2.0;
    let pad_y = 2.0;
    let inner_h = rect.height() - 2.0 * pad_y;
    let bar_h = (inner_h * 0.25).clamp(2.0, 4.0);
    let label_size = (inner_h * 0.40).clamp(6.0, 9.0);
    let value_size = (inner_h * 0.34).clamp(5.5, 8.0);

    // Cache the whole score map — lane_scores is small (≤ 12 entries).
    let scores = &state.llm.lane_scores;

    let mut hovered: Option<(Rect, LaneKind, Option<f32>, u32, u32)> = None;

    for (i, lane) in lanes.iter().enumerate() {
        let cell = Rect::from_min_size(
            Pos2::new(rect.min.x + i as f32 * cell_w + pad_x, rect.min.y + pad_y),
            Vec2::new((cell_w - 2.0 * pad_x).max(1.0), inner_h),
        );
        if cell.width() < 8.0 {
            continue;
        }
        // Separator tick between cells (skip the first so the left edge
        // doesn't double up with the bezel).
        if i > 0 {
            let x = rect.min.x + i as f32 * cell_w;
            painter.line_segment(
                [
                    Pos2::new(x, rect.min.y + 3.0),
                    Pos2::new(x, rect.max.y - 3.0),
                ],
                Stroke::new(0.5, theme::IRON),
            );
        }

        let label = lane.label();
        let entry = scores.get(label);
        let score = entry.map(|e| e.score);
        let cycle_changed = entry.map(|e| e.last_changed_cycle).unwrap_or(0);
        let change_count = entry.map(|e| e.change_count).unwrap_or(0);
        let is_current = in_flight.as_deref().map(|l| l == label).unwrap_or(false);

        // Label row.
        let label_color = if is_current {
            theme::CHALK
        } else if entry.is_some() {
            theme::FOG
        } else {
            theme::IRON
        };
        painter.text(
            Pos2::new(cell.center().x, cell.min.y + label_size * 0.5 + 1.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::monospace(label_size),
            label_color,
        );

        // Score value (plain digit, two decimals when present, "—" when
        // unscored).  Kept right above the bar.
        let value_text = match score {
            Some(v) => format!("{:.2}", v),
            None => "—".to_string(),
        };
        let value_color = match score {
            Some(v) => {
                // Map 0..1 to gray..chalk so higher scores read as
                // brighter — keeps the strip grayscale like the rest.
                let lum = (80.0 + 120.0 * v).clamp(80.0, 200.0) as u8;
                Color32::from_gray(lum)
            }
            None => theme::SLATE,
        };
        painter.text(
            Pos2::new(
                cell.center().x,
                cell.min.y + label_size + value_size * 0.5 + 3.0,
            ),
            egui::Align2::CENTER_CENTER,
            &value_text,
            egui::FontId::monospace(value_size),
            value_color,
        );

        // Score bar — draws along the bottom edge of the cell, width
        // proportional to the score (0..1).  Empty outline when unscored.
        let bar_left = cell.min.x + 2.0;
        let bar_right = cell.max.x - 2.0;
        let bar_y = cell.max.y - bar_h - 1.0;
        let bar_full = Rect::from_min_max(
            Pos2::new(bar_left, bar_y),
            Pos2::new(bar_right, bar_y + bar_h),
        );
        painter.rect_stroke(
            bar_full,
            egui::Rounding::same(0.5),
            Stroke::new(0.75, theme::IRON),
        );
        if let Some(v) = score {
            let w = (bar_right - bar_left) * v.clamp(0.0, 1.0);
            let bar_fill = Rect::from_min_max(
                Pos2::new(bar_left, bar_y),
                Pos2::new(bar_left + w, bar_y + bar_h),
            );
            let fill_lum = (110.0 + 130.0 * v).clamp(110.0, 240.0) as u8;
            painter.rect_filled(
                bar_fill,
                egui::Rounding::same(0.5),
                Color32::from_gray(fill_lum),
            );
        }

        // Pulse ring around the currently-inferring lane so the strip
        // mirrors the cycle viz's "this one is working" cue.
        if is_current {
            let ring = cell.shrink(1.0);
            let alpha = ((pulse_t * 3.0).sin() * 0.5 + 0.5) * 110.0;
            painter.rect_stroke(
                ring,
                egui::Rounding::same(2.0),
                Stroke::new(
                    1.0,
                    Color32::from_rgba_unmultiplied(220, 220, 220, alpha as u8),
                ),
            );
            ui.ctx().request_repaint();
        }

        // Collect hover-tooltip info — resolved after the loop so we
        // hit-test against the whole strip with a single interaction.
        let cell_response = ui.interact(
            cell,
            ui.id().with(("lane_score_cell", label)),
            Sense::hover(),
        );
        if cell_response.hovered() {
            hovered = Some((cell, *lane, score, cycle_changed, change_count));
        }
    }

    // Tooltip: full label, score, and Phase 1 bookkeeping.  One tooltip
    // total (the hit-test loop above keeps only the last hovered cell).
    if let Some((_, lane, score, cycle_changed, change_count)) = hovered {
        let current_cycle = state.llm.jam_cycle_count;
        let score_line = match score {
            Some(v) => format!("score {:.2}", v),
            None => "score —".to_string(),
        };
        let gap = current_cycle.saturating_sub(cycle_changed);
        let gap_line = if change_count == 0 {
            "never picked".to_string()
        } else {
            format!("#{} · {} cycles ago", change_count, gap)
        };
        egui::show_tooltip_at_pointer(
            ui.ctx(),
            ui.layer_id(),
            egui::Id::new(("lane_score_tooltip", lane.label())),
            |ui| {
                ui.label(
                    egui::RichText::new(lane.label())
                        .monospace()
                        .size(10.5)
                        .color(theme::CHALK),
                );
                ui.label(
                    egui::RichText::new(score_line)
                        .monospace()
                        .size(9.5)
                        .color(theme::FOG),
                );
                ui.label(
                    egui::RichText::new(gap_line)
                        .monospace()
                        .size(9.0)
                        .color(theme::SMOKE),
                );
            },
        );
    }
}
