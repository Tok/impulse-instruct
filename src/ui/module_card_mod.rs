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
    LfoTarget::Kick808Pitch,
    LfoTarget::An1xCutoff,
    LfoTarget::An1xPitch,
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

/// Draw a floating ComboBox over each Selector mod-jack on the back panel so
/// the user can pick which parameter that slot modulates.  Iterates the
/// per-frame port list; for each Mod-In port whose slot kind is `Selector`,
/// renders an overlay at the port position and writes the choice back into
/// `RackModule.mod_selectors[index]`.
pub fn draw_mod_selector_dropdowns(app: &mut ImpulseApp, ctx: &egui::Context, ports: &[PortPos]) {
    if !app.rack_flipped {
        return;
    }
    // Snapshot the per-module kind + current selector targets so we can render
    // without holding a write lock on app.state during the ComboBox pass.
    let snapshot: Vec<(u32, ModuleKind, Vec<LfoTarget>)> = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .map(|m| (m.id, m.kind, m.mod_selectors.clone()))
            .collect()
    };
    let mut changes: Vec<(u32, usize, LfoTarget)> = Vec::new();
    for p in ports
        .iter()
        .filter(|p| p.port.dir == PortDir::In && p.port.kind == PortKind::Mod)
    {
        let Some((_, kind, selectors)) = snapshot.iter().find(|(id, _, _)| *id == p.port.module_id)
        else {
            continue;
        };
        let idx = p.port.index as usize;
        let slots = mod_inputs(*kind);
        if !matches!(slots.get(idx), Some(ModInput::Selector)) {
            continue;
        }
        let current = selectors.get(idx).copied().unwrap_or(LfoTarget::None);
        // Scope the dropdown to targets that modulate this module kind, plus
        // the None option.  Voice kinds (no matching LfoTarget variants yet)
        // fall back to ALL_TARGETS so the slot is still usable.
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
        // Anchor the dropdown just to the right of the jack, centred vertically.
        let anchor = p.center + Vec2::new(8.0, -7.0);
        egui::Area::new(egui::Id::new(("mod_sel", p.port.module_id, idx)))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                let frame = egui::Frame::none()
                    .fill(Color32::from_gray(20))
                    .inner_margin(egui::Margin::symmetric(2.0, 0.0));
                frame.show(ui, |ui| {
                    let mut sel = current;
                    egui::ComboBox::from_id_source(("mod_sel_combo", p.port.module_id, idx))
                        .selected_text(
                            egui::RichText::new(lfo_target_short_label(current))
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
                    if sel != current {
                        changes.push((p.port.module_id, idx, sel));
                    }
                });
            });
    }
    if !changes.is_empty() {
        let mut s = app.state.write();
        for (mid, idx, tgt) in changes {
            if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == mid) {
                if m.mod_selectors.len() <= idx {
                    m.mod_selectors.resize(idx + 1, LfoTarget::None);
                }
                m.mod_selectors[idx] = tgt;
            }
        }
    }
}
