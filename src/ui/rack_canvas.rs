// ─── ui/rack_canvas.rs ────────────────────────────────────────────────────────
// The modular rack canvas.  Replaces the old 5-tab layout.
//
// Layout (top → bottom, all full window width, vertical scroll):
//   ┌──────────────────────────────────────────────────────────┐
//   │ SEQUENCER  (full width)                                  │
//   ├──────────────────────────────────────────────────────────┤  global rail
//   │ MASTER OUT  (compact, right-aligned)                     │
//   ├──────────────────────────────────────────────────────────┤  voice rail
//   │ [BASS SYNTH] [DRUM KIT A] [DRUM KIT B] [HOOVER] [AN1X]  │  ← wraps
//   ├──────────────────────────────────────────────────────────┤  fx/mod rail
//   │ [REVERB] [DELAY] [CHORUS] … [LFO] [LFO]                 │  ← wraps
//   └──────────────────────────────────────────────────────────┘
//
// Cables are drawn as a Painter overlay after all cards are placed, using
// screen positions collected during card rendering.

use egui::{Color32, Pos2, ScrollArea, Stroke, Vec2};

use crate::state::rack::lfo_target_module_kind;
use crate::state::{LfoTarget, ModuleKind, PortDir, PortKind, PortRef, Zone};
use crate::ui::module_card::PortPos;
use crate::ui::{ImpulseApp, module_card};

// ─── Cable drawing ───────────────────────────────────────────────────────────

/// Draw a single patch cable as a 3D bezier tube (two bezier passes).
/// Evaluate a cubic bezier at parameter t ∈ [0,1].
fn bezier(from: Pos2, cp1: Pos2, cp2: Pos2, to: Pos2, t: f32) -> Pos2 {
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

/// Draw a patch cable.
/// `time`         — wall-clock seconds (for wobble + signal animation).
/// `phase_offset` — per-cable phase so cables don't all sway together.
/// `animate_flow` — when true, draw a traveling signal dot from→to.
fn draw_cable(
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
    // Horizontal oscillation of the control points; keeps amplitude small so
    // cables look alive but not distracting.
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
    let cable_col = Color32::from_gray(155);
    painter.add(egui::Shape::line(
        points.clone(),
        Stroke::new(4.5, cable_col),
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

    // ── Signal flow dots — speed and spacing normalised to cable arc length ──────
    // Arc length from the already-sampled points (free, no extra Bezier evals).
    if animate_flow {
        let arc_len: f32 = points
            .windows(2)
            .map(|w| w[0].distance(w[1]))
            .sum::<f32>()
            .max(1.0);

        // Target: ~170 px/s travel speed regardless of cable length.
        // Clamp so very short or very long cables stay in a sensible range.
        let speed = (170.0 / arc_len).clamp(0.25, 2.5);

        // Dot spacing: one dot per ~130 px of cable; min 2, max 5.
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

// ─── In-progress cable drag state ────────────────────────────────────────────

/// UI-local state for a cable currently being dragged.
#[derive(Clone, Debug)]
pub struct CableDrag {
    pub from_port: crate::state::PortRef,
    pub from_screen: Pos2,
}

// ─── Module drag state ────────────────────────────────────────────────────────

/// State for a module title bar being dragged to reorder it in the rack.
#[derive(Clone, Debug)]
pub struct ModuleDrag {
    pub module_id: u32,
    pub pointer: Pos2,
}

// ─── Available module kinds per zone (for add menus) ─────────────────────────

const VOICE_KINDS: &[ModuleKind] = &[
    ModuleKind::AcidBass,
    ModuleKind::DrumKit808,
    ModuleKind::DrumKit909,
    ModuleKind::HooverLead,
    ModuleKind::An1xVoice,
    ModuleKind::AmenSampler,
    ModuleKind::NoiseVoice,
];

const FXMOD_KINDS: &[ModuleKind] = &[
    ModuleKind::FxReverb,
    ModuleKind::FxDelay,
    ModuleKind::FxChorus,
    ModuleKind::FxPhaser,
    ModuleKind::FxEq,
    ModuleKind::FxCompressor,
    ModuleKind::FxTapeSat,
    ModuleKind::FxDrive,
    ModuleKind::FxWaveshaper,
    ModuleKind::FxBitcrush,
    ModuleKind::FxRingMod,
    ModuleKind::LfoModule,
];

// ─── Main rack canvas ─────────────────────────────────────────────────────────

pub fn draw_rack(app: &mut ImpulseApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    // Collect port positions during this frame's render pass.
    let mut ports: Vec<PortPos> = Vec::new();

    // Capture the full canvas rect BEFORE allocating any content, so the
    // context menu can cover the entire rack area with a single interact region.
    let canvas_rect = ui.available_rect_before_wrap();

    // Disable drag-to-scroll only while a cable is being dragged, so normal
    // two-finger/trackpad scroll works when not patching.
    let dragging_cable = app.cable_drag.is_some();
    ScrollArea::vertical()
        .id_source("rack_scroll")
        .drag_to_scroll(!dragging_cable)
        .auto_shrink([false; 2])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            draw_rack_inner(app, ui, &mut ports);
        });

    // ── Cable overlay (Tab to show/hide) ──────────────────────────────────────
    {
        let time = ctx.input(|i| i.time) as f32;
        let mut painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("cables"),
        ));
        // Clip cables to the rack canvas only — keeps header, footer, and piano on top.
        painter.set_clip_rect(canvas_rect);

        // In-progress drag: no flow dot, neutral phase.
        if let Some(ref drag) = app.cable_drag
            && let Some(pointer) = ctx.pointer_latest_pos()
        {
            draw_cable(&painter, drag.from_screen, pointer, time, 0.0, false);
        }

        if app.show_cables {
            let cables = app.state.read().rack.cables.clone();
            for (ci, cable) in cables.iter().enumerate() {
                let from_pos = ports
                    .iter()
                    .find(|p| p.port == cable.from)
                    .map(|p| p.center);
                let to_pos = ports.iter().find(|p| p.port == cable.to).map(|p| p.center);
                if let (Some(from), Some(to)) = (from_pos, to_pos) {
                    // Spread phase offsets by golden ratio so cables wobble independently.
                    let phase = ci as f32 * 2.399;
                    draw_cable(&painter, from, to, time, phase, true);
                }
            }

            // Synthesise visual cables for active LFO slots.
            // The LFO→param connection is a state param (lfo.target), not a rack
            // cable, so we derive the cable position from current state here.
            {
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
                    // Skip if a real rack cable already covers this pair.
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
                        draw_cable(&painter, from, to, time, phase, true);
                    }
                }
            }

            // Animate continuously while cables are visible.
            ctx.request_repaint();
        }
    }

    // ── Cable drag — always active, no mode required ──────────────────────────
    handle_cable_drag(app, ctx, &ports);

    // ── Module drag ghost overlay ─────────────────────────────────────────────
    if let Some(ref drag) = app.module_drag
        && let Some(pointer) = ctx.pointer_latest_pos()
    {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("module_drag_ghost"),
        ));
        let kind = app
            .state
            .read()
            .rack
            .modules
            .iter()
            .find(|m| m.id == drag.module_id)
            .map(|m| m.kind);
        if let Some(k) = kind {
            let ghost_rect = egui::Rect::from_center_size(pointer, egui::Vec2::new(120.0, 22.0));
            painter.rect_filled(
                ghost_rect,
                egui::Rounding::same(4.0),
                Color32::from_rgba_premultiplied(30, 30, 30, 180),
            );
            painter.rect_stroke(
                ghost_rect,
                egui::Rounding::same(4.0),
                egui::Stroke::new(1.0, Color32::from_gray(80)),
            );
            painter.text(
                ghost_rect.center(),
                egui::Align2::CENTER_CENTER,
                k.label(),
                egui::FontId::monospace(9.5),
                Color32::from_gray(200),
            );
            ctx.request_repaint();
        }
    }

    // ── Add module popup ──────────────────────────────────────────────────────
    draw_add_menu(app, ctx);

    // ── Right-click context menu ──────────────────────────────────────────────
    ui.interact(canvas_rect, ui.id().with("ctx_menu"), egui::Sense::hover())
        .context_menu(|ui| {
            ui.label(
                egui::RichText::new("ADD MODULE")
                    .monospace()
                    .size(8.5)
                    .color(Color32::from_gray(100)),
            );
            ui.separator();
            ui.menu_button("Voice", |ui| {
                for kind in VOICE_KINDS {
                    if !kind.allows_multiple() {
                        let exists = app
                            .state
                            .read()
                            .rack
                            .modules
                            .iter()
                            .any(|m| m.kind == *kind);
                        if exists {
                            ui.add_enabled(
                                false,
                                egui::Button::new(
                                    egui::RichText::new(kind.label()).monospace().size(9.5),
                                ),
                            );
                            continue;
                        }
                    }
                    if ui
                        .button(egui::RichText::new(kind.label()).monospace().size(9.5))
                        .clicked()
                    {
                        app.state.write().rack.add_module(*kind);
                        ui.close_menu();
                    }
                }
            });
            ui.menu_button("FX + Mod", |ui| {
                for kind in FXMOD_KINDS {
                    if ui
                        .button(egui::RichText::new(kind.label()).monospace().size(9.5))
                        .clicked()
                    {
                        app.state.write().rack.add_module(*kind);
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            let cable_label = if app.show_cables {
                "Hide cables  [Tab]"
            } else {
                "Show cables  [Tab]"
            };
            if ui
                .button(egui::RichText::new(cable_label).monospace().size(9.5))
                .clicked()
            {
                app.show_cables = !app.show_cables;
                ui.close_menu();
            }
        });
}

fn draw_add_menu(app: &mut ImpulseApp, ctx: &egui::Context) {
    let zone = match app.add_menu_zone {
        Some(z) => z,
        None => return,
    };
    let kinds: &[ModuleKind] = match zone {
        crate::state::Zone::Voice => VOICE_KINDS,
        crate::state::Zone::FxMod => FXMOD_KINDS,
        crate::state::Zone::Global => return,
    };

    let mut open = true;
    egui::Window::new("add_module_popup")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(160.0);
            ui.label(
                egui::RichText::new("ADD MODULE")
                    .monospace()
                    .size(8.5)
                    .color(Color32::from_gray(100)),
            );
            ui.separator();
            let mut close = false;
            for kind in kinds {
                let already_exists = !kind.allows_multiple()
                    && app
                        .state
                        .read()
                        .rack
                        .modules
                        .iter()
                        .any(|m| m.kind == *kind);
                if already_exists {
                    ui.add_enabled(
                        false,
                        egui::Button::new(egui::RichText::new(kind.label()).monospace().size(9.5)),
                    );
                } else if ui
                    .button(egui::RichText::new(kind.label()).monospace().size(9.5))
                    .clicked()
                {
                    app.state.write().rack.add_module(*kind);
                    close = true;
                }
            }
            if close
                || ui
                    .button(egui::RichText::new("cancel").monospace().size(8.5))
                    .clicked()
            {
                app.add_menu_zone = None;
            }
        });
    if !open {
        app.add_menu_zone = None;
    }
}

// Golden-ratio width tiers.  Each module picks one; the layout never recomputes
// Slot widths use a responsive grid:
//
//   Global (Sequencer, Master): always full width.
//
//   Voice modules: minimum 420 px; how many fit per row depends on available_w:
//     < 840 px  → 1 per row (fills full width)
//     840-1259  → 2 per row (each ≈ half)
//     ≥ 1260 px → 3 per row (each ≈ third)
//   AN1X is "wide" — always takes 2 columns worth (or full width when 1-per-row).
//
//   FX / LFO: fixed ~220 px, 4-5 per row.

const VOICE_MIN_W: f32 = 420.0;
const FX_SLOT_W: f32 = 220.0;

fn module_slot_w(kind: ModuleKind, full_w: f32) -> f32 {
    match kind {
        ModuleKind::StepSequencer | ModuleKind::MasterOutput => full_w,
        // FX and LFO: fixed compact width
        ModuleKind::FxReverb
        | ModuleKind::FxDelay
        | ModuleKind::FxChorus
        | ModuleKind::FxPhaser
        | ModuleKind::FxRingMod
        | ModuleKind::FxWaveshaper
        | ModuleKind::FxBitcrush
        | ModuleKind::FxEq
        | ModuleKind::FxCompressor
        | ModuleKind::FxTapeSat
        | ModuleKind::FxDrive
        | ModuleKind::LfoModule => FX_SLOT_W.min(full_w),
        // AN1X: wide — 2 voice columns or full width
        ModuleKind::An1xVoice => {
            let cols = voice_cols(full_w);
            if cols >= 2 {
                (full_w / cols as f32) * 2.0
            } else {
                full_w
            }
        }
        // All other voice modules: fill evenly by column count
        _ => full_w / voice_cols(full_w) as f32,
    }
}

/// How many voice columns fit in `full_w` while respecting VOICE_MIN_W.
fn voice_cols(full_w: f32) -> usize {
    if full_w >= VOICE_MIN_W * 3.0 {
        3
    } else if full_w >= VOICE_MIN_W * 2.0 {
        2
    } else {
        1
    }
}

/// Group items into rows so each row fits within `available_w`.
/// Returns rows; each row is a slice of items that fits without overflow.
fn group_into_rows(
    items: &[(u32, ModuleKind, bool)],
    available_w: f32,
    gap: f32,
) -> Vec<Vec<(u32, ModuleKind, bool)>> {
    let mut rows: Vec<Vec<(u32, ModuleKind, bool)>> = vec![vec![]];
    let mut row_w = 0.0f32;
    for &item in items {
        let w = module_slot_w(item.1, available_w);
        if row_w > 0.0 && row_w + gap + w > available_w + 0.5 {
            rows.push(vec![]);
            row_w = 0.0;
        }
        if row_w > 0.0 {
            row_w += gap;
        }
        row_w += w;
        rows.last_mut().unwrap().push(item);
    }
    rows
}

fn draw_rack_inner(app: &mut ImpulseApp, ui: &mut egui::Ui, ports: &mut Vec<PortPos>) {
    // Subtract a small gutter so modules never touch the scrollbar.
    let available_w = (ui.available_width() - 8.0).max(200.0);

    // ── GLOBAL ZONE — sequencer + master, full width ──────────────────────────
    module_card::zone_rail(ui, "GLOBAL", false);

    // Sequencer — full available width
    {
        let seq_id = app
            .state
            .read()
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::StepSequencer)
            .map(|m| m.id)
            .unwrap_or(100);
        let enabled = app
            .state
            .read()
            .rack
            .modules
            .iter()
            .find(|m| m.id == seq_id)
            .map(|m| m.enabled)
            .unwrap_or(true);

        let (resp, _) = module_card::module_card(
            ui,
            seq_id,
            ModuleKind::StepSequencer,
            enabled,
            Some(available_w - 2.0),
            ports,
            |ui| {
                crate::ui::panels::draw_sequencer(app, ui);
            },
        );
        if resp.toggle_clicked
            && let Some(m) = app
                .state
                .write()
                .rack
                .modules
                .iter_mut()
                .find(|m| m.id == seq_id)
        {
            m.enabled = !m.enabled;
        }
    }

    ui.add_space(2.0);

    // Master output card — compact strip showing master volume + per-voice info
    {
        let master_id = app
            .state
            .read()
            .rack
            .modules
            .iter()
            .find(|m| m.kind == ModuleKind::MasterOutput)
            .map(|m| m.id)
            .unwrap_or(101);
        let (resp, _) = module_card::module_card(
            ui,
            master_id,
            ModuleKind::MasterOutput,
            true,
            Some(available_w - 2.0),
            ports,
            |ui| {
                draw_master_content(app, ui);
            },
        );
        let _ = resp;
    }

    ui.add_space(2.0);

    // ── VOICE ZONE ────────────────────────────────────────────────────────────
    if module_card::zone_rail(ui, "VOICES", true) {
        app.add_menu_zone = Some(Zone::Voice);
    }

    // Collect voice modules in slot order.
    let voice_ids: Vec<(u32, ModuleKind, bool)> = {
        let mut v: Vec<_> = app
            .state
            .read()
            .rack
            .modules
            .iter()
            .filter(|m| m.zone == Zone::Voice)
            .map(|m| (m.id, m.kind, m.enabled))
            .collect();
        v.sort_by_key(|&(id, _, _)| {
            app.state
                .read()
                .rack
                .modules
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.slot)
                .unwrap_or(0)
        });
        v
    };

    // Shared ctx clone for drag handling across both zones.
    let ctx_ref = ui.ctx().clone();

    // Voice modules — manual row grouping prevents any module from going off-screen.
    for row in group_into_rows(&voice_ids, available_w, 4.0) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
            for (id, kind, enabled) in &row {
                let slot_w = module_slot_w(*kind, available_w);
                // Dim the card ghost while it's being dragged
                let is_dragging = app.module_drag.as_ref().map(|d| d.module_id) == Some(*id);
                let eff_enabled = if is_dragging { false } else { *enabled };
                let (resp, _) = module_card::module_card(
                    ui,
                    *id,
                    *kind,
                    eff_enabled,
                    Some(slot_w),
                    ports,
                    |ui| {
                        draw_voice_content(app, ui, *kind);
                    },
                );
                if resp.toggle_clicked && !is_dragging {
                    let en = *enabled;
                    if let Some(m) = app
                        .state
                        .write()
                        .rack
                        .modules
                        .iter_mut()
                        .find(|m| m.id == *id)
                    {
                        m.enabled = !en;
                    }
                    app.push_fx_plan();
                }
                if resp.remove_clicked {
                    app.state.write().rack.remove_module(*id);
                    app.push_fx_plan();
                }
                let drop_pos = ctx_ref.pointer_latest_pos().unwrap_or_default();
                if handle_title_drag(app, &ctx_ref, *id, &resp) {
                    reorder_module_by_drop(app, *id, drop_pos, Zone::Voice);
                }
            }
        });
        ui.add_space(4.0);
    }

    ui.add_space(2.0);

    // ── FX + MOD ZONE ─────────────────────────────────────────────────────────
    if module_card::zone_rail(ui, "FX + MODULATION", true) {
        app.add_menu_zone = Some(Zone::FxMod);
    }

    let fxmod_ids: Vec<(u32, ModuleKind, bool)> = {
        let mut v: Vec<_> = app
            .state
            .read()
            .rack
            .modules
            .iter()
            .filter(|m| m.zone == Zone::FxMod)
            .map(|m| (m.id, m.kind, m.enabled))
            .collect();
        v.sort_by_key(|&(id, _, _)| {
            app.state
                .read()
                .rack
                .modules
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.slot)
                .unwrap_or(0)
        });
        v
    };

    // FX + Mod modules — manual row grouping, same as voice zone.
    for row in group_into_rows(&fxmod_ids, available_w, 4.0) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
            for (id, kind, enabled) in &row {
                let slot_w = module_slot_w(*kind, available_w);
                let is_dragging = app.module_drag.as_ref().map(|d| d.module_id) == Some(*id);
                let eff_enabled = if is_dragging { false } else { *enabled };
                let (resp, _) = module_card::module_card(
                    ui,
                    *id,
                    *kind,
                    eff_enabled,
                    Some(slot_w),
                    ports,
                    |ui| {
                        if *kind == ModuleKind::LfoModule {
                            draw_lfo_content(app, ui, *id);
                        } else {
                            draw_fx_content(app, ui, *kind);
                        }
                    },
                );
                if resp.toggle_clicked && !is_dragging {
                    let en = *enabled;
                    if let Some(m) = app
                        .state
                        .write()
                        .rack
                        .modules
                        .iter_mut()
                        .find(|m| m.id == *id)
                    {
                        m.enabled = !en;
                    }
                    app.push_fx_plan();
                }
                if resp.remove_clicked {
                    app.state.write().rack.remove_module(*id);
                    app.push_fx_plan();
                }
                let drop_pos = ctx_ref.pointer_latest_pos().unwrap_or_default();
                if handle_title_drag(app, &ctx_ref, *id, &resp) {
                    reorder_module_by_drop(app, *id, drop_pos, Zone::FxMod);
                }
            }
        });
        ui.add_space(4.0);
    }

    ui.add_space(4.0);
}

// ─── Module drag helpers ──────────────────────────────────────────────────────

/// Process drag start/stop from a card response and update app.module_drag.
/// Returns true if this card was just dropped (so caller can reorder slots).
fn handle_title_drag(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    id: u32,
    resp: &module_card::CardResponse,
) -> bool {
    if resp.title_dragged {
        if app.module_drag.as_ref().map(|d| d.module_id) != Some(id) {
            app.module_drag = Some(ModuleDrag {
                module_id: id,
                pointer: ctx.pointer_latest_pos().unwrap_or_default(),
            });
        } else if let Some(ref mut drag) = app.module_drag {
            drag.pointer = ctx.pointer_latest_pos().unwrap_or(drag.pointer);
        }
    }
    if resp.title_drag_released && app.module_drag.as_ref().map(|d| d.module_id) == Some(id) {
        app.module_drag = None;
        return true;
    }
    false
}

/// After a drop, reorder module slots so dragged module ends up at the position
/// closest to `drop_pos` within its zone's ordered list.
fn reorder_module_by_drop(
    app: &mut ImpulseApp,
    dragged_id: u32,
    drop_pos: egui::Pos2,
    zone: crate::state::Zone,
) {
    // Use 1200px as a reasonable default — drop position is proportional, not exact.
    let available_w = 1200.0f32;
    // Get ordered IDs + their widths for this zone
    let zone_entries: Vec<(u32, ModuleKind)> = {
        let rack = app.state.read();
        let mut v: Vec<_> = rack
            .rack
            .modules
            .iter()
            .filter(|m| m.zone == zone)
            .collect();
        v.sort_by_key(|m| m.slot);
        v.iter().map(|m| (m.id, m.kind)).collect()
    };
    let zone_ids: Vec<u32> = zone_entries.iter().map(|&(id, _)| id).collect();
    // Find dragged module's current index
    let Some(from_idx) = zone_ids.iter().position(|&id| id == dragged_id) else {
        return;
    };
    let n = zone_ids.len();
    if n < 2 {
        return;
    }
    // Estimate target index: walk cumulative widths to find which slot drop_pos.x falls in.
    // drop_pos.x is in screen space; subtract a small left margin approximation (8px).
    let x = (drop_pos.x - 8.0).max(0.0);
    let gap = 4.0f32;
    let mut cursor = 0.0f32;
    let mut to_idx = 0usize;
    for (i, &(_, kind)) in zone_entries.iter().enumerate() {
        let w = module_slot_w(kind, available_w);
        let mid = cursor + w * 0.5;
        if x >= mid {
            to_idx = i + 1;
        }
        cursor += w + gap;
    }
    let to_idx = to_idx.clamp(0, n - 1);
    if from_idx == to_idx {
        return;
    }
    // Reorder: move dragged to to_idx, shift others
    let mut ids = zone_ids;
    let removed = ids.remove(from_idx);
    ids.insert(to_idx, removed);
    // Write new slot values
    let mut state = app.state.write();
    for (slot, &id) in ids.iter().enumerate() {
        if let Some(m) = state.rack.modules.iter_mut().find(|m| m.id == id) {
            m.slot = slot as u8;
        }
    }
}

// ─── Content dispatchers (implementation in rack_content.rs) ─────────────────

use super::rack_content::{
    draw_fx_content, draw_lfo_content, draw_master_content, draw_voice_content, handle_cable_drag,
};
