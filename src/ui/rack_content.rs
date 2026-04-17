// ─── ui/rack_content.rs ──────────────────────────────────────────────────────
// Content draw functions for each module kind, split from rack_canvas.rs to
// keep file sizes under the 1000-line limit.

use crate::state::ModuleKind;
use crate::ui::fx_dir::draw_fx_dir_button;
use crate::ui::{ImpulseApp, module_card, theme};

pub(super) fn draw_voice_content(
    app: &mut ImpulseApp,
    ui: &mut egui::Ui,
    kind: ModuleKind,
    module_id: u32,
) {
    match kind {
        ModuleKind::AcidBass => crate::ui::panels::draw_bass(app, ui),
        ModuleKind::DrumKit808 => crate::ui::panels::draw_kit_a(app, ui),
        ModuleKind::DrumKit909 => crate::ui::panels::draw_kit_b(app, ui),
        ModuleKind::HooverLead => crate::ui::panels::draw_hoover(app, ui),
        ModuleKind::An1xVoice => crate::ui::panels::draw_an1x(app, ui),
        ModuleKind::AmenSampler => crate::ui::panels::draw_amen(app, ui),
        ModuleKind::NoiseVoice => crate::ui::panels::draw_noise(app, ui),
        ModuleKind::GranularTexture => crate::ui::panels::draw_granular(app, ui),
        ModuleKind::GabberKick => crate::ui::panels::draw_gabber(app, ui),
        ModuleKind::NeuTts => crate::ui::panels::draw_tts(app, ui, module_id),
        _ => {}
    }
}

pub(super) fn draw_fx_content(app: &mut ImpulseApp, ui: &mut egui::Ui, kind: ModuleKind) {
    use crate::ui::widgets;

    let scale: f32 = ui
        .ctx()
        .data(|d| d.get_temp(egui::Id::new("module_scale")))
        .unwrap_or(1.0);
    let ctrl = widgets::ControlPrefs::from_prefs_scaled(&app.state.read().ui_prefs, scale);
    let locked = app.state.read().llm.locked_params.clone();
    let focused = app.state.read().llm.focused_params.clone();
    let pm = |path: &str| crate::state::param_mode(path, &locked, &focused);
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = crate::ui::panels::KNOB_SPACING;

    // Helper: horizontal row of knobs
    macro_rules! hk {
        ($ui:expr, $( ($label:expr, $val:expr, $pm:expr) ),+ $(,)?) => {
            widgets::centered_row($ui, |ui| {
                $( if widgets::param_control(ui, $label, $val, $pm, ctrl).0 { changed = true; } )+
            });
        }
    }

    match kind {
        ModuleKind::FxReverb => {
            let (mut rs, mut rd, mut rm, mut rdir, mut rq) = {
                let s = app.state.read();
                (
                    s.fx.reverb_size,
                    s.fx.reverb_damp,
                    s.fx.reverb_mix,
                    s.fx.reverb_dir,
                    s.fx.reverb_rev_quant,
                )
            };
            hk!(
                ui,
                ("SIZE", &mut rs, pm("fx.reverb_size")),
                ("DAMPING", &mut rd, pm("fx.reverb_damp")),
                ("MIX", &mut rm, pm("fx.reverb_mix"))
            );
            let dir_changed = draw_fx_dir_button(ui, &mut rdir, "Reverb direction");
            let q_changed = crate::ui::fx_dir::draw_fx_rev_quant_button(ui, &mut rq, "Reverb");
            if changed || rs != app.state.read().fx.reverb_size || dir_changed || q_changed {
                let mut s = app.state.write();
                s.fx.reverb_size = rs;
                s.fx.reverb_damp = rd;
                s.fx.reverb_mix = rm;
                s.fx.reverb_dir = rdir;
                s.fx.reverb_rev_quant = rq;
            }
        }
        ModuleKind::FxDelay => {
            let (mut dt, mut df, mut dm, mut ddir, mut dq) = {
                let s = app.state.read();
                (
                    s.fx.delay_time,
                    s.fx.delay_feedback,
                    s.fx.delay_mix,
                    s.fx.delay_dir,
                    s.fx.delay_rev_quant,
                )
            };
            hk!(
                ui,
                ("TIME", &mut dt, pm("fx.delay_time")),
                ("FEEDBACK", &mut df, pm("fx.delay_feedback")),
                ("MIX", &mut dm, pm("fx.delay_mix"))
            );
            let dir_changed = draw_fx_dir_button(ui, &mut ddir, "Delay direction");
            let q_changed = crate::ui::fx_dir::draw_fx_rev_quant_button(ui, &mut dq, "Delay");
            if changed || dt != app.state.read().fx.delay_time || dir_changed || q_changed {
                let mut s = app.state.write();
                s.fx.delay_time = dt;
                s.fx.delay_feedback = df;
                s.fx.delay_mix = dm;
                s.fx.delay_dir = ddir;
                s.fx.delay_rev_quant = dq;
            }
        }
        ModuleKind::FxChorus => {
            let (mut r, mut d, mut m) = {
                let s = app.state.read();
                (s.fx.chorus_rate, s.fx.chorus_depth, s.fx.chorus_mix)
            };
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.chorus_rate")),
                ("DEPTH", &mut d, pm("fx.chorus_depth")),
                ("MIX", &mut m, pm("fx.chorus_mix"))
            );
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
            hk!(
                ui,
                ("RATE", &mut r, pm("fx.phaser_rate")),
                ("DEPTH", &mut d, pm("fx.phaser_depth")),
                ("MIX", &mut m, pm("fx.phaser_mix"))
            );
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
            hk!(
                ui,
                ("LOW", &mut lo, pm("fx.eq_low_gain")),
                ("MID", &mut mi, pm("fx.eq_mid_gain")),
                ("HIGH", &mut hi, pm("fx.eq_hi_gain"))
            );
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
            hk!(
                ui,
                ("THRESH", &mut th, pm("fx.compressor_threshold")),
                ("RATIO", &mut ra, pm("fx.compressor_ratio")),
                ("MIX", &mut mi, pm("fx.compressor_mix"))
            );
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
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.tape_drive")),
                ("FLUTTER", &mut fl, pm("fx.tape_flutter")),
                ("MIX", &mut mi, pm("fx.tape_mix"))
            );
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
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.distortion_drive")),
                ("MIX", &mut mi, pm("fx.distortion_mix"))
            );
            if changed {
                let mut s = app.state.write();
                s.fx.distortion_drive = dr;
                s.fx.distortion_mix = mi;
            }
        }
        ModuleKind::FxAutotune => {
            let (mut amt, mut mi) = {
                let s = app.state.read();
                (s.fx.autotune_amount, s.fx.autotune_mix)
            };
            hk!(
                ui,
                ("AMOUNT", &mut amt, pm("fx.autotune_amount")),
                ("MIX", &mut mi, pm("fx.autotune_mix"))
            );
            if changed {
                let mut s = app.state.write();
                s.fx.autotune_amount = amt;
                s.fx.autotune_mix = mi;
            }
        }
        ModuleKind::FxPan => {
            let (mut pos, mut width, mut rate) = {
                let s = app.state.read();
                (s.fx.fx_pan_pos, s.fx.fx_pan_width, s.fx.fx_pan_rate)
            };
            // POS control in the panel takes 0..1; map to -1..+1 under the
            // hood so the centre-click / drag feel matches other knobs.
            let mut pos_norm = (pos + 1.0) * 0.5;
            hk!(
                ui,
                ("POS", &mut pos_norm, pm("fx.fx_pan_pos")),
                ("WIDTH", &mut width, pm("fx.fx_pan_width")),
                ("RATE", &mut rate, pm("fx.fx_pan_rate"))
            );
            if changed {
                pos = (pos_norm * 2.0 - 1.0).clamp(-1.0, 1.0);
                let mut s = app.state.write();
                s.fx.fx_pan_pos = pos;
                s.fx.fx_pan_width = width;
                s.fx.fx_pan_rate = rate;
            }
        }
        ModuleKind::FxWaveshaper => {
            let (mut dr, mut mi) = {
                let s = app.state.read();
                (s.fx.waveshaper_drive, s.fx.waveshaper_mix)
            };
            hk!(
                ui,
                ("DRIVE", &mut dr, pm("fx.waveshaper_drive")),
                ("MIX", &mut mi, pm("fx.waveshaper_mix"))
            );
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
            hk!(
                ui,
                ("BITS", &mut bi, pm("fx.bitcrush_bits")),
                ("RATE", &mut ra, pm("fx.bitcrush_rate")),
                ("MIX", &mut mi, pm("fx.bitcrush_mix"))
            );
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
            hk!(
                ui,
                ("FREQ", &mut fr, pm("fx.ring_mod_freq")),
                ("MIX", &mut mi, pm("fx.ring_mod_mix"))
            );
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
        // Observe key FX params for style tracking
        let fx = &app.state.read().fx.clone();
        app.observe_edits(&[
            ("fx.reverb_mix", fx.reverb_mix),
            ("fx.delay_mix", fx.delay_mix),
            ("fx.delay_feedback", fx.delay_feedback),
            ("fx.chorus_mix", fx.chorus_mix),
            ("fx.compressor_threshold", fx.compressor_threshold),
            ("fx.distortion_drive", fx.distortion_drive),
            ("fx.master_volume", fx.master_volume),
            ("fx.stereo_width", fx.stereo_width),
        ]);
    }
}

pub(super) fn draw_lfo_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
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

pub(super) fn draw_master_content(app: &mut ImpulseApp, ui: &mut egui::Ui) {
    use crate::ui::widgets;
    let ctrl = widgets::ControlPrefs::from_prefs(&app.state.read().ui_prefs);
    let mut master_vol = app.state.read().fx.master_volume;

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("MASTER VOLUME")
                .monospace()
                .size(9.0)
                .color(theme::ASH),
        );
        if widgets::param_control(ui, "", &mut master_vol, crate::state::ParamMode::Free, ctrl).0 {
            app.state.write().fx.master_volume = master_vol;
            app.push_audio_params();
        }

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
                theme::PIT
            };
            ui.label(egui::RichText::new(*label).monospace().size(8.0).color(col));
        }
    });
}

pub(super) use super::agent_card::draw_llm_agent_content;

// ─── Cable drag interaction ───────────────────────────────────────────────────
/// Two port kinds can be patched together iff they're identical OR one side
/// is CV (LFO/seq output) and the other is Mod (per-knob modulation input).
fn port_kinds_compatible(a: crate::state::PortKind, b: crate::state::PortKind) -> bool {
    use crate::state::PortKind::*;
    a == b || matches!((a, b), (Cv, Mod) | (Mod, Cv))
}

pub(super) fn handle_cable_drag(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    ports: &[module_card::PortPos],
) {
    // Cable patching only in back-panel view (allow in-progress drags to complete)
    if !app.rack_flipped && app.cable_drag.is_none() {
        return;
    }
    let pointer = match ctx.pointer_latest_pos() {
        Some(p) => p,
        None => return,
    };
    let primary_down = ctx.input(|i| i.pointer.primary_down());
    let primary_released = ctx.input(|i| i.pointer.primary_released());

    let hovered_port = ports
        .iter()
        .find(|pp| pp.center.distance(pointer) <= module_card::PORT_RADIUS + 3.0);

    if primary_down
        && app.cable_drag.is_none()
        && let Some(pp) = hovered_port
    {
        app.cable_drag = Some(super::rack_canvas::CableDrag {
            from_port: pp.port.clone(),
            from_screen: pp.center,
        });
    }

    if primary_released
        && let Some(drag) = app.cable_drag.take()
        && let Some(target) = hovered_port
        && drag.from_port.dir != target.port.dir
        && port_kinds_compatible(drag.from_port.kind, target.port.kind)
        && drag.from_port.module_id != target.port.module_id
    {
        let (from, to) = if drag.from_port.dir == crate::state::PortDir::Out {
            (drag.from_port, target.port.clone())
        } else {
            (target.port.clone(), drag.from_port)
        };
        app.state.write().rack.connect(from, to);
        app.push_fx_plan();
    }

    // Right-click a port to disconnect all cables attached to it.
    let secondary_released = ctx.input(|i| i.pointer.secondary_released());
    if secondary_released
        && app.cable_drag.is_none()
        && let Some(pp) = hovered_port
    {
        let prev_len = app.state.read().rack.cables.len();
        app.state
            .write()
            .rack
            .cables
            .retain(|c| c.from != pp.port && c.to != pp.port);
        if app.state.read().rack.cables.len() != prev_len {
            app.push_fx_plan();
        }
    }
}

// ─── Drag helpers (moved from rack_canvas.rs to stay under 1000-line limit) ──
use crate::ui::rack_cables::ModuleDrag;

/// Process drag start/stop from a card response and update app.module_drag.
/// Returns true if this card was just dropped (so caller can reorder slots).
pub(super) fn handle_title_drag(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    id: u32,
    resp: &module_card::CardResponse,
    zone: crate::state::Zone,
    zone_origin: egui::Pos2,
    step: f32,
    col_w: f32,
) -> bool {
    if resp.title_dragged {
        if app.module_drag.as_ref().map(|d| d.module_id) != Some(id) {
            let (cw, rh) = app
                .state
                .read()
                .rack
                .modules
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.kind.grid_size(crate::state::GRID_COLS))
                .unwrap_or((1, 1));
            app.module_drag = Some(ModuleDrag {
                module_id: id,
                pointer: ctx.pointer_latest_pos().unwrap_or_default(),
                zone,
                col_span: cw,
                row_span: rh,
                zone_origin,
                step,
                col_w,
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

/// Snap-to-grid drop: compute the target grid cell from the pointer position
/// and move the module there if the cell is free (or swap with occupant).
pub(super) fn reorder_module_by_drop(
    app: &mut ImpulseApp,
    dragged_id: u32,
    drop_pos: egui::Pos2,
    zone: crate::state::Zone,
    zone_origin: egui::Pos2,
    step: f32,
    col_w: f32,
) {
    let (col_span, row_span) = app
        .state
        .read()
        .rack
        .modules
        .iter()
        .find(|m| m.id == dragged_id)
        .map(|m| m.kind.grid_size(crate::state::GRID_COLS))
        .unwrap_or((1, 1));

    // Compute snap target from pointer position relative to zone origin.
    let rel_x = drop_pos.x - zone_origin.x;
    let rel_y = drop_pos.y - zone_origin.y;
    let snap_col = (rel_x / step).round().max(0.0) as u8;
    let snap_row = (rel_y / step).round().max(0.0) as u8;
    let snap_col = snap_col.min(crate::state::GRID_COLS.saturating_sub(col_span));

    // Check current position — no-op if unchanged.
    let current = app
        .state
        .read()
        .rack
        .modules
        .iter()
        .find(|m| m.id == dragged_id)
        .map(|m| (m.grid_col, m.grid_row));
    if current == Some((snap_col, snap_row)) {
        return;
    }

    // Overlap check: reject drop if any other module in the same zone occupies
    // any cell in the target (snap_col..+col_span, snap_row..+row_span) block.
    let blocked = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .filter(|m| m.id != dragged_id && m.zone == zone)
            .any(|m| {
                let (mw, mh) = m.kind.grid_size(crate::state::GRID_COLS);
                // AABB overlap test
                snap_col < m.grid_col + mw
                    && m.grid_col < snap_col + col_span
                    && snap_row < m.grid_row + mh
                    && m.grid_row < snap_row + row_span
            })
    };
    if blocked {
        return; // target occupied — keep original position
    }

    app.push_history();
    if let Some(m) = app
        .state
        .write()
        .rack
        .modules
        .iter_mut()
        .find(|m| m.id == dragged_id)
    {
        m.grid_col = snap_col;
        m.grid_row = snap_row;
    }
    let _ = col_w;
}
