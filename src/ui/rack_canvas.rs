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

use crate::state::{ModuleKind, Zone};
use crate::ui::module_card::PortPos;
use crate::ui::{ImpulseApp, module_card};

// ─── Cable drawing ───────────────────────────────────────────────────────────

/// Draw a single patch cable as a 3D bezier tube (two bezier passes).
fn draw_cable(painter: &egui::Painter, from: Pos2, to: Pos2, color: Color32) {
    // Gravity sag: proportional to horizontal span, clamped so short cables
    // droop gently and very long cables don't go off-screen.
    // When cable goes upward the sag still pulls down (catenary under gravity).
    let dx = (to.x - from.x).abs();
    let dy = to.y - from.y; // positive = going downward
    // Base sag from horizontal span; add a fraction of upward travel so cables
    // going up still form a relaxed arc rather than a taut line.
    let sag = (dx * 0.28 + (-dy).max(0.0) * 0.18).clamp(20.0, 160.0);
    let cp1 = from + Vec2::new(0.0, sag);
    let cp2 = to + Vec2::new(0.0, sag);

    let points: Vec<Pos2> = (0..=32)
        .map(|i| {
            let t = i as f32 / 32.0;
            let mt = 1.0 - t;
            // Cubic bezier
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
        })
        .collect();

    // Shadow pass (3px dark, offset down 1.5px)
    let shadow: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.0, 1.5)).collect();
    painter.add(egui::Shape::line(
        shadow,
        Stroke::new(3.5, Color32::from_black_alpha(120)),
    ));

    // Main colour pass (3px)
    painter.add(egui::Shape::line(points.clone(), Stroke::new(3.0, color)));

    // Specular highlight pass (1px white, offset up 1px — creates 3D tube illusion)
    let highlight_color = Color32::from_white_alpha(80);
    let highlight: Vec<Pos2> = points.iter().map(|p| *p + Vec2::new(0.0, -1.0)).collect();
    painter.add(egui::Shape::line(
        highlight,
        Stroke::new(1.0, highlight_color),
    ));
}

// ─── In-progress cable drag state ────────────────────────────────────────────

/// UI-local state for a cable currently being dragged.
#[derive(Clone, Debug)]
pub struct CableDrag {
    pub from_port: crate::state::PortRef,
    pub from_screen: Pos2,
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

    // Disable drag-to-scroll so cable dragging across the rack doesn't pan it.
    // Users scroll via mouse wheel or the always-visible scrollbar.
    ScrollArea::vertical()
        .id_source("rack_scroll")
        .drag_to_scroll(false)
        .auto_shrink([false; 2])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            draw_rack_inner(app, ui, &mut ports);
        });

    // ── Cable overlay (Tab to show/hide) ──────────────────────────────────────
    {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("cables"),
        ));

        // In-progress drag cable is always drawn regardless of show_cables,
        // so users get immediate feedback when starting a connection.
        if let Some(ref drag) = app.cable_drag
            && let Some(pointer) = ctx.pointer_latest_pos()
        {
            draw_cable(&painter, drag.from_screen, pointer, Color32::from_gray(200));
        }

        if app.show_cables {
            let cables = app.state.read().rack.cables.clone();
            for cable in &cables {
                let from_pos = ports
                    .iter()
                    .find(|p| p.port == cable.from)
                    .map(|p| p.center);
                let to_pos = ports.iter().find(|p| p.port == cable.to).map(|p| p.center);
                if let (Some(from), Some(to)) = (from_pos, to_pos) {
                    draw_cable(&painter, from, to, cable.color.egui_color());
                }
            }
        }
    }

    // ── Cable drag — always active, no mode required ──────────────────────────
    handle_cable_drag(app, ctx, &ports);

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
// widths based on how many modules are present — this keeps the grid stable.
//
//   full   = available_w           (1 per row — global / sequencer / master)
//   wide   = available_w / φ       ≈ 61.8 %   (1 per row — complex voices, AN1X)
//   narrow = available_w / φ²      ≈ 38.2 %   (2 per row — simple voices, FX, LFO)
//
// Two narrow modules take 76.4 % of the row; three take 114.6 % → wraps to 2+1.
const PHI: f32 = 1.618034;

fn module_slot_w(kind: ModuleKind, full_w: f32) -> f32 {
    match kind {
        ModuleKind::StepSequencer | ModuleKind::MasterOutput => full_w,
        ModuleKind::An1xVoice => full_w / PHI,
        _ => full_w / (PHI * PHI),
    }
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

    // Voice modules — φ-based fixed slot widths.
    {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
            for (id, kind, enabled) in &voice_ids {
                let slot_w = module_slot_w(*kind, available_w);
                let scoped = ui.scope(|ui| {
                    ui.set_min_width(slot_w);
                    ui.set_max_width(slot_w);
                    module_card::module_card(ui, *id, *kind, *enabled, None, ports, |ui| {
                        draw_voice_content(app, ui, *kind);
                    })
                });
                let (resp, _) = scoped.inner;
                if resp.toggle_clicked {
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
                }
                if resp.remove_clicked {
                    app.state.write().rack.remove_module(*id);
                }
            }
        });
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

    {
        let fx_ids: Vec<_> = fxmod_ids
            .iter()
            .filter(|(_, k, _)| *k != ModuleKind::LfoModule)
            .cloned()
            .collect();
        let lfo_ids: Vec<_> = fxmod_ids
            .iter()
            .filter(|(_, k, _)| *k == ModuleKind::LfoModule)
            .cloned()
            .collect();

        // All FX and LFO modules are "narrow" (φ² = 38.2 %)
        let fx_lfo_w = module_slot_w(ModuleKind::FxReverb, available_w); // narrow tier

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);

            for (id, kind, enabled) in &fx_ids {
                let scoped = ui.scope(|ui| {
                    ui.set_min_width(fx_lfo_w);
                    ui.set_max_width(fx_lfo_w);
                    module_card::module_card(ui, *id, *kind, *enabled, None, ports, |ui| {
                        draw_fx_content(app, ui, *kind);
                    })
                });
                let (resp, _) = scoped.inner;
                if resp.toggle_clicked {
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
                }
                if resp.remove_clicked {
                    app.state.write().rack.remove_module(*id);
                }
            }

            for (id, kind, enabled) in &lfo_ids {
                let scoped = ui.scope(|ui| {
                    ui.set_min_width(fx_lfo_w);
                    ui.set_max_width(fx_lfo_w);
                    module_card::module_card(ui, *id, *kind, *enabled, None, ports, |ui| {
                        draw_lfo_content(app, ui, *id);
                    })
                });
                let (resp, _) = scoped.inner;
                if resp.toggle_clicked {
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
                }
                if resp.remove_clicked {
                    app.state.write().rack.remove_module(*id);
                }
            }
        });
    }

    ui.add_space(4.0);
}

// ─── Master output content ────────────────────────────────────────────────────

fn draw_master_content(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::ui::widgets;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let mut master_vol = app.state.read().fx.master_volume;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("MASTER VOL")
                .monospace()
                .size(9.0)
                .color(egui::Color32::from_gray(90)),
        );
        if widgets::param_control(ui, "", &mut master_vol, crate::state::ParamMode::Free, ctrl).0 {
            app.state.write().fx.master_volume = master_vol;
            app.push_audio_params();
        }

        // Active voices indicator strip
        ui.separator();
        let rack = app.state.read();
        let voice_kinds = [
            (crate::state::ModuleKind::AcidBass, "BASS"),
            (crate::state::ModuleKind::DrumKit808, "808"),
            (crate::state::ModuleKind::DrumKit909, "909"),
            (crate::state::ModuleKind::HooverLead, "HVVR"),
            (crate::state::ModuleKind::An1xVoice, "AN1X"),
            (crate::state::ModuleKind::AmenSampler, "AMEN"),
        ];
        for (kind, label) in &voice_kinds {
            let present = rack
                .rack
                .modules
                .iter()
                .any(|m| m.kind == *kind && m.enabled);
            let col = if present {
                egui::Color32::from_gray(160)
            } else {
                egui::Color32::from_gray(28)
            };
            ui.label(egui::RichText::new(*label).monospace().size(8.0).color(col));
        }
    });
}

// ─── Content dispatchers ──────────────────────────────────────────────────────

fn draw_voice_content(app: &mut ImpulseApp, ui: &mut egui::Ui, kind: ModuleKind) {
    match kind {
        ModuleKind::AcidBass => crate::ui::panels::draw_bass(app, ui),
        ModuleKind::DrumKit808 => crate::ui::panels::draw_kit_a(app, ui),
        ModuleKind::DrumKit909 => crate::ui::panels::draw_kit_b(app, ui),
        ModuleKind::HooverLead => crate::ui::panels::draw_hoover(app, ui),
        ModuleKind::An1xVoice => crate::ui::panels::draw_an1x(app, ui),
        ModuleKind::AmenSampler => crate::ui::panels::draw_amen(app, ui),
        ModuleKind::NoiseVoice => draw_noise_stub(app, ui),
        _ => {}
    }
}

fn draw_fx_content(app: &mut ImpulseApp, ui: &mut egui::Ui, kind: ModuleKind) {
    // Each FX module renders only its own group — extracted from the full FX
    // panel so they can be placed as individual cards.
    use crate::ui::widgets;

    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);
    let mut changed = false;

    match kind {
        ModuleKind::FxReverb => {
            let (mut rs, mut rd, mut rm) = {
                let s = app.state.read();
                (s.fx.reverb_size, s.fx.reverb_damp, s.fx.reverb_mix)
            };
            widgets::param_control(ui, "SIZE", &mut rs, pm("fx.reverb_size"), ctrl);
            if widgets::param_control(ui, "DAMP", &mut rd, pm("fx.reverb_damp"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut rm, pm("fx.reverb_mix"), ctrl).0 {
                changed = true;
            }
            if changed || rs != app.state.read().fx.reverb_size {
                let mut s = app.state.write();
                s.fx.reverb_size = rs;
                s.fx.reverb_damp = rd;
                s.fx.reverb_mix = rm;
            }
        }
        ModuleKind::FxDelay => {
            let (mut dt, mut df, mut dm) = {
                let s = app.state.read();
                (s.fx.delay_time, s.fx.delay_feedback, s.fx.delay_mix)
            };
            widgets::param_control(ui, "TIME", &mut dt, pm("fx.delay_time"), ctrl);
            widgets::param_control(ui, "FDBK", &mut df, pm("fx.delay_feedback"), ctrl);
            if widgets::param_control(ui, "MIX", &mut dm, pm("fx.delay_mix"), ctrl).0 {
                changed = true;
            }
            if changed || dt != app.state.read().fx.delay_time {
                let mut s = app.state.write();
                s.fx.delay_time = dt;
                s.fx.delay_feedback = df;
                s.fx.delay_mix = dm;
            }
        }
        ModuleKind::FxChorus => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.chorus_rate, s.fx.chorus_depth, s.fx.chorus_mix)
            };
            widgets::param_control(ui, "RATE", &mut r, pm("fx.chorus_rate"), ctrl);
            widgets::param_control(ui, "DEPTH", &mut d, pm("fx.chorus_depth"), ctrl);
            if widgets::param_control(ui, "MIX", &mut m, pm("fx.chorus_mix"), ctrl).0 {
                changed = true;
            }
            if changed || r != app.state.read().fx.chorus_rate {
                let mut s = app.state.write();
                s.fx.chorus_rate = r;
                s.fx.chorus_depth = d;
                s.fx.chorus_mix = m;
            }
        }
        ModuleKind::FxPhaser => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.phaser_rate, s.fx.phaser_depth, s.fx.phaser_mix)
            };
            widgets::param_control(ui, "RATE", &mut r, pm("fx.phaser_rate"), ctrl);
            widgets::param_control(ui, "DEPTH", &mut d, pm("fx.phaser_depth"), ctrl);
            if widgets::param_control(ui, "MIX", &mut m, pm("fx.phaser_mix"), ctrl).0 {
                changed = true;
            }
            if changed || r != app.state.read().fx.phaser_rate {
                let mut s = app.state.write();
                s.fx.phaser_rate = r;
                s.fx.phaser_depth = d;
                s.fx.phaser_mix = m;
            }
        }
        ModuleKind::FxEq => {
            let (mut lo, mut mi, mut hi) = {
                let s = app.state.read();
                (s.fx.eq_low_gain, s.fx.eq_mid_gain, s.fx.eq_hi_gain)
            };
            widgets::param_control(ui, "LOW", &mut lo, pm("fx.eq_low_gain"), ctrl);
            widgets::param_control(ui, "MID", &mut mi, pm("fx.eq_mid_gain"), ctrl);
            if widgets::param_control(ui, "HIGH", &mut hi, pm("fx.eq_hi_gain"), ctrl).0 {
                changed = true;
            }
            if changed || lo != app.state.read().fx.eq_low_gain {
                let mut s = app.state.write();
                s.fx.eq_low_gain = lo;
                s.fx.eq_mid_gain = mi;
                s.fx.eq_hi_gain = hi;
            }
        }
        ModuleKind::FxCompressor => {
            let (mut th, mut ra, mut mi) = {
                let s = app.state.read();
                (
                    s.fx.compressor_threshold,
                    s.fx.compressor_ratio,
                    s.fx.compressor_mix,
                )
            };
            widgets::param_control(ui, "THRESH", &mut th, pm("fx.compressor_threshold"), ctrl);
            widgets::param_control(ui, "RATIO", &mut ra, pm("fx.compressor_ratio"), ctrl);
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.compressor_mix"), ctrl).0 {
                changed = true;
            }
            if changed || th != app.state.read().fx.compressor_threshold {
                let mut s = app.state.write();
                s.fx.compressor_threshold = th;
                s.fx.compressor_ratio = ra;
                s.fx.compressor_mix = mi;
            }
        }
        ModuleKind::FxTapeSat => {
            let (mut dr, mut fl, mut mi) = {
                let s = app.state.read();
                (s.fx.tape_drive, s.fx.tape_flutter, s.fx.tape_mix)
            };
            widgets::param_control(ui, "DRIVE", &mut dr, pm("fx.tape_drive"), ctrl);
            widgets::param_control(ui, "FLUTTER", &mut fl, pm("fx.tape_flutter"), ctrl);
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.tape_mix"), ctrl).0 {
                changed = true;
            }
            if changed || dr != app.state.read().fx.tape_drive {
                let mut s = app.state.write();
                s.fx.tape_drive = dr;
                s.fx.tape_flutter = fl;
                s.fx.tape_mix = mi;
            }
        }
        ModuleKind::FxDrive => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.distortion_drive, s.fx.distortion_mix)
            };
            if widgets::param_control(ui, "DRIVE", &mut dr, pm("fx.distortion_drive"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.distortion_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.distortion_drive = dr;
                s.fx.distortion_mix = mi;
            }
        }
        ModuleKind::FxWaveshaper => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.waveshaper_drive, s.fx.waveshaper_mix)
            };
            if widgets::param_control(ui, "DRIVE", &mut dr, pm("fx.waveshaper_drive"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.waveshaper_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.waveshaper_drive = dr;
                s.fx.waveshaper_mix = mi;
            }
        }
        ModuleKind::FxBitcrush => {
            let (mut bi, mut ra, mut mi) = {
                let s = app.state.read();
                (s.fx.bitcrush_bits, s.fx.bitcrush_rate, s.fx.bitcrush_mix)
            };
            widgets::param_control(ui, "BITS", &mut bi, pm("fx.bitcrush_bits"), ctrl);
            widgets::param_control(ui, "RATE", &mut ra, pm("fx.bitcrush_rate"), ctrl);
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.bitcrush_mix"), ctrl).0 {
                changed = true;
            }
            if changed || bi != app.state.read().fx.bitcrush_bits {
                let mut s = app.state.write();
                s.fx.bitcrush_bits = bi;
                s.fx.bitcrush_rate = ra;
                s.fx.bitcrush_mix = mi;
            }
        }
        ModuleKind::FxRingMod => {
            let (mut fr, mut mi) = {
                let s = app.state.read();
                (s.fx.ring_mod_freq, s.fx.ring_mod_mix)
            };
            if widgets::param_control(ui, "FREQ", &mut fr, pm("fx.ring_mod_freq"), ctrl).0 {
                changed = true;
            }
            if widgets::param_control(ui, "MIX", &mut mi, pm("fx.ring_mod_mix"), ctrl).0 {
                changed = true;
            }
            if changed {
                let mut s = app.state.write();
                s.fx.ring_mod_freq = fr;
                s.fx.ring_mod_mix = mi;
            }
        }
        _ => {}
    }

    if changed {
        app.push_audio_params();
    }
}

fn draw_lfo_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    // Map module_id → LFO slot index (0–3) based on insertion order.
    let slot = {
        let rack = app.state.read();
        rack.rack
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::LfoModule)
            .enumerate()
            .find(|(_, m)| m.id == module_id)
            .map(|(i, _)| i)
            .unwrap_or(0)
    };
    crate::ui::panels::draw_lfo_slot(app, ui, slot);
}

fn draw_noise_stub(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::ui::widgets;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);

    let (mut vol, mut color, mut cutoff) = {
        let s = app.state.read();
        (
            s.noise_voice.volume,
            s.noise_voice.color,
            s.noise_voice.cutoff,
        )
    };
    let mut changed = false;
    if widgets::param_control(ui, "VOLUME", &mut vol, pm("noise_voice.volume"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "COLOR", &mut color, pm("noise_voice.color"), ctrl).0 {
        changed = true;
    }
    if widgets::param_control(ui, "CUTOFF", &mut cutoff, pm("noise_voice.cutoff"), ctrl).0 {
        changed = true;
    }
    if changed {
        {
            let mut s = app.state.write();
            s.noise_voice.volume = vol;
            s.noise_voice.color = color;
            s.noise_voice.cutoff = cutoff;
        }
        app.push_audio_params();
    }
}

// ─── Cable drag interaction ───────────────────────────────────────────────────

fn handle_cable_drag(app: &mut ImpulseApp, ctx: &egui::Context, ports: &[PortPos]) {
    let pointer = match ctx.pointer_latest_pos() {
        Some(p) => p,
        None => return,
    };
    let primary_down = ctx.input(|i| i.pointer.primary_down());
    let primary_released = ctx.input(|i| i.pointer.primary_released());

    // Check if pointer is over any port
    let hovered_port = ports
        .iter()
        .find(|pp| pp.center.distance(pointer) <= module_card::PORT_RADIUS + 3.0);

    // Start a drag when primary button pressed over a port
    if primary_down
        && app.cable_drag.is_none()
        && let Some(pp) = hovered_port
    {
        app.cable_drag = Some(CableDrag {
            from_port: pp.port.clone(),
            from_screen: pp.center,
        });
    }

    // Complete drag on release — .take() cancels the drag regardless of whether
    // we find a valid target, so no explicit "else: cancel" branch is needed.
    if primary_released
        && let Some(drag) = app.cable_drag.take()
        && let Some(target) = hovered_port
        && drag.from_port.dir != target.port.dir
        && drag.from_port.kind == target.port.kind
        && drag.from_port.module_id != target.port.module_id
    {
        let (from, to) = if drag.from_port.dir == crate::state::PortDir::Out {
            (drag.from_port, target.port.clone())
        } else {
            (target.port.clone(), drag.from_port)
        };
        app.state.write().rack.connect(from, to);
    }
}
