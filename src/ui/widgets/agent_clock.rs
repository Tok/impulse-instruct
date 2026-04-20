// ─── ui/widgets/agent_clock.rs ───────────────────────────────────────────────
// Per-agent mini clock-face — Phase 2 of the LLM-console cycle widget.
// The big `llm_cycle` shows every agent on one ring; this is the per-agent
// focus view that lives on the agent card itself, giving each agent its own
// compact "am I working, or when am I next?" readout.
//
// Draws (matching the big cycle's grayscale language):
//   - a recessed screen bezel + guide ring,
//   - a progress arc from 12 o'clock clockwise filling `lanes_done /
//     total_lanes` of the circle while a pipeline is live for this agent,
//   - a pulsing dot at the arc's leading edge (independent motion so slow
//     lanes still feel alive),
//   - centre text: `▶` during inference (≤ 18 px cells drop the label),
//     `Ns` countdown when this agent is the next round-robin fire,
//     `tps` tok/s while inferring on wider cells, `#N` cycle count at rest,
//     or `·` when idle.

use std::f32::consts::TAU;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::state::AppState;
use crate::ui::theme;

/// Render the per-agent mini clock in a `size`×`size` box at the cursor.
/// `agent_id` selects which agent's pipeline / cycle count to read.
/// `secs_to_next_fire` is `Some(s)` only when the jam loop is scheduled
/// to fire this specific agent next; `None` otherwise.
pub fn agent_clock(
    ui: &mut Ui,
    state: &AppState,
    agent_id: u32,
    secs_to_next_fire: Option<f32>,
    size: f32,
) {
    let sz = Vec2::splat(size);
    let (rect, _) = ui.allocate_exact_size(sz, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let r_outer = size * 0.45;
    let r_inner = r_outer * 0.45;

    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));
    painter.circle_stroke(center, r_outer, Stroke::new(0.75, theme::SLATE));
    painter.circle_stroke(center, r_inner, Stroke::new(0.5, theme::IRON));

    // 12 o'clock tick — anchors "turn start" the same way the big cycle does.
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - r_outer * 1.08),
            Pos2::new(center.x, center.y - r_outer * 0.92),
        ],
        Stroke::new(1.0, Color32::from_gray(140)),
    );

    // Look up the agent + its progress.  Missing agent → blank clock.
    let agent = state.llm_agents.iter().find(|a| a.id == agent_id);
    let Some(agent) = agent else {
        return;
    };
    let progress = agent.pipeline_progress.as_ref();
    let inferring = agent.is_inferring;
    let tps = agent.tokens_per_sec;
    let cycles = agent.jam_cycle_count;

    let pulse_t = ui.ctx().input(|i| i.time) as f32;

    // Progress arc: same `lanes_done / total_lanes` fraction as the big
    // cycle viz, but drawn on this agent's own ring instead of sharing
    // one circle with every other agent.  Pulse-tween so the arc grows
    // smoothly into the lane currently running.
    if let Some(p) = progress {
        let total = p.total_lanes.max(1) as f32;
        let done = p.lanes_done as f32;
        let in_flight = if p.current_lane.is_some() { 1.0 } else { 0.0 };
        let pulse = (pulse_t * 1.3).sin() * 0.5 + 0.5;
        let smooth = (done + in_flight * pulse * 0.85).min(total);
        let frac = (smooth / total).clamp(0.0, 1.0);
        let start_a = -TAU * 0.25; // 12 o'clock
        let span_a = TAU * frac;
        let arc_steps = ((span_a.abs() / (TAU / 64.0)) as usize).max(2);
        let mut pts: Vec<Pos2> = Vec::with_capacity(arc_steps + 1);
        for k in 0..=arc_steps {
            let t = k as f32 / arc_steps as f32;
            let a = start_a + span_a * t;
            pts.push(Pos2::new(
                center.x + a.cos() * r_outer,
                center.y + a.sin() * r_outer,
            ));
        }
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(1.4, Color32::from_gray(170)));
        }
        if let Some(head) = pts.last() {
            theme::led_flat(
                &painter,
                *head,
                (r_outer * 0.14).clamp(2.0, 4.0),
                theme::CHALK,
                1.0,
            );
        }
        ui.ctx().request_repaint();
    }

    // Next-fire wedge: when this agent is scheduled next, draw a small
    // outward triangle at 12 o'clock so the countdown reads clearly as
    // "about to fire THIS clock", not just "some countdown".
    if secs_to_next_fire.is_some() {
        let tip = Pos2::new(center.x, center.y - r_outer * 1.18);
        let base_l = Pos2::new(center.x - r_outer * 0.14, center.y - r_outer * 0.94);
        let base_r = Pos2::new(center.x + r_outer * 0.14, center.y - r_outer * 0.94);
        painter.add(egui::Shape::convex_polygon(
            vec![tip, base_l, base_r],
            Color32::from_gray(175),
            Stroke::NONE,
        ));
    }

    // Active-agent ping rings (independent of pipeline arc so motion is
    // visible even between lane completions on slow models).
    if inferring {
        for offset in [0.0_f32, 0.5] {
            let phase = ((pulse_t * 1.2) + offset).fract();
            let r = r_inner * (1.0 + phase * 2.8);
            let a = ((1.0 - phase) * 85.0) as u8;
            painter.circle_stroke(
                center,
                r,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(220, 220, 220, a)),
            );
        }
        ui.ctx().request_repaint();
    }

    // Centre readout: most informative thing first, with a narrow-cell
    // fallback for tiny sizes.
    let text_size = (size * 0.28).clamp(6.5, 11.0);
    let (text, color) = match secs_to_next_fire {
        Some(s) if s > 0.0 => (format!("{:.1}s", s), theme::FOG),
        _ if inferring && size >= 28.0 => (format!("{:.0}t/s", tps), theme::FOG),
        _ if inferring => ("▶".to_string(), theme::CHALK),
        _ if cycles > 0 => (format!("#{}", cycles), theme::IRON),
        _ => ("·".to_string(), theme::IRON),
    };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        &text,
        egui::FontId::monospace(text_size),
        color,
    );

    // Suppress an unused-import warning if future edits drop Rect usage.
    let _ = Rect::NOTHING;
}
