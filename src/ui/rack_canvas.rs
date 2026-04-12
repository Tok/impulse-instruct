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

use egui::{Color32, ScrollArea};

use crate::state::{ModuleKind, Zone, rack::GRID_COLS};
use crate::ui::module_card::PortPos;
use crate::ui::{ImpulseApp, module_card, rack_cables};
// Re-export so callers referencing `rack_canvas::CableDrag` keep working.
pub use crate::ui::rack_cables::{CableDrag, ModuleDrag};
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
const AI_KINDS: &[ModuleKind] = &[ModuleKind::LlmConsole, ModuleKind::LlmAgent];
const GLOBAL_KINDS: &[ModuleKind] = &[];
const FXMOD_KINDS: &[ModuleKind] = &[
    ModuleKind::FxReverb,
    ModuleKind::FxDelay,
    ModuleKind::FxChorus,
    ModuleKind::FxPhaser,
    ModuleKind::FxEq,
    ModuleKind::FxCompressor,
    ModuleKind::FxTapeSat,
    ModuleKind::FxDrive,
    ModuleKind::FxAutotune,
    ModuleKind::FxWaveshaper,
    ModuleKind::FxBitcrush,
    ModuleKind::FxRingMod,
    ModuleKind::SpectrumAnalyzer,
    ModuleKind::StereoMeter,
    ModuleKind::ActivityTimeline,
    ModuleKind::LfoModule,
];

// ─── Main rack canvas ─────────────────────────────────────────────────────────

pub fn draw_rack(app: &mut ImpulseApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    {
        // publish agent persona names
        let s = app.state.read();
        for agent in &s.llm_agents {
            ctx.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new("agent_persona").with(agent.id),
                    agent.persona_name.clone(),
                );
            });
        }
    }
    if let Some((a, g, v, f)) = app.state.write().collapse_requested.take() {
        app.zone_ai_collapsed = a;
        app.zone_global_collapsed = g;
        app.zone_voice_collapsed = v;
        app.zone_fxmod_collapsed = f;
    }
    super::rack_toolbar::draw_toolbar(app, ui);
    ui.add_space(2.0);
    let mut ports: Vec<PortPos> = Vec::new();
    let canvas_rect = ui.available_rect_before_wrap();

    let ports_mem_id = egui::Id::new("rack_port_positions");
    let prev_ports: Vec<PortPos> = ctx
        .memory(|m| m.data.get_temp(ports_mem_id))
        .unwrap_or_default();
    let pointer_near_port = ctx
        .pointer_latest_pos()
        .map(|p| {
            prev_ports
                .iter()
                .any(|pp| pp.center.distance(p) <= module_card::PORT_RADIUS + 6.0)
        })
        .unwrap_or(false);
    let dragging_cable = app.cable_drag.is_some();
    let scroll_target: Option<String> = app.state.write().scroll_target.take();

    if let Some(ref t) = scroll_target
        && let Some(kind) = super::rack_scroll::resolve_focus_kind(t)
    {
        app.focused_module = Some(kind);
        app.focus_time = std::time::Instant::now();
        // Expand the zone containing the target so the module is visible
        match kind.default_zone() {
            crate::state::Zone::Ai => app.zone_ai_collapsed = false,
            crate::state::Zone::Global => app.zone_global_collapsed = false,
            crate::state::Zone::Voice => app.zone_voice_collapsed = false,
            crate::state::Zone::FxMod => app.zone_fxmod_collapsed = false,
        }
    }

    let scroll_out = ScrollArea::vertical()
        .id_source("rack_scroll")
        .drag_to_scroll(false)
        .auto_shrink([false; 2])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            draw_rack_inner(app, ui, &mut ports);
        });

    super::rack_scroll::handle_scroll(app, ctx, &scroll_target, &scroll_out);
    super::rack_scroll::publish_focus(app, ctx);

    ctx.memory_mut(|m| m.data.insert_temp(ports_mem_id, ports.clone()));
    rack_cables::draw_cable_overlay(app, ctx, &ports, canvas_rect);

    if (app.rack_flipped || app.cable_drag.is_some())
        && let Some(pointer) = ctx.pointer_latest_pos()
    {
        let time = ctx.input(|i| i.time) as f32;
        let mut overlay = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("port_highlights"),
        ));
        overlay.set_clip_rect(canvas_rect);

        for pp in &ports {
            let dist = pp.center.distance(pointer);
            let is_hovered = dist <= module_card::PORT_RADIUS + 6.0;

            if let Some(ref drag) = app.cable_drag {
                let is_source = pp.port == drag.from_port;
                let is_compatible = pp.port.dir != drag.from_port.dir
                    && pp.port.kind == drag.from_port.kind
                    && pp.port.module_id != drag.from_port.module_id;
                if is_source {
                    // Steady bright ring on the drag source port
                    overlay.circle_stroke(
                        pp.center,
                        module_card::PORT_RADIUS + 3.5,
                        egui::Stroke::new(1.5, egui::Color32::from_white_alpha(200)),
                    );
                } else if is_compatible {
                    // Pulsing ring on valid targets — faster pulse when hovered.
                    // Strictly grayscale (R=G=B) per ui-design.md.
                    let freq = if is_hovered { 4.0 } else { 2.0 };
                    let pulse = (time * freq * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                    let base_alpha = if is_hovered { 160u8 } else { 80u8 };
                    let alpha = (base_alpha as f32 + pulse * 80.0) as u8;
                    overlay.circle_stroke(
                        pp.center,
                        module_card::PORT_RADIUS + 3.5,
                        egui::Stroke::new(1.5, egui::Color32::from_white_alpha(alpha)),
                    );
                }
                // Incompatible ports: no decoration
            } else if is_hovered {
                // Idle hover: subtle static ring
                overlay.circle_stroke(
                    pp.center,
                    module_card::PORT_RADIUS + 2.5,
                    egui::Stroke::new(1.0, egui::Color32::from_white_alpha(110)),
                );
            }
        }

        // Cursor feedback
        if dragging_cable {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        } else if pointer_near_port {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    handle_cable_drag(app, ctx, &ports);
    if let Some(ref drag) = app.module_drag
        && let Some(pointer) = ctx.pointer_latest_pos()
    {
        // Compute snap position on the grid
        let rel_x = pointer.x - drag.zone_origin.x;
        let rel_y = pointer.y - drag.zone_origin.y;
        let snap_col = (rel_x / drag.step).round().max(0.0) as u8;
        let snap_row = (rel_y / drag.step).round().max(0.0) as u8;
        let snap_col = snap_col.min(GRID_COLS.saturating_sub(drag.col_span));

        // Ghost rect at the snapped grid position
        let gx = drag.zone_origin.x + snap_col as f32 * drag.step;
        let gy = drag.zone_origin.y + snap_row as f32 * drag.step;
        let gw = module_grid_w(
            app.state
                .read()
                .rack
                .modules
                .iter()
                .find(|m| m.id == drag.module_id)
                .map(|m| m.kind)
                .unwrap_or(ModuleKind::FxReverb),
            drag.col_w,
        );
        let gh =
            drag.row_span as f32 * drag.col_w + (drag.row_span as f32 - 1.0).max(0.0) * RACK_GAP;
        let ghost_rect = egui::Rect::from_min_size(egui::pos2(gx, gy), egui::vec2(gw, gh));

        // Check if drop position is blocked by another module
        let drop_blocked = {
            let s = app.state.read();
            s.rack
                .modules
                .iter()
                .filter(|m| m.id != drag.module_id && m.zone == drag.zone)
                .any(|m| {
                    let (mw, mh) = m.kind.grid_size(GRID_COLS);
                    snap_col < m.grid_col + mw
                        && m.grid_col < snap_col + drag.col_span
                        && snap_row < m.grid_row + mh
                        && m.grid_row < snap_row + drag.row_span
                })
        };

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("module_drag_ghost"),
        ));
        let (fill, stroke_col) = if drop_blocked {
            (
                Color32::from_rgba_premultiplied(80, 20, 20, 100),
                Color32::from_gray(80),
            )
        } else {
            (
                Color32::from_rgba_premultiplied(60, 60, 60, 100),
                Color32::from_gray(140),
            )
        };
        painter.rect_filled(ghost_rect, egui::Rounding::same(8.0), fill);
        painter.rect_stroke(
            ghost_rect,
            egui::Rounding::same(8.0),
            egui::Stroke::new(2.0, stroke_col),
        );
        ctx.request_repaint();
    }
    draw_add_menu(app, ctx);
    draw_remove_confirm(app, ctx);
}

fn draw_remove_confirm(app: &mut ImpulseApp, ctx: &egui::Context) {
    let module_id = match app.confirm_remove_module {
        Some(id) => id,
        None => return,
    };
    let label = app
        .state
        .read()
        .rack
        .modules
        .iter()
        .find(|m| m.id == module_id)
        .map(|m| m.kind.label().to_string())
        .unwrap_or_else(|| format!("Module #{}", module_id));
    let mut open = true;
    egui::Window::new("confirm_remove")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Remove {}?", label))
                    .monospace()
                    .size(9.5)
                    .color(Color32::from_gray(200)),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("Remove").clicked() {
                    let is_agent = app
                        .state
                        .read()
                        .rack
                        .modules
                        .iter()
                        .any(|m| m.id == module_id && m.kind == ModuleKind::LlmAgent);
                    app.state.write().rack.remove_module(module_id);
                    if is_agent {
                        app.state.write().llm_agents.retain(|a| a.id != module_id);
                    }
                    app.push_fx_plan();
                    app.confirm_remove_module = None;
                }
                if ui.button("Cancel").clicked() {
                    app.confirm_remove_module = None;
                }
            });
        });
    if !open {
        app.confirm_remove_module = None;
    }
}

fn draw_add_menu(app: &mut ImpulseApp, ctx: &egui::Context) {
    let zone = match app.add_menu_zone {
        Some(z) => z,
        None => return,
    };
    let kinds: &[ModuleKind] = match zone {
        crate::state::Zone::Ai => AI_KINDS,
        crate::state::Zone::Voice => VOICE_KINDS,
        crate::state::Zone::FxMod => FXMOD_KINDS,
        crate::state::Zone::Global => GLOBAL_KINDS,
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
                    let id = app.state.write().rack.add_module(*kind);
                    // Place the new module on the grid
                    app.state.write().rack.arrange_grid();
                    if *kind == ModuleKind::LlmAgent {
                        let agent =
                            crate::state::LlmAgentState::from_singleton(id, &app.state.read().llm);
                        app.state.write().llm_agents.push(agent);
                    }
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

// ─── Grid-based rack layout ──────────────────────────────────────────────────
// Fixed 12-column grid (like CSS/Bootstrap).  Each module spans a number of
// columns and rows defined in `ModuleKind::grid_size()`.

pub(super) const RACK_GAP: f32 = 4.0;

/// Size of one grid column (floored to whole pixels for crisp alignment).
pub(super) fn grid_col_w(available_w: f32) -> f32 {
    let n = GRID_COLS as f32;
    ((available_w - (n - 1.0) * RACK_GAP) / n).floor()
}

/// Pixel width for a module's grid column span.
pub(super) fn module_grid_w(kind: ModuleKind, col_w: f32) -> f32 {
    let (c, _) = kind.grid_size(GRID_COLS);
    c as f32 * col_w + (c as f32 - 1.0).max(0.0) * RACK_GAP
}

/// Pixel height for a module's grid row span.
pub(super) fn module_grid_h(kind: ModuleKind, col_w: f32) -> f32 {
    let (_, r) = kind.grid_size(GRID_COLS);
    let r = r as f32;
    r * col_w + (r - 1.0).max(0.0) * RACK_GAP
}

/// Dynamic row count for the sequencer grid cell — adapts to visible lanes.
pub(super) fn sequencer_grid_rows(state: &crate::state::AppState, col_w: f32) -> u8 {
    use crate::state::ModuleKind;
    let has = |k: ModuleKind| state.rack.modules.iter().any(|m| m.kind == k && m.enabled);
    let has_bass = has(ModuleKind::AcidBass);
    let has_hoover = has(ModuleKind::HooverLead);
    let has_an1x = has(ModuleKind::An1xVoice);
    let active_drum_voices = state
        .sequencer
        .drum_patterns
        .iter()
        .filter(|(_, p)| p.iter().any(|st| st.active))
        .count();
    // Pixel estimate — mirrors draw_sequencer. Tuned so the grid cell fits
    // the content exactly (no trailing empty row).
    let pad = state.ui_prefs.effective_pad_px();
    let step_row = pad + 22.0; // main step button row with padding
    let marker_row = 16.0; // accent / slide marker row (compact)
    let drum_sub = 14.0; // vel/prob/ratchet sub-lane under drum step row
    let header_h = 55.0;
    let sub_rows = state.sequencer.steps.min(64).div_ceil(32).max(1) as f32;
    let bass_h = if has_bass {
        sub_rows * (step_row + marker_row + marker_row)
    } else {
        0.0
    };
    let hoover_h = if has_hoover { sub_rows * step_row } else { 0.0 };
    let an1x_h = if has_an1x { sub_rows * step_row } else { 0.0 };
    let drums_h = active_drum_voices as f32 * sub_rows * (step_row + drum_sub);
    let total_h = header_h + bass_h + hoover_h + an1x_h + drums_h;
    // Convert pixel height to grid rows: each grid row occupies col_w + RACK_GAP
    // (minus one trailing gap on the last row).
    let step = col_w + RACK_GAP;
    let grid_rows = ((total_h + RACK_GAP) / step).ceil() as u8;
    grid_rows.max(2)
}

/// Pixel height for the sequencer, dynamically sized by rack contents.
pub(super) fn sequencer_grid_h(state: &crate::state::AppState, col_w: f32) -> f32 {
    let r = sequencer_grid_rows(state, col_w) as f32;
    r * col_w + (r - 1.0).max(0.0) * RACK_GAP
}

/// Grid step: col_w + gap. Cards at adjacent grid positions are separated by this.
pub(super) fn grid_step(col_w: f32) -> f32 {
    col_w + RACK_GAP
}

/// Width spanning `n` grid columns (including internal gaps).
#[allow(dead_code)]
pub(crate) fn span_w(n: u8, col_w: f32) -> f32 {
    n as f32 * col_w + (n as f32 - 1.0).max(0.0) * RACK_GAP
}

/// Published grid column width — readable from any panel via egui temp data.
/// Stored by `draw_rack_inner`, read via `grid_unit()`.
const GRID_COL_W_ID: &str = "rack_grid_col_w";

/// Read the current grid column width from egui temp data.
/// Panels can use this to size sub-elements to grid multiples.
/// Returns 0.0 if called outside the rack draw cycle.
#[allow(dead_code)]
pub(crate) fn grid_unit(ctx: &egui::Context) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(egui::Id::new(GRID_COL_W_ID)))
        .unwrap_or(0.0)
}

/// Compute card X position — mirrored horizontally when the rack is flipped.
pub(super) fn card_x(zone_left: f32, gc: u8, col_span: u8, step: f32, flipped: bool) -> f32 {
    if flipped {
        // Mirror: a card at column gc with width col_span maps to
        // column (GRID_COLS - gc - col_span) from the left.
        let mirror_col = GRID_COLS.saturating_sub(gc + col_span);
        zone_left + mirror_col as f32 * step
    } else {
        zone_left + gc as f32 * step
    }
}

/// Paint subtle dots at grid intersections within a zone's content area.
/// Called per-zone so each collapsible section gets its own aligned grid.
/// `zone_left` is the X where modules actually start (cursor X at zone start).
pub(super) fn draw_zone_grid_dots(
    ui: &egui::Ui,
    zone_left: f32,
    zone_top: f32,
    zone_bottom: f32,
    col_w: f32,
) {
    if col_w < 5.0 || zone_bottom <= zone_top {
        return;
    }
    let painter = ui.painter();
    let dot_color = Color32::from_gray(35);
    let step = col_w + RACK_GAP;
    // Dots sit at gap centerlines — offset by half_gap so a card spanning
    // columns c..c+n has dots at its left edge, between it and its neighbour,
    // and at its right edge.
    let half = RACK_GAP * 0.5;
    let ox = zone_left - half;
    let oy = zone_top - half;
    let right = ox + GRID_COLS as f32 * step + RACK_GAP;
    let mut y = oy;
    while y <= zone_bottom + step {
        for c in 0..=GRID_COLS {
            let x = ox + c as f32 * step;
            if x <= right + 1.0 {
                painter.circle_filled(egui::Pos2::new(x, y), 1.0, dot_color);
            }
        }
        y += step;
    }
}

fn draw_rack_inner(app: &mut ImpulseApp, ui: &mut egui::Ui, ports: &mut Vec<PortPos>) {
    let available_w = (ui.available_width() - 8.0).max(200.0);
    let col_w = grid_col_w(available_w);
    // Publish col_w so panels can read it via grid_unit()
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new(GRID_COL_W_ID), col_w));

    let mut card_rects: Vec<(ModuleKind, egui::Rect)> = Vec::new();

    // Compute which FX modules are connected via audio cables (for visual dimming).
    let connected_fx: std::collections::HashSet<u32> = {
        let s = app.state.read();
        s.rack
            .cables
            .iter()
            .filter(|c| c.from.kind == crate::state::PortKind::Audio)
            .flat_map(|c| [c.from.module_id, c.to.module_id])
            .filter(|id| {
                s.rack.modules.iter().any(|m| {
                    m.id == *id && crate::state::fx_plan::kind_to_fx_step(m.kind).is_some()
                })
            })
            .collect()
    };

    let content_top = ui.cursor().top();

    let all_collapsed = app.zone_ai_collapsed
        && app.zone_global_collapsed
        && app.zone_voice_collapsed
        && app.zone_fxmod_collapsed;

    // ─── AI zone (LLM console + agents) ──────────────────────────────────────
    super::rack_ai::draw_ai_zone(
        app,
        ui,
        ports,
        &mut card_rects,
        col_w,
        available_w,
        content_top,
        all_collapsed,
    );

    // ─── MAIN AUDIO zone (sequencer + master) ────────────────────────────────
    app.zone_y[1] = ui.cursor().top() - content_top;
    {
        let (add, toggle, toggle_all) = module_card::zone_rail(
            ui,
            "MAIN AUDIO",
            true,
            24,
            app.zone_global_collapsed,
            all_collapsed,
        );
        if toggle {
            app.zone_global_collapsed = !app.zone_global_collapsed;
        }
        if toggle_all {
            let target = !all_collapsed;
            app.zone_ai_collapsed = target;
            app.zone_global_collapsed = target;
            app.zone_voice_collapsed = target;
            app.zone_fxmod_collapsed = target;
        }
        if add {
            app.add_menu_zone = Some(crate::state::Zone::Global);
        }
    }

    if !app.zone_global_collapsed {
        let ctx_global = ui.ctx().clone();
        let zone_left = ui.cursor().left();
        let zone_top = ui.cursor().top();
        let step = grid_step(col_w);

        // Collect global modules with grid positions
        let global_mods: Vec<(u32, ModuleKind, bool, u8, u8)> = {
            let s = app.state.read();
            s.rack
                .modules
                .iter()
                .filter(|m| m.zone == Zone::Global)
                .map(|m| (m.id, m.kind, m.enabled, m.grid_col, m.grid_row))
                .collect()
        };
        let seq_rows = sequencer_grid_rows(&app.state.read(), col_w);
        // Sync dynamic sequencer height to rack state so arrange_grid uses it.
        {
            let mut s = app.state.write();
            if s.rack.dyn_sequencer_rows != Some(seq_rows) {
                s.rack.dyn_sequencer_rows = Some(seq_rows);
                s.rack.arrange_grid();
            }
        }
        let seq_rows = seq_rows as usize;
        let zone_rows = global_mods
            .iter()
            .map(|&(_, kind, _, _, gr)| {
                let h = if kind == ModuleKind::StepSequencer {
                    seq_rows
                } else {
                    let (_, h) = kind.grid_size(GRID_COLS);
                    h as usize
                };
                gr as usize + h
            })
            .max()
            .unwrap_or(0);
        let zone_h = zone_rows as f32 * step;
        let zone_rect = ui
            .allocate_exact_size(egui::Vec2::new(available_w, zone_h), egui::Sense::hover())
            .0;

        for &(id, kind, enabled, gc, gr) in &global_mods {
            let slot_w = module_grid_w(kind, col_w);
            let slot_h = if kind == ModuleKind::StepSequencer {
                sequencer_grid_h(&app.state.read(), col_w)
            } else {
                module_grid_h(kind, col_w)
            };
            let (col_span, _) = kind.grid_size(GRID_COLS);
            let x = card_x(zone_rect.min.x, gc, col_span, step, app.rack_flipped);
            let y = zone_rect.min.y + gr as f32 * step;
            let card_rect =
                egui::Rect::from_min_size(egui::Pos2::new(x, y), egui::Vec2::new(slot_w, slot_h));
            let mut child = ui.child_ui(card_rect, egui::Layout::top_down(egui::Align::LEFT), None);
            let resp = if app.rack_flipped {
                module_card::module_card_back(
                    &mut child,
                    id,
                    kind,
                    enabled,
                    Some(slot_w),
                    Some(slot_h),
                    app.kind_scale(kind),
                    ports,
                )
            } else {
                module_card::module_card_sized(
                    &mut child,
                    id,
                    kind,
                    enabled,
                    Some(slot_w),
                    Some(slot_h),
                    app.kind_scale(kind),
                    ports,
                    |ui| {
                        // Global zone content dispatch
                        match kind {
                            ModuleKind::LlmConsole => app.draw_llm_console_content(ui),
                            ModuleKind::LlmAgent => draw_llm_agent_content(app, ui, id),
                            ModuleKind::StepSequencer => crate::ui::panels::draw_sequencer(app, ui),
                            ModuleKind::MasterOutput => draw_master_content(app, ui),
                            _ => {}
                        }
                    },
                )
                .0
            };
            card_rects.push((kind, resp.card_rect));
            if resp.toggle_clicked
                && let Some(m) = app
                    .state
                    .write()
                    .rack
                    .modules
                    .iter_mut()
                    .find(|m| m.id == id)
            {
                m.enabled = !enabled;
            }
            if resp.remove_clicked {
                app.confirm_remove_module = Some(id);
            }
            // Drag only for non-fixed globals (agents)
            if kind == ModuleKind::LlmAgent {
                let drop_pos = ctx_global.pointer_latest_pos().unwrap_or_default();
                let zo = zone_rect.min;
                if handle_title_drag(app, &ctx_global, id, &resp, Zone::Global, zo, step, col_w) {
                    reorder_module_by_drop(app, id, drop_pos, Zone::Global, zo, step, col_w);
                }
            }
        }

        draw_zone_grid_dots(ui, zone_left, zone_top, zone_top + zone_h, col_w);
    } // end zone_global_collapsed guard

    app.zone_y[2] = ui.cursor().top() - content_top;
    {
        let (add, toggle, toggle_all) = module_card::zone_rail(
            ui,
            "VOICES",
            true,
            18,
            app.zone_voice_collapsed,
            all_collapsed,
        );
        if toggle {
            app.zone_voice_collapsed = !app.zone_voice_collapsed;
        }
        if toggle_all {
            let target = !all_collapsed;
            app.zone_ai_collapsed = target;
            app.zone_global_collapsed = target;
            app.zone_voice_collapsed = target;
            app.zone_fxmod_collapsed = target;
        }
        if add {
            app.add_menu_zone = Some(Zone::Voice);
        }
    }
    let ctx_ref = ui.ctx().clone();

    if !app.zone_voice_collapsed {
        let zone_left = ui.cursor().left();
        let zone_top = ui.cursor().top();
        let step = grid_step(col_w);

        // Collect voice modules with grid positions
        let voice_mods: Vec<(u32, ModuleKind, bool, u8, u8)> = {
            let s = app.state.read();
            s.rack
                .modules
                .iter()
                .filter(|m| m.zone == Zone::Voice)
                .map(|m| (m.id, m.kind, m.enabled, m.grid_col, m.grid_row))
                .collect()
        };

        // Compute zone height from module positions
        let zone_rows = voice_mods
            .iter()
            .map(|&(_, kind, _, _, gr)| {
                let (_, h) = kind.grid_size(GRID_COLS);
                gr as usize + h as usize
            })
            .max()
            .unwrap_or(0);
        let zone_h = zone_rows as f32 * step;

        // Reserve space for the entire zone grid
        let zone_rect = ui
            .allocate_exact_size(egui::Vec2::new(available_w, zone_h), egui::Sense::hover())
            .0;

        // Place each module at its grid coordinates
        for &(id, kind, enabled, gc, gr) in &voice_mods {
            let slot_w = module_grid_w(kind, col_w);
            let slot_h = module_grid_h(kind, col_w);
            let (col_span, _) = kind.grid_size(GRID_COLS);
            let x = card_x(zone_rect.min.x, gc, col_span, step, app.rack_flipped);
            let y = zone_rect.min.y + gr as f32 * step;
            let card_rect =
                egui::Rect::from_min_size(egui::Pos2::new(x, y), egui::Vec2::new(slot_w, slot_h));
            let mut child = ui.child_ui(card_rect, egui::Layout::top_down(egui::Align::LEFT), None);
            let is_dragging = app.module_drag.as_ref().map(|d| d.module_id) == Some(id);
            let eff_enabled = if is_dragging { false } else { enabled };
            let resp = if app.rack_flipped {
                module_card::module_card_back(
                    &mut child,
                    id,
                    kind,
                    eff_enabled,
                    Some(slot_w),
                    Some(slot_h),
                    app.kind_scale(kind),
                    ports,
                )
            } else {
                module_card::module_card_sized(
                    &mut child,
                    id,
                    kind,
                    eff_enabled,
                    Some(slot_w),
                    Some(slot_h),
                    app.kind_scale(kind),
                    ports,
                    |ui| {
                        draw_voice_content(app, ui, kind, id);
                    },
                )
                .0
            };
            card_rects.push((kind, resp.card_rect));
            if resp.toggle_clicked && !is_dragging {
                if let Some(m) = app
                    .state
                    .write()
                    .rack
                    .modules
                    .iter_mut()
                    .find(|m| m.id == id)
                {
                    m.enabled = !enabled;
                }
                app.push_fx_plan();
            }
            if resp.remove_clicked {
                app.confirm_remove_module = Some(id);
            }
            let drop_pos = ctx_ref.pointer_latest_pos().unwrap_or_default();
            let zo = zone_rect.min;
            if handle_title_drag(app, &ctx_ref, id, &resp, Zone::Voice, zo, step, col_w) {
                reorder_module_by_drop(app, id, drop_pos, Zone::Voice, zo, step, col_w);
            }
        }
        draw_zone_grid_dots(ui, zone_left, zone_top, zone_top + zone_h, col_w);
    } // end zone_voice_collapsed guard

    app.zone_y[3] = ui.cursor().top() - content_top;
    {
        let (add, toggle, toggle_all) = module_card::zone_rail(
            ui,
            "FX + MODULATION",
            true,
            14,
            app.zone_fxmod_collapsed,
            all_collapsed,
        );
        if toggle {
            app.zone_fxmod_collapsed = !app.zone_fxmod_collapsed;
        }
        if toggle_all {
            let target = !all_collapsed;
            app.zone_ai_collapsed = target;
            app.zone_global_collapsed = target;
            app.zone_voice_collapsed = target;
            app.zone_fxmod_collapsed = target;
        }
        if add {
            app.add_menu_zone = Some(Zone::FxMod);
        }
    }

    if !app.zone_fxmod_collapsed {
        let zone_left = ui.cursor().left();
        let zone_top = ui.cursor().top();
        let step = grid_step(col_w);
        let fx_mods: Vec<(u32, ModuleKind, bool, u8, u8)> = {
            let s = app.state.read();
            s.rack
                .modules
                .iter()
                .filter(|m| m.zone == Zone::FxMod)
                .map(|m| (m.id, m.kind, m.enabled, m.grid_col, m.grid_row))
                .collect()
        };
        let zone_rows = fx_mods
            .iter()
            .map(|&(_, kind, _, _, gr)| {
                let (_, h) = kind.grid_size(GRID_COLS);
                gr as usize + h as usize
            })
            .max()
            .unwrap_or(0);
        let zone_h = zone_rows as f32 * step;
        let zone_rect = ui
            .allocate_exact_size(egui::Vec2::new(available_w, zone_h), egui::Sense::hover())
            .0;
        for &(id, kind, enabled, gc, gr) in &fx_mods {
            let slot_w = module_grid_w(kind, col_w);
            let slot_h = module_grid_h(kind, col_w);
            let (col_span, _) = kind.grid_size(GRID_COLS);
            let x = card_x(zone_rect.min.x, gc, col_span, step, app.rack_flipped);
            let y = zone_rect.min.y + gr as f32 * step;
            let card_rect =
                egui::Rect::from_min_size(egui::Pos2::new(x, y), egui::Vec2::new(slot_w, slot_h));
            let mut child = ui.child_ui(card_rect, egui::Layout::top_down(egui::Align::LEFT), None);
            let is_dragging = app.module_drag.as_ref().map(|d| d.module_id) == Some(id);
            let eff_enabled = if is_dragging { false } else { enabled };
            let resp = if app.rack_flipped {
                module_card::module_card_back(
                    &mut child,
                    id,
                    kind,
                    eff_enabled,
                    Some(slot_w),
                    Some(slot_h),
                    app.kind_scale(kind),
                    ports,
                )
            } else {
                module_card::module_card_sized(
                    &mut child,
                    id,
                    kind,
                    eff_enabled,
                    Some(slot_w),
                    Some(slot_h),
                    app.kind_scale(kind),
                    ports,
                    |ui| {
                        if kind == ModuleKind::LfoModule {
                            draw_lfo_content(app, ui, id);
                        } else if kind == ModuleKind::SpectrumAnalyzer {
                            crate::ui::panels::draw_spectrum(app, ui);
                        } else if kind == ModuleKind::StereoMeter {
                            crate::ui::panels::draw_stereo_meter(app, ui);
                        } else if kind == ModuleKind::ActivityTimeline {
                            crate::ui::panels::draw_timeline(app, ui);
                        } else {
                            draw_fx_content(app, ui, kind);
                        }
                    },
                )
                .0
            };
            card_rects.push((kind, resp.card_rect));
            // Dim disconnected FX modules on the back panel
            if app.rack_flipped && !connected_fx.contains(&id) {
                let p = ui.painter();
                p.rect_filled(
                    resp.card_rect,
                    egui::Rounding::same(8.0),
                    Color32::from_rgba_premultiplied(0, 0, 0, 120),
                );
            }
            if resp.toggle_clicked && !is_dragging {
                if let Some(m) = app
                    .state
                    .write()
                    .rack
                    .modules
                    .iter_mut()
                    .find(|m| m.id == id)
                {
                    m.enabled = !enabled;
                }
                app.push_fx_plan();
            }
            if resp.remove_clicked {
                app.confirm_remove_module = Some(id);
            }
            let drop_pos = ctx_ref.pointer_latest_pos().unwrap_or_default();
            let zo = zone_rect.min;
            if handle_title_drag(app, &ctx_ref, id, &resp, Zone::FxMod, zo, step, col_w) {
                reorder_module_by_drop(app, id, drop_pos, Zone::FxMod, zo, step, col_w);
            }
        }
        draw_zone_grid_dots(ui, zone_left, zone_top, zone_top + zone_h, col_w);
    } // end zone_fxmod_collapsed guard

    ui.ctx().memory_mut(|m| {
        m.data
            .insert_temp(egui::Id::new("module_card_rects"), card_rects)
    });
}
// ─── Module drag helpers ──────────────────────────────────────────────────────
// ─── Content dispatchers (implementation in rack_content.rs) ─────────────────
use super::rack_content::{
    draw_fx_content, draw_lfo_content, draw_llm_agent_content, draw_master_content,
    draw_voice_content, handle_cable_drag, handle_title_drag, reorder_module_by_drop,
};
