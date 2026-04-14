// ─── ui/module_card_mod.rs ───────────────────────────────────────────────────
// Back-panel rendering of per-knob modulation input jacks.
//
// Every `ModuleKind` declares its mod-input slot list via
// `state::modulation::mod_inputs`.  This helper renders each declared slot as
// a `PortKind::Mod` jack below the standard AUD/CV/CTL input column.

use egui::{Color32, FontId, Pos2, Rect, Sense, Vec2};

use crate::state::{
    LfoTarget, ModInput, ModuleKind, PortDir, PortKind, PortRef, lfo_target_short_label,
    mod_input_label, mod_inputs, modulation::lfo_target_module_kind,
};
use crate::ui::ImpulseApp;
use crate::ui::module_card::{PortPos, draw_port_circle};

/// Ordered list of all LfoTarget variants — used to populate Selector-slot
/// dropdowns.  Kept in display order (voices → FX → master).
const ALL_TARGETS: &[LfoTarget] = &[
    LfoTarget::None,
    LfoTarget::BassCutoff,
    LfoTarget::BassResonance,
    LfoTarget::BassPitch,
    LfoTarget::BassVolume,
    LfoTarget::BassPan,
    LfoTarget::HooverPan,
    LfoTarget::NoisePan,
    LfoTarget::Kick808Pitch,
    LfoTarget::Kick808Pan,
    LfoTarget::Snare808Pan,
    LfoTarget::Hihat808Pan,
    LfoTarget::Kick909Pan,
    LfoTarget::Snare909Pan,
    LfoTarget::Hihat909Pan,
    LfoTarget::Clap909Pan,
    LfoTarget::An1xCutoff,
    LfoTarget::An1xPitch,
    LfoTarget::An1xPan,
    LfoTarget::ReverbMix,
    LfoTarget::ReverbSize,
    LfoTarget::ReverbDamp,
    LfoTarget::DelayTime,
    LfoTarget::DelayFeedback,
    LfoTarget::DelayMix,
    LfoTarget::ChorusRate,
    LfoTarget::ChorusDepth,
    LfoTarget::ChorusMix,
    LfoTarget::PhaserRate,
    LfoTarget::PhaserDepth,
    LfoTarget::PhaserMix,
    LfoTarget::WaveshaperDrive,
    LfoTarget::WaveshaperMix,
    LfoTarget::DistortionDrive,
    LfoTarget::DistortionMix,
    LfoTarget::BitcrushBits,
    LfoTarget::BitcrushRate,
    LfoTarget::BitcrushMix,
    LfoTarget::RingModFreq,
    LfoTarget::RingModMix,
    LfoTarget::EqLow,
    LfoTarget::EqMid,
    LfoTarget::EqHigh,
    LfoTarget::CompThresh,
    LfoTarget::CompRatio,
    LfoTarget::CompMix,
    LfoTarget::TapeDrive,
    LfoTarget::TapeMix,
    LfoTarget::TapeFlutter,
    LfoTarget::AutotuneAmount,
    LfoTarget::AutotuneMix,
    LfoTarget::MasterVolume,
];

/// Vertical gap between successive back-panel ports (matches the core card).
const PORT_SPACING: f32 = 20.0;

/// Draw Mod-in jacks for `kind` starting at `start_y` on the left (input)
/// column, appending each port to `ports`.  Returns the y-coordinate below the
/// last port (so callers can continue stacking if needed).
#[allow(clippy::too_many_arguments)]
pub fn draw_mod_input_ports(
    ui: &mut egui::Ui,
    sp: &egui::Painter,
    module_id: u32,
    kind: ModuleKind,
    left_x: f32,
    start_y: f32,
    label_font: &FontId,
    label_col: Color32,
    port_size: Vec2,
    ports: &mut Vec<PortPos>,
) -> f32 {
    let slots = mod_inputs(kind);
    let mut y = start_y;
    for (i, slot) in slots.iter().enumerate() {
        let pos = Pos2::new(left_x, y);
        draw_port_circle(sp, pos, PortKind::Mod, PortDir::In);
        ports.push(PortPos {
            port: PortRef {
                module_id,
                dir: PortDir::In,
                kind: PortKind::Mod,
                index: i as u8,
            },
            center: pos,
        });
        sp.text(
            pos + Vec2::new(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            mod_input_label(kind, i),
            label_font.clone(),
            label_col,
        );
        let tip = match slot {
            ModInput::Fixed(_) => format!("MOD IN #{} — dedicated target", i + 1),
            ModInput::Selector => format!("MOD IN #{} — target picked on back panel", i + 1),
        };
        ui.interact(
            Rect::from_center_size(pos, port_size),
            ui.id().with(("bp_mod_in", i)),
            Sense::hover(),
        )
        .on_hover_text(tip);
        y += PORT_SPACING;
    }
    y
}

/// Per-port back-panel overlays: target dropdown (Selector slots only) +
/// depth knob (every Mod-In jack).  Iterates the per-frame port list and
/// renders an `egui::Area` overlay at each Mod-In jack position.  Writes
/// changes back into `RackModule.mod_selectors` and `mod_input_depths` in a
/// single write lock at the end of the frame.
pub fn draw_mod_selector_dropdowns(app: &mut ImpulseApp, ctx: &egui::Context, ports: &[PortPos]) {
    if !app.rack_flipped {
        return;
    }
    // Snapshot module kind + current selector targets + depths under a read
    // lock so the overlay pass can render lock-free.
    let snapshot: Vec<(u32, ModuleKind, Vec<LfoTarget>, Vec<f32>)> = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .map(|m| {
                (
                    m.id,
                    m.kind,
                    m.mod_selectors.clone(),
                    m.mod_input_depths.clone(),
                )
            })
            .collect()
    };
    let mut sel_changes: Vec<(u32, usize, LfoTarget)> = Vec::new();
    let mut depth_changes: Vec<(u32, usize, f32)> = Vec::new();
    for p in ports
        .iter()
        .filter(|p| p.port.dir == PortDir::In && p.port.kind == PortKind::Mod)
    {
        let Some((_, kind, selectors, depths)) = snapshot
            .iter()
            .find(|(id, _, _, _)| *id == p.port.module_id)
        else {
            continue;
        };
        let idx = p.port.index as usize;
        let slots = mod_inputs(*kind);
        let Some(slot) = slots.get(idx) else {
            continue;
        };
        let is_selector = matches!(slot, ModInput::Selector);
        let cur_sel = selectors.get(idx).copied().unwrap_or(LfoTarget::None);
        let cur_depth = depths.get(idx).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        // Scope dropdown options to this module's targets when possible.
        let own_targets: Vec<LfoTarget> = ALL_TARGETS
            .iter()
            .copied()
            .filter(|t| lfo_target_module_kind(*t) == Some(*kind))
            .collect();
        let options: Vec<LfoTarget> = if own_targets.is_empty() {
            ALL_TARGETS.to_vec()
        } else {
            std::iter::once(LfoTarget::None)
                .chain(own_targets)
                .collect()
        };
        let anchor = p.center + Vec2::new(8.0, -7.0);
        egui::Area::new(egui::Id::new(("mod_overlay", p.port.module_id, idx)))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                let frame = egui::Frame::none()
                    .fill(Color32::from_gray(20))
                    .inner_margin(egui::Margin::symmetric(2.0, 0.0));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if is_selector {
                            let mut sel = cur_sel;
                            egui::ComboBox::from_id_source((
                                "mod_sel_combo",
                                p.port.module_id,
                                idx,
                            ))
                            .selected_text(
                                egui::RichText::new(lfo_target_short_label(cur_sel))
                                    .monospace()
                                    .size(7.5)
                                    .color(Color32::from_gray(170)),
                            )
                            .width(52.0)
                            .show_ui(ui, |ui| {
                                for t in &options {
                                    ui.selectable_value(
                                        &mut sel,
                                        *t,
                                        egui::RichText::new(lfo_target_short_label(*t))
                                            .monospace()
                                            .size(8.0),
                                    );
                                }
                            });
                            if sel != cur_sel {
                                sel_changes.push((p.port.module_id, idx, sel));
                            }
                        }
                        // Depth: 0..100 % drag value (compact).  Stored as 0..1.
                        let mut pct = (cur_depth * 100.0).round() as i32;
                        let resp = ui.add(
                            egui::DragValue::new(&mut pct)
                                .range(0..=100)
                                .speed(1.0)
                                .suffix("%")
                                .custom_formatter(|n, _| format!("{:>3}%", n as i32)),
                        );
                        if resp.changed() {
                            let new_d = (pct as f32 / 100.0).clamp(0.0, 1.0);
                            if (new_d - cur_depth).abs() > f32::EPSILON {
                                depth_changes.push((p.port.module_id, idx, new_d));
                            }
                        }
                    });
                });
            });
    }
    if !sel_changes.is_empty() || !depth_changes.is_empty() {
        let mut s = app.state.write();
        for (mid, idx, tgt) in sel_changes {
            if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == mid) {
                if m.mod_selectors.len() <= idx {
                    m.mod_selectors.resize(idx + 1, LfoTarget::None);
                }
                m.mod_selectors[idx] = tgt;
            }
        }
        for (mid, idx, d) in depth_changes {
            if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == mid) {
                if m.mod_input_depths.len() <= idx {
                    m.mod_input_depths.resize(idx + 1, 1.0);
                }
                m.mod_input_depths[idx] = d;
            }
        }
    }
}
