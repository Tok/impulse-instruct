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

use egui::{Color32, ScrollArea, Vec2};

use crate::state::{ModuleKind, Zone};
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
const GLOBAL_KINDS: &[ModuleKind] = &[ModuleKind::LlmConsole, ModuleKind::LlmAgent];
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
    if let Some((g, v, f)) = app.state.write().collapse_requested.take() {
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
            // Draw a bright insertion line at the drop target position
            let drop_x = pointer.x;
            let screen = ctx.screen_rect();
            painter.line_segment(
                [
                    egui::pos2(drop_x, screen.min.y + 40.0),
                    egui::pos2(drop_x, screen.max.y - 40.0),
                ],
                egui::Stroke::new(2.0, Color32::from_gray(160)),
            );
            ctx.request_repaint();
        }
    }
    draw_add_menu(app, ctx);
}

fn draw_add_menu(app: &mut ImpulseApp, ctx: &egui::Context) {
    let zone = match app.add_menu_zone {
        Some(z) => z,
        None => return,
    };
    let kinds: &[ModuleKind] = match zone {
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

pub(super) fn module_slot_w(kind: ModuleKind, full_w: f32) -> f32 {
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
        | ModuleKind::FxAutotune
        | ModuleKind::LfoModule => FX_SLOT_W.min(full_w),
        // Agent cards: ~1/4 of rack width, minimum FX_SLOT_W
        ModuleKind::LlmAgent => (full_w / 4.0).max(FX_SLOT_W).min(full_w),
        // Analysis modules: wider for better resolution
        ModuleKind::SpectrumAnalyzer => 320.0_f32.min(full_w),
        ModuleKind::StereoMeter => FX_SLOT_W.min(full_w),
        ModuleKind::ActivityTimeline => 400.0_f32.min(full_w),
        // LLM Console + global modules: full width
        ModuleKind::LlmConsole => full_w,
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

/// For a row of modules, compute an expansion factor and left-padding so modules
/// fill the available width.  When expansion would be excessive (>1.4×), keep
/// modules at their nominal width and center the row instead.
fn row_expand_and_pad(row: &[(u32, ModuleKind, bool)], available_w: f32, gap: f32) -> (f32, f32) {
    let n = row.len() as f32;
    let total_gap = (n - 1.0).max(0.0) * gap;
    let nominal: f32 = row
        .iter()
        .map(|(_, k, _)| module_slot_w(*k, available_w))
        .sum();
    let fill_w = available_w - total_gap;
    if nominal <= 0.0 {
        return (1.0, 0.0);
    }
    let ratio = fill_w / nominal;
    if ratio <= 1.4 {
        // Expand proportionally so the row fills the available width.
        (ratio, 0.0)
    } else {
        // Would stretch too far — center at nominal widths instead.
        let pad = ((available_w - nominal - total_gap) / 2.0).max(0.0);
        (1.0, pad)
    }
}

fn draw_rack_inner(app: &mut ImpulseApp, ui: &mut egui::Ui, ports: &mut Vec<PortPos>) {
    let available_w = (ui.available_width() - 8.0).max(200.0);
    let mut card_rects: Vec<(ModuleKind, egui::Rect)> = Vec::new();

    let content_top = ui.cursor().top();

    app.zone_y[0] = ui.cursor().top() - content_top;
    {
        let (add, toggle) =
            module_card::zone_rail(ui, "GLOBAL", true, 24, app.zone_global_collapsed);
        if toggle {
            app.zone_global_collapsed = !app.zone_global_collapsed;
        }
        if add {
            app.add_menu_zone = Some(crate::state::Zone::Global);
        }
    }

    if !app.zone_global_collapsed {
        // LLM Console — style, prompt, JAM (singleton, full-width, always first)
        {
            let console_id = app
                .state
                .read()
                .rack
                .modules
                .iter()
                .find(|m| m.kind == ModuleKind::LlmConsole)
                .map(|m| m.id);
            if let Some(id) = console_id {
                let enabled = app
                    .state
                    .read()
                    .rack
                    .modules
                    .iter()
                    .find(|m| m.id == id)
                    .map(|m| m.enabled)
                    .unwrap_or(true);
                ui.add_space(2.0);
                let resp = if app.rack_flipped {
                    module_card::module_card_back(
                        ui,
                        id,
                        ModuleKind::LlmConsole,
                        enabled,
                        Some(available_w - 2.0),
                        app.kind_scale(ModuleKind::LlmConsole),
                        ports,
                    )
                } else {
                    module_card::module_card(
                        ui,
                        id,
                        ModuleKind::LlmConsole,
                        enabled,
                        Some(available_w - 2.0),
                        app.kind_scale(ModuleKind::LlmConsole),
                        ports,
                        |ui| {
                            app.draw_llm_console_content(ui);
                        },
                    )
                    .0
                };
                card_rects.push((ModuleKind::LlmConsole, resp.card_rect));
                if resp.toggle_clicked
                    && let Some(m) = app
                        .state
                        .write()
                        .rack
                        .modules
                        .iter_mut()
                        .find(|m| m.id == id)
                {
                    m.enabled = !m.enabled;
                }
            }
        }

        // LLM agent cards (compact, side-by-side)
        {
            let agent_ids: Vec<(u32, bool)> = app
                .state
                .read()
                .rack
                .modules
                .iter()
                .filter(|m| m.kind == ModuleKind::LlmAgent)
                .map(|m| (m.id, m.enabled))
                .collect();
            if !agent_ids.is_empty() {
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    let slot_w = module_slot_w(ModuleKind::LlmAgent, available_w);
                    // Center agent cards
                    let total_w = agent_ids.len() as f32 * (slot_w + ui.spacing().item_spacing.x);
                    let pad = ((available_w - total_w) / 2.0).max(0.0);
                    if pad > 0.0 {
                        ui.add_space(pad);
                    }
                    for (id, enabled) in &agent_ids {
                        let resp = if app.rack_flipped {
                            module_card::module_card_back(
                                ui,
                                *id,
                                ModuleKind::LlmAgent,
                                *enabled,
                                Some(slot_w),
                                app.kind_scale(ModuleKind::LlmAgent),
                                ports,
                            )
                        } else {
                            module_card::module_card(
                                ui,
                                *id,
                                ModuleKind::LlmAgent,
                                *enabled,
                                Some(slot_w),
                                app.kind_scale(ModuleKind::LlmAgent),
                                ports,
                                |ui| {
                                    draw_llm_agent_content(app, ui, *id);
                                },
                            )
                            .0
                        };
                        card_rects.push((ModuleKind::LlmAgent, resp.card_rect));
                        if resp.toggle_clicked
                            && let Some(m) = app
                                .state
                                .write()
                                .rack
                                .modules
                                .iter_mut()
                                .find(|m| m.id == *id)
                        {
                            m.enabled = !m.enabled;
                        }
                        if resp.remove_clicked {
                            app.state.write().rack.remove_module(*id);
                            app.state.write().llm_agents.retain(|a| a.id != *id);
                            app.push_fx_plan();
                        }
                    }
                });
            }
        }

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
            ui.add_space(2.0);
            let resp = if app.rack_flipped {
                module_card::module_card_back(
                    ui,
                    seq_id,
                    ModuleKind::StepSequencer,
                    enabled,
                    Some(available_w - 2.0),
                    app.kind_scale(ModuleKind::StepSequencer),
                    ports,
                )
            } else {
                module_card::module_card(
                    ui,
                    seq_id,
                    ModuleKind::StepSequencer,
                    enabled,
                    Some(available_w - 2.0),
                    app.kind_scale(ModuleKind::StepSequencer),
                    ports,
                    |ui| {
                        crate::ui::panels::draw_sequencer(app, ui);
                    },
                )
                .0
            };
            card_rects.push((ModuleKind::StepSequencer, resp.card_rect));
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

        // Master output
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
            ui.add_space(2.0);
            let resp = if app.rack_flipped {
                module_card::module_card_back(
                    ui,
                    master_id,
                    ModuleKind::MasterOutput,
                    true,
                    Some(available_w - 2.0),
                    app.kind_scale(ModuleKind::MasterOutput),
                    ports,
                )
            } else {
                module_card::module_card(
                    ui,
                    master_id,
                    ModuleKind::MasterOutput,
                    true,
                    Some(available_w - 2.0),
                    app.kind_scale(ModuleKind::MasterOutput),
                    ports,
                    |ui| {
                        draw_master_content(app, ui);
                    },
                )
                .0
            };
            card_rects.push((ModuleKind::MasterOutput, resp.card_rect));
        }

        ui.add_space(2.0);
    } // end zone_global_collapsed guard

    app.zone_y[1] = ui.cursor().top() - content_top;
    {
        let (add, toggle) =
            module_card::zone_rail(ui, "VOICES", true, 18, app.zone_voice_collapsed);
        if toggle {
            app.zone_voice_collapsed = !app.zone_voice_collapsed;
        }
        if add {
            app.add_menu_zone = Some(Zone::Voice);
        }
    }
    let ctx_ref = ui.ctx().clone();

    if !app.zone_voice_collapsed {
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

        // Voice modules — manual row grouping prevents any module from going off-screen.
        for row in group_into_rows(&voice_ids, available_w, 4.0) {
            let (expand, pad) = row_expand_and_pad(&row, available_w, 4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                if pad > 1.0 {
                    ui.add_space(pad);
                }
                for (id, kind, enabled) in &row {
                    let slot_w = module_slot_w(*kind, available_w) * expand;
                    // Dim the card ghost while it's being dragged
                    let is_dragging = app.module_drag.as_ref().map(|d| d.module_id) == Some(*id);
                    let eff_enabled = if is_dragging { false } else { *enabled };
                    let resp = if app.rack_flipped {
                        module_card::module_card_back(
                            ui,
                            *id,
                            *kind,
                            eff_enabled,
                            Some(slot_w),
                            app.kind_scale(*kind),
                            ports,
                        )
                    } else {
                        module_card::module_card(
                            ui,
                            *id,
                            *kind,
                            eff_enabled,
                            Some(slot_w),
                            app.kind_scale(*kind),
                            ports,
                            |ui| {
                                draw_voice_content(app, ui, *kind, *id);
                            },
                        )
                        .0
                    };
                    card_rects.push((*kind, resp.card_rect));
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
                        reorder_module_by_drop(app, *id, drop_pos, Zone::Voice, available_w);
                    }
                }
            });
            ui.add_space(4.0);
        }

        ui.add_space(2.0);
    } // end zone_voice_collapsed guard

    app.zone_y[2] = ui.cursor().top() - content_top;
    {
        let (add, toggle) =
            module_card::zone_rail(ui, "FX + MODULATION", true, 14, app.zone_fxmod_collapsed);
        if toggle {
            app.zone_fxmod_collapsed = !app.zone_fxmod_collapsed;
        }
        if add {
            app.add_menu_zone = Some(Zone::FxMod);
        }
    }

    if !app.zone_fxmod_collapsed {
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
            let (expand, pad) = row_expand_and_pad(&row, available_w, 4.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                if pad > 1.0 {
                    ui.add_space(pad);
                }
                for (id, kind, enabled) in &row {
                    let slot_w = module_slot_w(*kind, available_w) * expand;
                    let is_dragging = app.module_drag.as_ref().map(|d| d.module_id) == Some(*id);
                    let eff_enabled = if is_dragging { false } else { *enabled };
                    let resp = if app.rack_flipped {
                        module_card::module_card_back(
                            ui,
                            *id,
                            *kind,
                            eff_enabled,
                            Some(slot_w),
                            app.kind_scale(*kind),
                            ports,
                        )
                    } else {
                        module_card::module_card(
                            ui,
                            *id,
                            *kind,
                            eff_enabled,
                            Some(slot_w),
                            app.kind_scale(*kind),
                            ports,
                            |ui| {
                                if *kind == ModuleKind::LfoModule {
                                    draw_lfo_content(app, ui, *id);
                                } else if *kind == ModuleKind::SpectrumAnalyzer {
                                    crate::ui::panels::draw_spectrum(app, ui);
                                } else if *kind == ModuleKind::StereoMeter {
                                    crate::ui::panels::draw_stereo_meter(app, ui);
                                } else if *kind == ModuleKind::ActivityTimeline {
                                    crate::ui::panels::draw_timeline(app, ui);
                                } else {
                                    draw_fx_content(app, ui, *kind);
                                }
                            },
                        )
                        .0
                    };
                    card_rects.push((*kind, resp.card_rect));
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
                        reorder_module_by_drop(app, *id, drop_pos, Zone::FxMod, available_w);
                    }
                }
            });
            ui.add_space(4.0);
        }

        ui.add_space(4.0);
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
