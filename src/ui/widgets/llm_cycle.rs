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
    // Match the ring oscilloscope's geometry so the two share visual
    // language: outer = 45% of the chip, inner guide = 40% of outer
    // (i.e. 18% of the chip), background is the same recessed-screen
    // bezel with a 2 px rounded square (not a perfect circle — the
    // chip housing is square, the lit content is round).
    let r_outer = diameter * 0.45;
    let r_inner = r_outer * 0.40;
    let r_rim = r_outer;
    let r_dots = (r_outer + r_inner) * 0.5; // mid-band for queued dots

    theme::draw_screen_bezel(&painter, rect, egui::Rounding::same(2.0));
    // Subtle guide rings — same colours as `draw_ring_scope_colored`.
    painter.circle_stroke(center, r_outer, Stroke::new(1.0, theme::SLATE));
    painter.circle_stroke(center, r_inner, Stroke::new(0.5, theme::IRON));

    // 12 o'clock tick — start of cycle, drawn just outside the rim.
    painter.line_segment(
        [
            Pos2::new(center.x, center.y - r_outer * 1.06),
            Pos2::new(center.x, center.y - r_outer * 0.94),
        ],
        Stroke::new(1.5, Color32::from_gray(140)),
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

    // Sizes scale with the chip diameter so the widget looks right at
    // any panel height (the cycle viz now fills the full LLM-console
    // height, which can range ~120–280 px).
    let led_r = (r_outer * 0.07).clamp(2.5, 7.0);
    let dot_r = (r_outer * 0.04).clamp(1.5, 4.0);
    let label_size = (r_outer * 0.18).clamp(7.0, 11.0);
    let centre_size = (r_outer * 0.20).clamp(8.0, 13.0);

    // Cursor wedge: small filled triangle just outside the rim at the
    // next-to-fire slot.  Points outward from centre.
    {
        let i = next_idx as f32;
        let a = angle(i);
        let tip = pos(i, r_outer * 1.04);
        let base_l = Pos2::new(
            center.x + (a + 0.18).cos() * r_outer * 0.85,
            center.y + (a + 0.18).sin() * r_outer * 0.85,
        );
        let base_r = Pos2::new(
            center.x + (a - 0.18).cos() * r_outer * 0.85,
            center.y + (a - 0.18).sin() * r_outer * 0.85,
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
            Color32::from_rgb(230, 230, 230)
        } else {
            Color32::from_gray(120)
        };
        theme::led(&painter, p, led_r, led_color, intensity);

        // Persona name placed just outside the rim at this slot.
        let label_pos = pos(i_f, r_outer * 1.14);
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
            egui::FontId::monospace(label_size),
            if active { theme::CHALK } else { theme::SMOKE },
        );

        // Pending dots tucked just inside the rim at this slot.  Rendered
        // along a short tangent so multiple queued items fan out neatly.
        let pending = queue.count_for(Some(agent.id));
        if pending > 0 {
            let tangent_a = angle(i_f) + TAU * 0.25;
            for k in 0..pending.min(6) {
                let off = (k as f32 - (pending.min(6) as f32 - 1.0) * 0.5) * dot_r * 2.5;
                let dp = Pos2::new(
                    center.x + angle(i_f).cos() * r_dots + tangent_a.cos() * off,
                    center.y + angle(i_f).sin() * r_dots + tangent_a.sin() * off,
                );
                painter.circle_filled(dp, dot_r, Color32::from_gray(180));
            }
            if pending > 6 {
                painter.text(
                    pos(i_f, r_dots * 0.85),
                    egui::Align2::CENTER_CENTER,
                    format!("+{}", pending - 6),
                    egui::FontId::monospace((label_size - 1.0).max(6.0)),
                    theme::FOG,
                );
            }
        }
    }

    // Centre label: countdown to next fire, "▶" while inferring, or idle.
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
        egui::FontId::monospace(centre_size),
        if any_active || secs_to_next_fire.is_some() {
            theme::FOG
        } else {
            theme::IRON
        },
    );

    // Global-bucket dots (agent_id = None sends) just below the centre.
    if queue.global > 0 {
        let y = center.y + r_inner * 0.6;
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
