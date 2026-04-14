// ─── ui/module_card_mod.rs ───────────────────────────────────────────────────
// Back-panel rendering of per-knob modulation input jacks.
//
// Every `ModuleKind` declares its mod-input slot list via
// `state::modulation::mod_inputs`.  This helper renders each declared slot as
// a `PortKind::Mod` jack below the standard AUD/CV/CTL input column.

use egui::{Color32, FontId, Pos2, Rect, Sense, Vec2};

use crate::state::{ModInput, ModuleKind, PortDir, PortKind, PortRef, mod_input_label, mod_inputs};
use crate::ui::module_card::{PortPos, draw_port_circle};

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
