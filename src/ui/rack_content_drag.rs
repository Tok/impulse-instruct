// ─── ui/rack_content_drag.rs ─────────────────────────────────────────────────
// Cable-patching + module-drag interaction helpers extracted from
// rack_content.rs to keep that file under the 1000-line cap.  Three
// entry points:
//
//   • `handle_cable_drag`  — primary-drag to patch ports, right-click
//     to disconnect.
//   • `handle_title_drag`  — track title-bar drag state on a card.
//   • `reorder_module_by_drop` — snap-to-grid module placement.

use crate::ui::ImpulseApp;
use crate::ui::module_card;
use crate::ui::rack_cables::ModuleDrag;

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
            let (cw, rh) = {
                let s = app.state.read();
                s.rack
                    .modules
                    .iter()
                    .find(|m| m.id == id)
                    .map(|m| s.rack.effective_grid_size(m))
                    .unwrap_or((1, 1))
            };
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
    let (col_span, row_span) = {
        let s = app.state.read();
        s.rack
            .modules
            .iter()
            .find(|m| m.id == dragged_id)
            .map(|m| s.rack.effective_grid_size(m))
            .unwrap_or((1, 1))
    };

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
                let (mw, mh) = s.rack.effective_grid_size(m);
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
