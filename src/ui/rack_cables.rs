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
    /// Zone the dragged module belongs to.
    pub zone: crate::state::Zone,
    /// Grid column span of the module (from grid_size).
    pub col_span: u8,
    /// Grid row span of the module.
    pub row_span: u8,
    /// Zone rect origin (top-left of the zone's grid area).
    pub zone_origin: Pos2,
    /// Grid step (col_w + gap).
    pub step: f32,
    /// Grid column width.
    pub col_w: f32,
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

    // ── Signal flow dots — speed and spacing normalised to cable arc length ──
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

/// Draw a signal cable (LFO / CV / Mod) — sized between the fat audio
/// patch cable and the thin LLM control cable.  Same 3D tube treatment
/// as the audio cable but slimmer and a touch darker so audio routing
/// reads as visually dominant and modulation reads as a thinner
/// secondary path.
pub fn draw_signal_cable(
    painter: &egui::Painter,
    from: Pos2,
    to: Pos2,
    time: f32,
    phase_offset: f32,
    animate_flow: bool,
) {
    let dx = (to.x - from.x).abs();
    let dy = to.y - from.y;
    let sag = (dx * 0.25 + (-dy).max(0.0) * 0.16).clamp(16.0, 130.0);
    let wobble = ((time * 1.0 + phase_offset) * std::f32::consts::TAU).sin() * 2.0;
    let cp1 = from + Vec2::new(wobble, sag);
    let cp2 = to + Vec2::new(-wobble * 0.8, sag);
    let n = 40usize;
    let points: Vec<Pos2> = (0..=n)
        .map(|i| bezier(from, cp1, cp2, to, i as f32 / n as f32))
        .collect();
    // 1. Drop shadow
    let shadow: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.4, 2.0)).collect();
    painter.add(egui::Shape::line(
        shadow,
        Stroke::new(4.0, Color32::from_black_alpha(70)),
    ));
    // 2. Body — mid gray (between audio 155 and control 90)
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(3.5, Color32::from_gray(125)),
    ));
    // 3. Core — slightly lighter (between audio 185 and control 120)
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(1.8, Color32::from_gray(155)),
    ));
    // 4. Specular highlight (subtler than audio)
    let hilight: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.0, -1.2)).collect();
    painter.add(egui::Shape::line(
        hilight,
        Stroke::new(0.8, Color32::from_white_alpha(70)),
    ));
    if animate_flow {
        let arc_len: f32 = points
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .sum::<f32>()
            .max(1.0);
        let speed = (140.0 / arc_len).clamp(0.2, 2.0);
        let num_dots = ((arc_len / 150.0).round() as u8).clamp(2, 4);
        let spacing = 1.0 / num_dots as f32;
        for i in 0..num_dots {
            let t_dot = (time * speed + phase_offset * 0.3 + i as f32 * spacing) % 1.0;
            let dot = bezier(from, cp1, cp2, to, t_dot);
            painter.circle_filled(dot, 3.5, Color32::from_white_alpha(25));
            painter.circle_filled(dot, 1.8, Color32::from_gray(200));
        }
    }
}

/// Draw a thin, dark control cable (LLM agent → target module).
pub fn draw_control_cable(
    painter: &egui::Painter,
    from: Pos2,
    to: Pos2,
    time: f32,
    phase_offset: f32,
) {
    let dx = (to.x - from.x).abs();
    let dy = to.y - from.y;
    let sag = (dx * 0.20 + (-dy).max(0.0) * 0.12).clamp(12.0, 100.0);
    let wobble = ((time * 0.8 + phase_offset) * std::f32::consts::TAU).sin() * 1.5;
    let cp1 = from + Vec2::new(wobble, sag);
    let cp2 = to + Vec2::new(-wobble * 0.7, sag);
    let n = 32usize;
    let points: Vec<Pos2> = (0..=n)
        .map(|i| bezier(from, cp1, cp2, to, i as f32 / n as f32))
        .collect();
    // Subtle shadow
    let shadow: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.3, 1.5)).collect();
    painter.add(egui::Shape::line(
        shadow,
        Stroke::new(2.5, Color32::from_black_alpha(50)),
    ));
    // Body — dark gray
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(2.0, Color32::from_gray(90)),
    ));
    // Core — slightly lighter
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(1.0, Color32::from_gray(120)),
    ));
    // Slow signal dots
    let arc_len: f32 = points
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .sum::<f32>()
        .max(1.0);
    let speed = (80.0 / arc_len).clamp(0.1, 1.0);
    let num_dots = ((arc_len / 200.0).round() as u8).clamp(1, 3);
    let spacing = 1.0 / num_dots as f32;
    for i in 0..num_dots {
        let t_dot = (time * speed + phase_offset * 0.3 + i as f32 * spacing) % 1.0;
        let dot = bezier(from, cp1, cp2, to, t_dot);
        painter.circle_filled(dot, 2.5, Color32::from_gray(160));
    }
}

/// Draw the full cable overlay: in-progress drag, rack cables, and synthesised LFO cables.
pub fn draw_cable_overlay(
    app: &crate::ui::ImpulseApp,
    ctx: &egui::Context,
    ports: &[crate::ui::module_card::PortPos],
    canvas_rect: egui::Rect,
) {
    use crate::state::modulation::lfo_target_module_kind;
    use crate::state::{LfoTarget, ModuleKind, PortDir, PortKind, PortRef};

    let alt_held = ctx.input(|i| i.modifiers.alt);
    let show_cables = app.rack_flipped && !alt_held && !app.show_prefs;
    let has_drag = app.cable_drag.is_some();

    // Only create the foreground painter when we actually need to draw cables.
    // Creating it unconditionally can interfere with widget input (ComboBox popups).
    if !show_cables && !has_drag {
        return;
    }

    let time = ctx.input(|i| i.time) as f32;
    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("cables"),
    ));
    painter.set_clip_rect(canvas_rect);

    // In-progress drag
    if let Some(ref drag) = app.cable_drag
        && let Some(pointer) = ctx.pointer_latest_pos()
    {
        draw_cable(&painter, drag.from_screen, pointer, time, 0.0, false);
    }

    if show_cables {
        let cables = app.state.read().rack.cables.clone();
        // Layer order (back → front): audio (fattest) → signal (mid) →
        // control (thinnest).  Control sits on top because it's the
        // most semantically important path to read clearly when you're
        // chasing how an agent is steering the rack.
        for (ci, cable) in cables.iter().enumerate() {
            if cable.from.kind != PortKind::Audio {
                continue;
            }
            let from_pos = ports
                .iter()
                .find(|p| p.port == cable.from)
                .map(|p| p.center);
            let to_pos = ports.iter().find(|p| p.port == cable.to).map(|p| p.center);
            if let (Some(from), Some(to)) = (from_pos, to_pos) {
                draw_cable(&painter, from, to, time, ci as f32 * 2.399, true);
            }
        }
        for (ci, cable) in cables.iter().enumerate() {
            if !matches!(cable.from.kind, PortKind::Cv | PortKind::Mod) {
                continue;
            }
            let from_pos = ports
                .iter()
                .find(|p| p.port == cable.from)
                .map(|p| p.center);
            let to_pos = ports.iter().find(|p| p.port == cable.to).map(|p| p.center);
            if let (Some(from), Some(to)) = (from_pos, to_pos) {
                draw_signal_cable(&painter, from, to, time, ci as f32 * 2.399, true);
            }
        }
        for (ci, cable) in cables.iter().enumerate() {
            if cable.from.kind != PortKind::Control {
                continue;
            }
            let from_pos = ports
                .iter()
                .find(|p| p.port == cable.from)
                .map(|p| p.center);
            let to_pos = ports.iter().find(|p| p.port == cable.to).map(|p| p.center);
            if let (Some(from), Some(to)) = (from_pos, to_pos) {
                draw_control_cable(&painter, from, to, time, ci as f32 * 2.399);
            }
        }

        // Synthesised LFO cables
        let state = app.state.read();
        let lfo_ids: Vec<u32> = state
            .rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .map(|m| m.id)
            .collect();
        for (i, lfo_slot) in state.lfo.iter().enumerate() {
            if !lfo_slot.enabled || lfo_slot.target == LfoTarget::None {
                continue;
            }
            let Some(&lfo_id) = lfo_ids.get(i) else {
                continue;
            };
            let Some(tgt_kind) = lfo_target_module_kind(lfo_slot.target) else {
                continue;
            };
            let Some(tgt_id) = state
                .rack
                .modules
                .iter()
                .find(|m| m.kind == tgt_kind)
                .map(|m| m.id)
            else {
                continue;
            };
            if cables
                .iter()
                .any(|c| c.from.module_id == lfo_id && c.to.module_id == tgt_id)
            {
                continue;
            }
            let from_ref = PortRef {
                module_id: lfo_id,
                dir: PortDir::Out,
                kind: PortKind::Cv,
                index: 0,
            };
            let to_ref = PortRef {
                module_id: tgt_id,
                dir: PortDir::In,
                kind: PortKind::Cv,
                index: 0,
            };
            let from_pos = ports.iter().find(|p| p.port == from_ref).map(|p| p.center);
            let to_pos = ports.iter().find(|p| p.port == to_ref).map(|p| p.center);
            if let (Some(from), Some(to)) = (from_pos, to_pos) {
                let phase = (cables.len() + i) as f32 * 2.399;
                draw_signal_cable(&painter, from, to, time, phase, true);
            }
        }
        ctx.request_repaint();
    }
}
