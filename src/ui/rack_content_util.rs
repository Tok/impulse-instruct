// ─── ui/rack_content_util.rs ─────────────────────────────────────────────────
// Per-instance content dispatchers for the newer CV utility modules
// (TriggerDiv, LogicGate, FunctionGen).  Lifted out of `rack_content.rs`
// once that file crossed the 1000-line cap during the FunctionGen
// ship.  Same pattern as the existing utility content fns
// (Slew / Quantizer / Comparator / SampleHold / Math) over there:
// look up the rack-order slot index for `module_id` filtered by
// kind, then delegate to the panel's `draw_<util>(app, ui, slot)`.
//
// The pre-existing utility content dispatchers in `rack_content.rs`
// stay where they are; only the newer ones live here so the diff for
// future CV utility ships keeps landing in this file rather than
// growing the cap-bound original.

use crate::state::ModuleKind;
use crate::ui::ImpulseApp;

pub(super) fn draw_trigger_div_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = slot_index_for(app, ModuleKind::TriggerDiv, module_id);
    crate::ui::panels::draw_trigger_div(app, ui, slot);
}

pub(super) fn draw_logic_gate_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = slot_index_for(app, ModuleKind::LogicGate, module_id);
    crate::ui::panels::draw_logic_gate(app, ui, slot);
}

pub(super) fn draw_function_gen_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = slot_index_for(app, ModuleKind::FunctionGen, module_id);
    crate::ui::panels::draw_function_gen(app, ui, slot);
}

pub(super) fn draw_crossfader_content(app: &mut ImpulseApp, ui: &mut egui::Ui, module_id: u32) {
    let slot = slot_index_for(app, ModuleKind::Crossfader, module_id);
    crate::ui::panels::draw_crossfader(app, ui, slot);
}

/// Resolve the rack-order slot index for `module_id` among modules of
/// `kind`.  Returns 0 if the module isn't found — the same fallback
/// the original utility content dispatchers use, since the index
/// only matters when the module actually exists in the rack.
fn slot_index_for(app: &ImpulseApp, kind: ModuleKind, module_id: u32) -> usize {
    let rack = app.state.read();
    rack.rack
        .modules
        .iter()
        .filter(|m| m.kind == kind)
        .enumerate()
        .find(|(_, m)| m.id == module_id)
        .map(|(i, _)| i)
        .unwrap_or(0)
}
