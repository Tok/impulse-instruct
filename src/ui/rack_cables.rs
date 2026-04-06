// ─── ui/rack_cables.rs ────────────────────────────────────────────────────────
// Patch cable rendering: bezier tube, signal flow animation, drag/drop structs.

use egui::{Color32, Pos2, Stroke, Vec2};

// ─── Drag state structs ───────────────────────────────────────────────────────

/// UI-local state for a cable currently being dragged.
#[derive(Clone, Debug)]
pub struct CableDrag {
    pub from_port: crate::state::PortRef,
    pub from_screen: Pos2,
}

/// State for a module title bar being dragged to reorder it in the rack.
#[derive(Clone, Debug)]
pub struct ModuleDrag {
    pub module_id: u32,
    pub pointer: Pos2,
}

// ─── Cable drawing ────────────────────────────────────────────────────────────

/// Evaluate a cubic bezier at parameter t ∈ [0,1].
pub fn bezier(from: Pos2, cp1: Pos2, cp2: Pos2, to: Pos2, t: f32) -> Pos2 {
    let mt = 1.0 - t;
    Pos2::new(
        mt * mt * mt * from.x
            + 3.0 * mt * mt * t * cp1.x
            + 3.0 * mt * t * t * cp2.x
            + t * t * t * to.x,
        mt * mt * mt * from.y
            + 3.0 * mt * mt * t * cp1.y
            + 3.0 * mt * t * t * cp2.y
            + t * t * t * to.y,
    )
}

/// Draw a patch cable as a 3D bezier tube.
/// `time`         — wall-clock seconds (for wobble + signal animation).
/// `phase_offset` — per-cable phase so cables don't all sway together.
/// `animate_flow` — when true, draw a traveling signal dot from→to.
pub fn draw_cable(
    painter: &egui::Painter,
    from: Pos2,
    to: Pos2,
    time: f32,
    phase_offset: f32,
    animate_flow: bool,
) {
    // ── Gravity sag ───────────────────────────────────────────────────────────
    let dx = (to.x - from.x).abs();
    let dy = to.y - from.y;
    let sag = (dx * 0.28 + (-dy).max(0.0) * 0.18).clamp(20.0, 160.0);

    // ── Gentle wobble — each cable sways at its own phase ────────────────────
    let wobble = ((time * 1.1 + phase_offset) * std::f32::consts::TAU).sin() * 2.5;
    let cp1 = from + Vec2::new(wobble, sag);
    let cp2 = to + Vec2::new(-wobble * 0.8, sag);

    // ── Sample curve points (48 segments for smooth 3D tube) ─────────────────
    let n = 48usize;
    let points: Vec<Pos2> = (0..=n)
        .map(|i| bezier(from, cp1, cp2, to, i as f32 / n as f32))
        .collect();

    // ── 3D tube rendering — 4 passes ─────────────────────────────────────────
    // 1. Wide dark drop shadow
    let shadow: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.5, 2.5)).collect();
    painter.add(egui::Shape::line(
        shadow,
        Stroke::new(5.5, Color32::from_black_alpha(90)),
    ));

    // 2. Cable body — bright gray
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(4.5, Color32::from_gray(155)),
    ));

    // 3. Core — slightly lighter, narrower (depth gradient)
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(2.5, Color32::from_gray(185)),
    ));

    // 4. Specular highlight — thin bright line along the top edge
    let hilight: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.0, -1.5)).collect();
    painter.add(egui::Shape::line(
        hilight,
        Stroke::new(1.0, Color32::from_white_alpha(110)),
    ));

    // ── Signal flow dots — speed and spacing normalised to cable arc length ───
    if animate_flow {
        let arc_len: f32 = points
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .sum::<f32>()
            .max(1.0);
        let speed = (170.0 / arc_len).clamp(0.25, 2.5);
        let num_dots = ((arc_len / 130.0).round() as u8).clamp(2, 5);
        let spacing = 1.0 / num_dots as f32;
        for i in 0..num_dots {
            let t_dot = (time * speed + phase_offset * 0.3 + i as f32 * spacing) % 1.0;
            let dot = bezier(from, cp1, cp2, to, t_dot);
            painter.circle_filled(dot, 5.0, Color32::from_white_alpha(35));
            painter.circle_filled(dot, 2.5, Color32::from_gray(240));
        }
    }
}
