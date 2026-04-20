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
use crate::ui::theme;

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
    LfoTarget::Kick808Decay,
    LfoTarget::Kick808Pan,
    LfoTarget::Snare808Tone,
    LfoTarget::Snare808Decay,
    LfoTarget::Snare808Pan,
    LfoTarget::Hihat808Pan,
    LfoTarget::Kick909Pitch,
    LfoTarget::Kick909Decay,
    LfoTarget::Kick909Pan,
    LfoTarget::Snare909Tone,
    LfoTarget::Snare909Decay,
    LfoTarget::Snare909Pan,
    LfoTarget::Hihat909Pan,
    LfoTarget::Clap909Decay,
    LfoTarget::Clap909Pan,
    LfoTarget::An1xCutoff,
    LfoTarget::An1xPitch,
    LfoTarget::An1xPan,
    LfoTarget::AmenVolume,
    LfoTarget::AmenStart,
    LfoTarget::AmenGate,
    LfoTarget::GranularVolume,
    LfoTarget::GranularDensity,
    LfoTarget::GranularGrain,
    LfoTarget::GranularPos,
    LfoTarget::NeuTtsVolume,
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
    LfoTarget::GabberKickPitch,
    LfoTarget::GabberKickDecay,
    LfoTarget::GabberKickClip,
    LfoTarget::GabberKickPan,
    LfoTarget::MasterVolume,
    LfoTarget::StereoWidth,
];

/// Vertical gap between successive back-panel ports.
/// - Fixed jacks: tight (no chip strip).
/// - Selector jacks: taller (chip strip wraps under the slider).
/// - Compact mode: 1-cell-tall cards (NoiseVoice, most FX) can't afford
///   the tall selector spacing, so selectors collapse their chip strip
///   inline onto the slider row and stack at the same tight pitch as
///   Fixed jacks — otherwise a 3-jack FX overlay overflows its card.
const PORT_SPACING_FIXED: f32 = 24.0;
const PORT_SPACING_SELECTOR: f32 = 42.0;
const PORT_SPACING_COMPACT: f32 = 24.0;

/// True if the module kind has room for the 2-row selector overlay.
/// Currently keyed off grid height: anything taller than one grid row
/// can fit the stacked chip strip; one-row cards use compact mode.
pub(crate) fn back_is_compact(kind: ModuleKind) -> bool {
    kind.grid_size(crate::state::rack::GRID_COLS).1 <= 1
}

/// Vertical spacing for a given slot kind, given the card's compact flag.
fn slot_spacing(slot: &crate::state::modulation::ModInput, compact: bool) -> f32 {
    match slot {
        crate::state::modulation::ModInput::Fixed(_) => PORT_SPACING_FIXED,
        crate::state::modulation::ModInput::Selector if compact => PORT_SPACING_COMPACT,
        crate::state::modulation::ModInput::Selector => PORT_SPACING_SELECTOR,
    }
}

/// Chip-text font size (px) — shrunk slightly from the previous 7.5 so the
/// per-target chips fit more per row inside a typical card width.
const MOD_OVERLAY_CHIP_FONT_PX: f32 = 6.5;

/// Small labelled toggle chip — selected = light fill + bright text,
/// unselected = dim fill + dim text.  Used in mod-target multi-select.
fn chip_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = egui::RichText::new(label)
        .monospace()
        .size(MOD_OVERLAY_CHIP_FONT_PX)
        .color(if selected {
            Color32::from_gray(235)
        } else {
            Color32::from_gray(140)
        });
    ui.scope(|ui| {
        // Tighter padding than the default — chips need to pack densely.
        ui.spacing_mut().button_padding = egui::vec2(2.0, 0.5);
        ui.add(
            egui::Button::new(text)
                .small()
                .fill(if selected { theme::IRON } else { theme::PIT })
                .stroke(egui::Stroke::NONE)
                .rounding(2.0),
        )
    })
    .inner
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
/// top row (~32 px), then the mod-input jacks stack vertically below
/// using `slot_spacing` for each slot (Fixed = tight, Selector = taller,
/// Selector-on-1-cell-card = tight with inline chips).
pub fn back_strip_height(kind: ModuleKind) -> f32 {
    let compact = back_is_compact(kind);
    let stack: f32 = mod_inputs(kind)
        .iter()
        .map(|s| slot_spacing(s, compact))
        .sum();
    (32.0 + stack).max(40.0)
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
        y += slot_spacing(slot, back_is_compact(kind));
    }
    y
}

/// Per-port back-panel overlays: target dropdown (Selector slots only) +
/// depth knob (every Mod-In jack).  Iterates the per-frame port list and
/// renders an `egui::Area` overlay at each Mod-In jack position.  Writes
/// changes back into `RackModule.mod_selectors` and `mod_input_depths` in a
/// single write lock at the end of the frame.
pub fn draw_mod_selector_dropdowns(
    app: &mut ImpulseApp,
    ctx: &egui::Context,
    ports: &[PortPos],
    canvas_rect: egui::Rect,
) {
    if !app.rack_flipped {
        return;
    }
    // Skip overlays whose extent would land in (or below) the bottom-panel
    // strip — the piano + footer always stay on top of egui Areas because
    // panels live at a higher LayerId.  The earlier 105 px cutoff used
    // only the anchor; the overlay itself extends ~50–70 px below it (one
    // slider row + up to two wrapped chip rows), so we need a fatter
    // buffer to avoid the bottom-panel collision the user reported.
    let screen_bottom = ctx.screen_rect().max.y;
    let bottom_panel_h = 105.0_f32;
    let overlay_est_h_max = 70.0_f32;
    let max_overlay_y = screen_bottom - bottom_panel_h - overlay_est_h_max;
    // Same idea for the top edge: when the rack scrolls and a back-panel
    // jack ends up above the canvas, its Foreground Area would otherwise
    // paint over the header info panel and the prompt strip.  Using the
    // canvas top as the cutoff matches what the cable overlay clips to.
    let min_overlay_y = canvas_rect.min.y;
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
        let compact = back_is_compact(*kind);
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
        // Anchor on the SAME ROW as the jack/label.  The label "MOD1"
        // (etc.) sits at jack.x + 10 and is ~26 px wide, so we offset
        // 36 px right of the jack centre to clear it, and 8 px up to
        // vertically centre the slider/chips around the jack y.
        let anchor = p.center + Vec2::new(36.0, -8.0);
        if anchor.y > max_overlay_y {
            // Jack is below the visible rack region — skip its overlay so
            // the piano panel below isn't covered by the floating Area.
            continue;
        }
        if anchor.y < min_overlay_y {
            // Jack is scrolled above the canvas — skip so the floating
            // Area doesn't paint over the header/prompt strip above.
            continue;
        }
        // Use the actual card width as the wrap budget.  Jack sits 16 px
        // in from the card's left edge and the overlay anchors 36 px
        // further right (past the jack label) — so usable horizontal
        // budget = card_w - 16 - 36 - 4 (right margin).
        let card_w = ctx
            .data(|d| d.get_temp::<f32>(egui::Id::new("back_card_w").with(p.port.module_id)))
            .unwrap_or(180.0);
        let overlay_max_w = (card_w - 56.0).max(110.0);
        let chip_strip_w = (overlay_max_w - 8.0).max(90.0);
        egui::Area::new(egui::Id::new(("mod_overlay", p.port.module_id, idx)))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ctx, |ui| {
                ui.set_max_width(overlay_max_w);
                let frame = egui::Frame::none()
                    .fill(theme::DEEP)
                    .stroke(egui::Stroke::new(0.5, Color32::from_gray(50)))
                    .rounding(2.0)
                    .inner_margin(egui::Margin::symmetric(3.0, 1.0));
                frame.show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 1.0);
                    // First row: polarity · depth slider · % readout
                    // (+ inlined chip strip when the card is compact so
                    // 1-cell-tall modules don't overflow their strip).
                    ui.horizontal_wrapped(|ui| {
                        let pol_label = if cur_invert { "−" } else { "+" };
                        if chip_button(ui, pol_label, cur_invert).clicked() {
                            invert_changes.push((p.port.module_id, idx, !cur_invert));
                        }
                        // polarity ~14, pct readout ~30, frame padding 6,
                        // item spacing 3*2 — leaves the rest for the slider.
                        // In compact mode the slider shrinks further so the
                        // chip row fits on the same line; otherwise it can
                        // stretch up to 140 px on wider cards.
                        let slider_w = if compact {
                            (overlay_max_w - 14.0 - 30.0 - 12.0).clamp(14.0, 40.0)
                        } else {
                            (overlay_max_w - 14.0 - 30.0 - 12.0).clamp(20.0, 140.0)
                        };
                        let mut d = cur_depth;
                        // Aggressively shrink the slider's interact size,
                        // padding, and item spacing so the handle + track
                        // are noticeably smaller than the default and the
                        // depth row stays single-line even on the
                        // narrowest 2-cell-wide FX cards.
                        let style = ui.style_mut();
                        style.spacing.slider_width = slider_w;
                        style.spacing.interact_size.y = 8.0;
                        style.spacing.button_padding = egui::vec2(2.0, 0.0);
                        style.spacing.item_spacing = egui::vec2(2.0, 0.0);
                        ui.add(egui::Slider::new(&mut d, 0.0..=1.0).show_value(false))
                            .on_hover_text(format!(
                                "Depth {}{:.0}%",
                                if cur_invert { "-" } else { "+" },
                                cur_depth * 100.0
                            ));
                        // Allocate a fixed-size slot matching the slider's
                        // 12-px row height so `centered_and_justified` can
                        // vertically centre the % readout inside it — the
                        // outer `horizontal_wrapped` is Align::TOP which
                        // would otherwise float the small label at the top
                        // of the tall slider row.
                        ui.allocate_ui_with_layout(
                            egui::vec2(28.0, 12.0),
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}{:>3}%",
                                        if cur_invert { "-" } else { " " },
                                        (d * 100.0).round() as i32
                                    ))
                                    .monospace()
                                    .size(7.0)
                                    .color(Color32::from_gray(170)),
                                );
                            },
                        );
                        if d != cur_depth {
                            depth_changes.push((p.port.module_id, idx, d));
                        }
                        // Compact + Selector: inline chip strip right after
                        // the % readout so the overlay stays single-row.
                        if compact && is_selector {
                            ui.add_space(2.0);
                            if chip_button(ui, "—", all_selected).clicked() {
                                let new_targets = if all_selected {
                                    Vec::new()
                                } else {
                                    real_targets.clone()
                                };
                                sel_changes.push((p.port.module_id, idx, new_targets));
                            }
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
                    if is_selector && !compact {
                        // Second (+) row: ALL meta-chip + per-target chips.
                        // Wrapped so wide kits split into multiple lines
                        // instead of running off the side of the card.
                        ui.scope(|ui| {
                            ui.set_max_width(chip_strip_w);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
                                if chip_button(ui, "—", all_selected).clicked() {
                                    let new_targets = if all_selected {
                                        Vec::new()
                                    } else {
                                        real_targets.clone()
                                    };
                                    sel_changes.push((p.port.module_id, idx, new_targets));
                                }
                                for t in &real_targets {
                                    let active = cur_targets.contains(t);
                                    if chip_button(ui, lfo_target_short_label(*t), active).clicked()
                                    {
                                        let mut next = cur_targets.clone();
                                        if active {
                                            next.retain(|x| x != t);
                                        } else {
                                            next.push(*t);
                                        }
                                        sel_changes.push((p.port.module_id, idx, next));
                                    }
                                }
                            });
                        });
                    }
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
