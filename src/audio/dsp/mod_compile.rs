// ─── audio/dsp/mod_compile.rs ─────────────────────────────────────────────────
// Cable-graph compile passes for the modulation pipeline.  Walk
// the rack's cables once per `AudioParams::from_app_state` build
// and emit:
//   * `compile_mod_routes` — flat ModRouteCopy array driving
//     `apply_mod_target` from any CV-out source.
//   * `compile_slew_params` — Slew utility-slot snapshots with
//     resolved `cv_in_buf_idx`.
//   * (future utility-kind compile passes share this file.)
//
// Lifted out of params.rs once that file hit the 1000-line cap
// after the cv_buf + utility modules landed.  No behaviour change
// — functions moved verbatim with their imports updated.

use super::lfo_target_opcode::lfo_target_to_u8;
use super::params::{
    MAX_MOD_ROUTES, MOD_BUF_CV_SEQ_BASE, MOD_BUF_LFO_BASE, MOD_BUF_SLEW_BASE, ModRouteCopy,
    SlewParamsCopy,
};
use crate::state::{AppState, LfoTarget};

/// Walk the rack's Mod cables and emit a fixed-size array of compiled mod
/// routes for the audio thread to consume.  Each route resolves the source
/// LFO module to its slot index (position in the rack's LfoModule order) and
/// the destination Mod-In jack to its `LfoTarget` (Fixed slot or the user-
/// picked Selector value).  Routes whose source/target can't be resolved or
/// whose target is `None` are silently skipped.  The depth defaults to 1.0
/// (a per-cable depth knob is a future addition).
pub fn compile_mod_routes(s: &AppState) -> ([ModRouteCopy; MAX_MOD_ROUTES], u8) {
    use crate::state::{ModInput, ModuleKind, PortKind, mod_inputs};
    let mut routes = [ModRouteCopy::default(); MAX_MOD_ROUTES];
    let mut count = 0usize;
    let lfo_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .collect();
    let cv_seq_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::CvSequencer)
        .map(|m| m.id)
        .collect();
    let slew_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::Slew)
        .map(|m| m.id)
        .collect();
    for cable in &s.rack.cables {
        if count >= MAX_MOD_ROUTES {
            break;
        }
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        // Resolve the source — LFO, CV sequencer, or utility.
        // Each kind owns a 4-slot range in `cv_buf`; resolve to a
        // flat buf index so the audio thread reads the value
        // without re-checking the source kind.
        let source_buf_idx: u8 = if let Some(slot_idx) =
            lfo_ids.iter().position(|id| *id == cable.from.module_id)
        {
            if slot_idx >= s.lfo.len() {
                continue;
            }
            (MOD_BUF_LFO_BASE + slot_idx) as u8
        } else if let Some(slot_idx) = cv_seq_ids.iter().position(|id| *id == cable.from.module_id)
        {
            if slot_idx >= crate::state::CV_SEQ_SLOTS {
                continue;
            }
            (MOD_BUF_CV_SEQ_BASE + slot_idx) as u8
        } else if let Some(slot_idx) = slew_ids.iter().position(|id| *id == cable.from.module_id) {
            if slot_idx >= crate::state::SLEW_SLOTS {
                continue;
            }
            (MOD_BUF_SLEW_BASE + slot_idx) as u8
        } else {
            continue;
        };
        let Some(target_module) = s.rack.modules.iter().find(|m| m.id == cable.to.module_id) else {
            continue;
        };
        let inputs = mod_inputs(target_module.kind);
        let depth_unipolar = target_module
            .mod_input_depths
            .get(cable.to.index as usize)
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let invert = target_module
            .mod_input_invert
            .get(cable.to.index as usize)
            .copied()
            .unwrap_or(false);
        let depth = if invert {
            -depth_unipolar
        } else {
            depth_unipolar
        };
        // Resolve the slot's effective target list — Fixed = its single
        // target, Selector = the multi-select Vec the user picked.
        let targets: &[LfoTarget] = match inputs.get(cable.to.index as usize) {
            Some(ModInput::Fixed(t)) => std::slice::from_ref(t),
            Some(ModInput::Selector) => target_module
                .mod_selectors
                .get(cable.to.index as usize)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
            None => continue,
        };
        for &t in targets {
            if t == LfoTarget::None || count >= MAX_MOD_ROUTES {
                continue;
            }
            routes[count] = ModRouteCopy {
                source_buf_idx,
                target_u8: lfo_target_to_u8(t),
                depth,
            };
            count += 1;
        }
    }
    (routes, count as u8)
}

/// Walk the rack's cables and produce the Slew utility-slot snapshot
/// (4 slots, each with audio-thread-ready params + a resolved
/// `cv_in_buf_idx`).  Each `ModuleKind::Slew` instance maps to
/// one slot in rack order; the 5th instance stacks on the last
/// slot.  Cables from any CV-out source (LFO / CvSeq / future
/// utility) into a Slew's Mod-In port are resolved here so the
/// audio thread can read the source value from `cv_buf` without
/// re-walking the cable graph.
pub fn compile_slew_params(s: &AppState) -> [SlewParamsCopy; crate::state::SLEW_SLOTS] {
    use crate::state::{ModuleKind, PortKind};
    let mut out = [SlewParamsCopy::default(); crate::state::SLEW_SLOTS];
    let lfo_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::LfoModule)
        .map(|m| m.id)
        .collect();
    let cv_seq_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::CvSequencer)
        .map(|m| m.id)
        .collect();
    let slew_ids: Vec<u32> = s
        .rack
        .modules
        .iter()
        .filter(|m| m.kind == ModuleKind::Slew)
        .map(|m| m.id)
        .collect();
    // Copy the user-set knobs into each slot first (they apply
    // even when no cable is wired — the slot still smooths the 0
    // input toward 0 with its release time).
    for (i, slot) in s.slew.iter().enumerate().take(crate::state::SLEW_SLOTS) {
        out[i] = SlewParamsCopy {
            enabled: slot.enabled,
            attack: slot.attack.clamp(0.0, 1.0),
            release: slot.release.clamp(0.0, 1.0),
            cv_in_buf_idx: u8::MAX,
        };
    }
    // Resolve the cv_in_buf_idx for each Slew slot from cables
    // landing on Slew.ModIn[0].  Last-cable-wins; the UI prevents
    // multiple cables on the same Mod-In jack so this is normally
    // a single-cable resolution.
    for cable in &s.rack.cables {
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(slew_idx) = slew_ids.iter().position(|id| *id == cable.to.module_id) else {
            continue;
        };
        if slew_idx >= crate::state::SLEW_SLOTS {
            continue;
        }
        let source_buf_idx: u8 = if let Some(slot_idx) =
            lfo_ids.iter().position(|id| *id == cable.from.module_id)
        {
            if slot_idx >= s.lfo.len() {
                continue;
            }
            (MOD_BUF_LFO_BASE + slot_idx) as u8
        } else if let Some(slot_idx) = cv_seq_ids.iter().position(|id| *id == cable.from.module_id)
        {
            if slot_idx >= crate::state::CV_SEQ_SLOTS {
                continue;
            }
            (MOD_BUF_CV_SEQ_BASE + slot_idx) as u8
        } else {
            continue;
        };
        out[slew_idx].cv_in_buf_idx = source_buf_idx;
    }
    out
}
