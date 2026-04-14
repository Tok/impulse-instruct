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

/// Vertical gap between successive back-panel ports — large enough that the
/// per-jack mod-overlay (slider + chip row) doesn't overlap its neighbours.
const PORT_SPACING: f32 = 22.0;

/// Small labelled toggle chip — selected = light fill + bright text,
/// unselected = dim fill + dim text.  Used in mod-target multi-select.
fn chip_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = egui::RichText::new(label)
        .monospace()
        .size(7.5)
        .color(if selected {
            Color32::from_gray(235)
        } else {
            Color32::from_gray(140)
        });
    ui.add(
        egui::Button::new(text)
            .small()
            .fill(if selected {
                Color32::from_gray(60)
            } else {
                Color32::from_gray(28)
            })
            .stroke(egui::Stroke::NONE)
            .rounding(2.0),
    )
}

/// True if `kind` exposes an audio-in jack on the back panel.
pub fn has_audio_in(kind: ModuleKind) -> bool {
    use ModuleKind::*;
    matches!(
        kind,
        FxReverb
            | FxDelay
            | FxChorus
            | FxPhaser
            | FxRingMod
            | FxWaveshaper
            | FxBitcrush
            | FxEq
            | FxCompressor
            | FxTapeSat
            | FxDrive
            | FxAutotune
            | MasterOutput
    )
}

/// True if `kind` exposes a CV-in (gate/pitch) jack — voices.
pub fn has_cv_in(kind: ModuleKind) -> bool {
    use ModuleKind::*;
    matches!(
        kind,
        AcidBass
            | DrumKit808
            | DrumKit909
            | HooverLead
            | An1xVoice
            | AmenSampler
            | NoiseVoice
            | NeuTts
    )
}

/// True if `kind` accepts a Control-in cable from an LLM agent.
pub fn has_control_in(kind: ModuleKind) -> bool {
    use ModuleKind::*;
    !matches!(kind, MasterOutput | LlmAgent | LlmConsole)
}

/// Computed back-panel strip height — AUD/CV/CTL share a single horizontal
/// top row (~30 px), then the mod-input jacks stack vertically below.
pub fn back_strip_height(kind: ModuleKind) -> f32 {
    let mods = mod_inputs(kind).len();
    (32.0 + mods as f32 * PORT_SPACING).max(48.0)
}

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
    // Skip overlays whose anchor would land in (or below) the bottom-panel
    // strip — the piano + footer always stay on top regardless of egui
    // layer order.  Approximate piano+footer reserved height = 105 px.
    let screen_bottom = ctx.screen_rect().max.y;
    let max_overlay_y = screen_bottom - 105.0;
    // Snapshot module kind + current per-slot multi-select target lists +
    // depths under a read lock so the overlay pass can render lock-free.
    type Snap = (u32, ModuleKind, Vec<Vec<LfoTarget>>, Vec<f32>, Vec<bool>);
    let snapshot: Vec<Snap> = {
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
                    m.mod_input_invert.clone(),
                )
            })
            .collect()
    };
    // Pending changes — applied in a single write lock at end of frame.
    let mut sel_changes: Vec<(u32, usize, Vec<LfoTarget>)> = Vec::new();
    let mut depth_changes: Vec<(u32, usize, f32)> = Vec::new();
    let mut invert_changes: Vec<(u32, usize, bool)> = Vec::new();
    for p in ports
        .iter()
        .filter(|p| p.port.dir == PortDir::In && p.port.kind == PortKind::Mod)
    {
        let Some((_, kind, selectors, depths, inverts)) = snapshot
            .iter()
            .find(|(id, _, _, _, _)| *id == p.port.module_id)
        else {
            continue;
        };
        let idx = p.port.index as usize;
        let slots = mod_inputs(*kind);
        let Some(slot) = slots.get(idx) else {
            continue;
        };
        let is_selector = matches!(slot, ModInput::Selector);
        let cur_targets: Vec<LfoTarget> = selectors.get(idx).cloned().unwrap_or_default();
        let cur_depth = depths.get(idx).copied().unwrap_or(1.0).clamp(0.0, 1.0);
        let cur_invert = inverts.get(idx).copied().unwrap_or(false);
        // Scope chips to this module's own LfoTargets.  "—" is a meta-chip:
        // selected when ALL real targets are active; clicking it toggles all
        // on (or all off if everything is currently active).
        let real_targets: Vec<LfoTarget> = ALL_TARGETS
            .iter()
            .copied()
            .filter(|t| lfo_target_module_kind(*t) == Some(*kind))
            .collect();
        let all_selected =
            !real_targets.is_empty() && real_targets.iter().all(|t| cur_targets.contains(t));
        let anchor = p.center + Vec2::new(10.0, -8.0);
        if anchor.y > max_overlay_y {
            // Jack is below the visible rack region — skip its overlay so
            // the piano panel below isn't covered by the floating Area.
            continue;
        }
        egui::Area::new(egui::Id::new(("mod_overlay", p.port.module_id, idx)))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                let frame = egui::Frame::none()
                    .fill(Color32::from_gray(18))
                    .stroke(egui::Stroke::new(0.5, Color32::from_gray(50)))
                    .rounding(2.0)
                    .inner_margin(egui::Margin::symmetric(3.0, 1.0));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);
                        // Polarity toggle — `+` for normal, `−` for inverted.
                        // Shown FIRST so the slider's % visually matches the
                        // sign next to it.
                        let pol_label = if cur_invert { "−" } else { "+" };
                        if chip_button(ui, pol_label, cur_invert).clicked() {
                            invert_changes.push((p.port.module_id, idx, !cur_invert));
                        }
                        let mut d = cur_depth;
                        ui.spacing_mut().slider_width = 50.0;
                        ui.add(egui::Slider::new(&mut d, 0.0..=1.0).show_value(false))
                            .on_hover_text(format!(
                                "Depth {}{:.0}%",
                                if cur_invert { "-" } else { "+" },
                                cur_depth * 100.0
                            ));
                        ui.label(
                            egui::RichText::new(format!(
                                "{}{:>3}%",
                                if cur_invert { "-" } else { " " },
                                (d * 100.0).round() as i32
                            ))
                            .monospace()
                            .size(7.5)
                            .color(Color32::from_gray(170)),
                        );
                        if d != cur_depth {
                            depth_changes.push((p.port.module_id, idx, d));
                        }
                        if is_selector {
                            // "—" / ALL meta-chip
                            if chip_button(ui, "—", all_selected).clicked() {
                                let new_targets = if all_selected {
                                    Vec::new()
                                } else {
                                    real_targets.clone()
                                };
                                sel_changes.push((p.port.module_id, idx, new_targets));
                            }
                            // Per-target toggle chips (multi-select)
                            for t in &real_targets {
                                let active = cur_targets.contains(t);
                                if chip_button(ui, lfo_target_short_label(*t), active).clicked() {
                                    let mut next = cur_targets.clone();
                                    if active {
                                        next.retain(|x| x != t);
                                    } else {
                                        next.push(*t);
                                    }
                                    sel_changes.push((p.port.module_id, idx, next));
                                }
                            }
                        }
                    });
                });
            });
    }
    if !sel_changes.is_empty() || !depth_changes.is_empty() || !invert_changes.is_empty() {
        let mut s = app.state.write();
        for (mid, idx, targets) in sel_changes {
            if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == mid) {
                if m.mod_selectors.len() <= idx {
                    m.mod_selectors.resize(idx + 1, Vec::new());
                }
                m.mod_selectors[idx] = targets;
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
        for (mid, idx, inv) in invert_changes {
            if let Some(m) = s.rack.modules.iter_mut().find(|m| m.id == mid) {
                if m.mod_input_invert.len() <= idx {
                    m.mod_input_invert.resize(idx + 1, false);
                }
                m.mod_input_invert[idx] = inv;
            }
        }
    }
}
