// ─── ui/widgets/llm_cycle.rs ──────────────────────────────────────────────────
// Round-robin "cycle" visualisation for the LLM console.  Cycles → circles:
// the round-robin progresses clockwise around the ring starting at 12 o'clock.
// Each enabled agent occupies one slot on the rim; the slot that is currently
// inferring pulses, and the slot that the round-robin will fire next is
// marked by a small cursor wedge.  Queued LlmInput::Infer messages for an
// agent appear as small dots tucked just inside the rim at that agent's slot;
// the global queue (agent_id = None) shows around the centre.

use std::f32::consts::TAU;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

use crate::state::AppState;
use crate::ui::{LlmQueueShadow, theme};

/// Render the round-robin cycle visualiser.  `next_agent_idx` is the
/// `jam_next_agent` counter (round-robin position).  `secs_to_next_fire`
/// is `Some(s)` when a fire is scheduled (jam_bars > 0) and `None`
/// when the loop is either firing immediately or dormant.
pub fn llm_cycle(
    ui: &mut Ui,
    state: &AppState,
    queue: &LlmQueueShadow,
    next_agent_idx: usize,
    secs_to_next_fire: Option<f32>,
    diameter: f32,
) {
    let size = Vec2::splat(diameter);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let r_outer = diameter * 0.42;
    let r_rim = r_outer * 0.93;
    let r_inner_dots = r_outer * 0.74;

    // Background dome (matches the screen-bezel look used elsewhere).
    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(diameter * 0.5));

    // Rim ring — the cycle path.
    painter.circle_stroke(center, r_rim, Stroke::new(1.0, Color32::from_gray(45)));

    // 12 o'clock tick — start of cycle.
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - r_outer),
            Pos2::new(center.x, center.y - r_outer * 0.78),
        ],
        Stroke::new(1.5, Color32::from_gray(120)),
    );

    // Collect enabled agents in rack order.
    let enabled: Vec<&crate::state::LlmAgentState> = state
        .llm_agents
        .iter()
        .filter(|a| state.rack.modules.iter().any(|m| m.id == a.id && m.enabled))
        .collect();

    if enabled.is_empty() {
        // No agents — just show "—" in the centre.
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            "—",
            egui::FontId::monospace(11.0),
            Color32::from_gray(70),
        );
        return;
    }

    let n = enabled.len() as f32;
    // Angle helper: i = 0 sits at 12 o'clock, advancing clockwise.
    // Egui's y axis points down, so "clockwise from up" is the angle
    // (-π/2 + 2π * i / n).
    let angle = |i: f32| -> f32 { -TAU * 0.25 + (i / n) * TAU };
    let pos = |i: f32, r: f32| -> Pos2 {
        let a = angle(i);
        Pos2::new(center.x + a.cos() * r, center.y + a.sin() * r)
    };

    let next_idx = if !enabled.is_empty() {
        next_agent_idx % enabled.len()
    } else {
        0
    };

    // Cursor wedge: small filled triangle just outside the rim at the
    // next-to-fire slot.  Points outward from centre.
    {
        let i = next_idx as f32;
        let a = angle(i);
        let tip = pos(i, r_outer * 0.99);
        let base_l = Pos2::new(
            center.x + (a + 0.18).cos() * r_outer * 0.75,
            center.y + (a + 0.18).sin() * r_outer * 0.75,
        );
        let base_r = Pos2::new(
            center.x + (a - 0.18).cos() * r_outer * 0.75,
            center.y + (a - 0.18).sin() * r_outer * 0.75,
        );
        painter.add(egui::Shape::convex_polygon(
            vec![tip, base_l, base_r],
            Color32::from_gray(170),
            Stroke::NONE,
        ));
    }

    // Per-agent slot rendering.
    let pulse_t = ui.ctx().input(|i| i.time) as f32;
    let mut any_active = false;
    for (i, agent) in enabled.iter().enumerate() {
        let i_f = i as f32;
        let p = pos(i_f, r_rim);

        // LED for the slot: pulse if inferring, dim if idle.
        let active = agent.is_inferring;
        if active {
            any_active = true;
        }
        let intensity = if active {
            (pulse_t * 4.0 * std::f32::consts::PI).sin() * 0.25 + 0.75
        } else {
            0.35
        };
        let led_color = if active {
            Color32::from_rgb(220, 220, 220)
        } else {
            Color32::from_gray(120)
        };
        theme::led(&painter, p, diameter * 0.045, led_color, intensity);

        // Persona name placed just outside the rim at this slot.
        let label_pos = pos(i_f, r_outer * 1.16);
        // Anchor text to the side of the circle to keep it readable.
        let a = angle(i_f);
        let align = if !(-TAU * 0.125..=TAU * 0.375).contains(&a) {
            egui::Align2::CENTER_BOTTOM
        } else if (-TAU * 0.125..TAU * 0.125).contains(&a) {
            egui::Align2::LEFT_CENTER
        } else {
            egui::Align2::CENTER_TOP
        };
        painter.text(
            label_pos,
            align,
            &agent.persona_name,
            egui::FontId::monospace(8.0),
            if active { theme::CHALK } else { theme::SMOKE },
        );

        // Pending dots tucked just inside the rim at this slot.  Rendered
        // along a short tangent so multiple queued items fan out neatly.
        let pending = queue.count_for(Some(agent.id));
        if pending > 0 {
            let tangent_a = angle(i_f) + TAU * 0.25; // 90° offset = tangent
            let dot_r = diameter * 0.022;
            for k in 0..pending.min(6) {
                let off = (k as f32 - (pending.min(6) as f32 - 1.0) * 0.5) * dot_r * 2.5;
                let dp = Pos2::new(
                    center.x + angle(i_f).cos() * r_inner_dots + tangent_a.cos() * off,
                    center.y + angle(i_f).sin() * r_inner_dots + tangent_a.sin() * off,
                );
                painter.circle_filled(dp, dot_r, Color32::from_gray(180));
            }
            if pending > 6 {
                painter.text(
                    pos(i_f, r_inner_dots * 0.78),
                    egui::Align2::CENTER_CENTER,
                    format!("+{}", pending - 6),
                    egui::FontId::monospace(7.0),
                    theme::FOG,
                );
            }
        }
    }

    // Centre label: countdown to next fire, "JAM" pulse, or "idle".
    let centre_text = match secs_to_next_fire {
        Some(s) if s > 0.0 => format!("{:.1}s", s),
        _ if any_active => "▶".to_string(),
        _ if queue.global > 0 || queue.per_agent.values().any(|c| *c > 0) => "···".to_string(),
        _ => "idle".to_string(),
    };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        &centre_text,
        egui::FontId::monospace(9.0),
        if any_active || secs_to_next_fire.is_some() {
            theme::FOG
        } else {
            theme::IRON
        },
    );

    // Global-bucket dots (agent_id = None sends) tucked under the centre.
    if queue.global > 0 {
        let dot_r = diameter * 0.022;
        let y = center.y + diameter * 0.18;
        for k in 0..queue.global.min(6) {
            let off = (k as f32 - (queue.global.min(6) as f32 - 1.0) * 0.5) * dot_r * 2.5;
            painter.circle_filled(Pos2::new(center.x + off, y), dot_r, Color32::from_gray(160));
        }
    }

    if any_active || secs_to_next_fire.is_some() {
        ui.ctx().request_repaint();
    }

    // Suppress the bezel-rect borrow warning if the variable isn't used.
    let _ = Rect::NOTHING;
}
