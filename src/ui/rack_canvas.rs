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
    // Control points: drop down from the output port, rise up to input port,
    // creating a natural gravity-sag cable shape.
    let sag = (to - from).length().max(80.0) * 0.4;
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

// ─── Main rack canvas ─────────────────────────────────────────────────────────

pub fn draw_rack(app: &mut ImpulseApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    // Collect port positions during this frame's render pass.
    let mut ports: Vec<PortPos> = Vec::new();

    ScrollArea::vertical()
        .id_source("rack_scroll")
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());

            draw_rack_inner(app, ui, &mut ports);
        });

    // ── Cable overlay ─────────────────────────────────────────────────────────
    // After layout is complete, draw cables over everything using the absolute
    // screen positions we collected.
    {
        let cables = app.state.read().rack.cables.clone();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("cables"),
        ));

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

        // In-progress drag cable follows the cursor
        if let Some(ref drag) = app.cable_drag
            && let Some(pointer) = ctx.pointer_latest_pos()
        {
            draw_cable(&painter, drag.from_screen, pointer, Color32::from_gray(180));
        }
    }

    // ── Cable drag interaction ────────────────────────────────────────────────
    handle_cable_drag(app, ctx, &ports);
}

fn draw_rack_inner(app: &mut ImpulseApp, ui: &mut egui::Ui, ports: &mut Vec<PortPos>) {
    let available_w = ui.available_width();

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

    // ── VOICE ZONE ────────────────────────────────────────────────────────────
    module_card::zone_rail(ui, "VOICES", true);

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

    // Voice modules wrap horizontally — each gets at minimum 1/N of the width,
    // but is free to take as much as it needs (the content determines height).
    // We use horizontal_wrapped so they flow to new rows automatically.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);

        for (id, kind, enabled) in &voice_ids {
            // Each voice card gets a proportional share; clamp to sensible bounds.
            let n = voice_ids.len().max(1);
            let share_w = (available_w / n as f32 - 4.0)
                .max(320.0)
                .min(available_w - 8.0);

            let (resp, _) =
                module_card::module_card(ui, *id, *kind, *enabled, Some(share_w), ports, |ui| {
                    draw_voice_content(app, ui, *kind)
                });
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

    ui.add_space(2.0);

    // ── FX + MOD ZONE ─────────────────────────────────────────────────────────
    module_card::zone_rail(ui, "FX + MODULATION", true);

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

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);

        // Separate FX modules (medium cards) from LFO modules (small cards).
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

        // FX modules: aim for 4-across (wider than LFOs)
        let _fx_count = fx_ids.len().max(1);
        let fx_per_row = if available_w > 1400.0 {
            5
        } else if available_w > 900.0 {
            4
        } else {
            3
        };
        let fx_w = (available_w / fx_per_row as f32 - 4.0).max(200.0);

        for (id, kind, enabled) in &fx_ids {
            let (resp, _) =
                module_card::module_card(ui, *id, *kind, *enabled, Some(fx_w), ports, |ui| {
                    draw_fx_content(app, ui, *kind)
                });
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

        // LFO modules: compact cards
        let lfo_per_row = if available_w > 1200.0 { 6 } else { 4 };
        let lfo_w = (available_w / lfo_per_row as f32 - 4.0).max(160.0);

        for (id, kind, enabled) in &lfo_ids {
            let (resp, _) =
                module_card::module_card(ui, *id, *kind, *enabled, Some(lfo_w), ports, |ui| {
                    draw_lfo_content(app, ui, *id)
                });
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

    ui.add_space(4.0);
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
