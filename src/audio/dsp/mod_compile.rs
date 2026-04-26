// ─── audio/dsp/mod_compile.rs ─────────────────────────────────────────────────
// Cable-graph compile passes for the modulation pipeline.  Walk
// the rack's cables once per `AudioParams::from_app_state` build
// and emit:
//   * `compile_mod_routes` — flat ModRouteCopy array driving
//     `apply_mod_target` from any CV-out source.
//   * `compile_slew_params` / `compile_quantizer_params` /
//     (future utility-kind compile passes) — utility-slot
//     snapshots with resolved `cv_in_buf_idx`.
//
// The shared `CvSourceMaps` walks the rack once and stashes the
// id lists for every CV-emitting module kind so per-utility
// compile passes don't re-walk the rack four times each.

use super::lfo_target_opcode::lfo_target_to_u8;
use super::params::{
    ComparatorParamsCopy, MAX_MOD_ROUTES, MOD_BUF_COMPARATOR_BASE, MOD_BUF_CV_SEQ_BASE,
    MOD_BUF_LFO_BASE, MOD_BUF_MATH_BASE, MOD_BUF_QUANTIZER_BASE, MOD_BUF_SAMPLE_HOLD_BASE,
    MOD_BUF_SLEW_BASE, MathParamsCopy, ModRouteCopy, QuantizerParamsCopy, SampleHoldParamsCopy,
    SlewParamsCopy,
};
use crate::state::{AppState, LfoTarget, ModuleKind, PortKind};

/// Per-kind id lists for every CV-emitting module.  Walks the
/// rack once; downstream resolves use the cached vecs to map a
/// cable's source module id to its `cv_buf` slot.
struct CvSourceMaps {
    lfo: Vec<u32>,
    cv_seq: Vec<u32>,
    slew: Vec<u32>,
    quantizer: Vec<u32>,
    comparator: Vec<u32>,
    sample_hold: Vec<u32>,
    math: Vec<u32>,
}

impl CvSourceMaps {
    fn build(s: &AppState) -> Self {
        let mut lfo = Vec::new();
        let mut cv_seq = Vec::new();
        let mut slew = Vec::new();
        let mut quantizer = Vec::new();
        let mut comparator = Vec::new();
        let mut sample_hold = Vec::new();
        let mut math = Vec::new();
        for m in &s.rack.modules {
            match m.kind {
                ModuleKind::LfoModule => lfo.push(m.id),
                ModuleKind::CvSequencer => cv_seq.push(m.id),
                ModuleKind::Slew => slew.push(m.id),
                ModuleKind::Quantizer => quantizer.push(m.id),
                ModuleKind::Comparator => comparator.push(m.id),
                ModuleKind::SampleHold => sample_hold.push(m.id),
                ModuleKind::Math => math.push(m.id),
                _ => {}
            }
        }
        Self {
            lfo,
            cv_seq,
            slew,
            quantizer,
            comparator,
            sample_hold,
            math,
        }
    }

    /// Resolve a cable's source module id to a `cv_buf` index.
    /// Returns `None` if the source isn't a recognised CV
    /// emitter or its slot index is out of range for that kind.
    fn resolve(&self, s: &AppState, src_module_id: u32) -> Option<u8> {
        if let Some(idx) = self.lfo.iter().position(|id| *id == src_module_id) {
            if idx >= s.lfo.len() {
                return None;
            }
            return Some((MOD_BUF_LFO_BASE + idx) as u8);
        }
        if let Some(idx) = self.cv_seq.iter().position(|id| *id == src_module_id) {
            if idx >= crate::state::CV_SEQ_SLOTS {
                return None;
            }
            return Some((MOD_BUF_CV_SEQ_BASE + idx) as u8);
        }
        if let Some(idx) = self.slew.iter().position(|id| *id == src_module_id) {
            if idx >= crate::state::SLEW_SLOTS {
                return None;
            }
            return Some((MOD_BUF_SLEW_BASE + idx) as u8);
        }
        if let Some(idx) = self.quantizer.iter().position(|id| *id == src_module_id) {
            if idx >= crate::state::QUANTIZER_SLOTS {
                return None;
            }
            return Some((MOD_BUF_QUANTIZER_BASE + idx) as u8);
        }
        if let Some(idx) = self.comparator.iter().position(|id| *id == src_module_id) {
            if idx >= crate::state::COMPARATOR_SLOTS {
                return None;
            }
            return Some((MOD_BUF_COMPARATOR_BASE + idx) as u8);
        }
        if let Some(idx) = self.sample_hold.iter().position(|id| *id == src_module_id) {
            if idx >= crate::state::SAMPLE_HOLD_SLOTS {
                return None;
            }
            return Some((MOD_BUF_SAMPLE_HOLD_BASE + idx) as u8);
        }
        if let Some(idx) = self.math.iter().position(|id| *id == src_module_id) {
            if idx >= crate::state::MATH_SLOTS {
                return None;
            }
            return Some((MOD_BUF_MATH_BASE + idx) as u8);
        }
        None
    }
}

/// Walk the rack's Mod cables and emit a fixed-size array of
/// compiled mod routes for the audio thread to consume.  Every
/// route resolves the source CV emitter (LFO, CV sequencer, or
/// any utility module) to a `cv_buf` index, then walks the
/// destination Mod-In's `LfoTarget` list and emits one route
/// per target.  Routes whose source/target can't be resolved or
/// whose target is `None` are silently skipped.
pub fn compile_mod_routes(s: &AppState) -> ([ModRouteCopy; MAX_MOD_ROUTES], u8) {
    use crate::state::{ModInput, mod_inputs};
    let mut routes = [ModRouteCopy::default(); MAX_MOD_ROUTES];
    let mut count = 0usize;
    let maps = CvSourceMaps::build(s);
    for cable in &s.rack.cables {
        if count >= MAX_MOD_ROUTES {
            break;
        }
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(source_buf_idx) = maps.resolve(s, cable.from.module_id) else {
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

/// Walk the rack's cables and produce the Slew utility-slot
/// snapshot.  Each `ModuleKind::Slew` instance maps to one slot
/// in rack order; cables from any CV-out source into the
/// instance's Mod-In port resolve the slot's `cv_in_buf_idx`.
pub fn compile_slew_params(s: &AppState) -> [SlewParamsCopy; crate::state::SLEW_SLOTS] {
    let mut out = [SlewParamsCopy::default(); crate::state::SLEW_SLOTS];
    let maps = CvSourceMaps::build(s);
    let slew_ids = &maps.slew;
    for (i, slot) in s.slew.iter().enumerate().take(crate::state::SLEW_SLOTS) {
        out[i] = SlewParamsCopy {
            enabled: slot.enabled,
            attack: slot.attack.clamp(0.0, 1.0),
            release: slot.release.clamp(0.0, 1.0),
            cv_in_buf_idx: u8::MAX,
        };
    }
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
        if let Some(buf_idx) = maps.resolve(s, cable.from.module_id) {
            out[slew_idx].cv_in_buf_idx = buf_idx;
        }
    }
    out
}

/// Walk the rack's cables and produce the Quantizer utility-slot
/// snapshot.  Same per-instance slot mapping as `compile_slew_params`;
/// resolves the `cv_in_buf_idx` for any cable landing on a
/// Quantizer's Mod-In port.
pub fn compile_quantizer_params(
    s: &AppState,
) -> [QuantizerParamsCopy; crate::state::QUANTIZER_SLOTS] {
    let mut out = [QuantizerParamsCopy::default(); crate::state::QUANTIZER_SLOTS];
    let maps = CvSourceMaps::build(s);
    let quantizer_ids = &maps.quantizer;
    for (i, slot) in s
        .quantizer
        .iter()
        .enumerate()
        .take(crate::state::QUANTIZER_SLOTS)
    {
        out[i] = QuantizerParamsCopy {
            enabled: slot.enabled,
            root: slot.root.min(11),
            scale: slot.scale,
            cv_in_buf_idx: u8::MAX,
        };
    }
    for cable in &s.rack.cables {
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(qidx) = quantizer_ids
            .iter()
            .position(|id| *id == cable.to.module_id)
        else {
            continue;
        };
        if qidx >= crate::state::QUANTIZER_SLOTS {
            continue;
        }
        if let Some(buf_idx) = maps.resolve(s, cable.from.module_id) {
            out[qidx].cv_in_buf_idx = buf_idx;
        }
    }
    out
}

/// Walk the rack's cables and produce the Comparator utility-slot
/// snapshot.
pub fn compile_comparator_params(
    s: &AppState,
) -> [ComparatorParamsCopy; crate::state::COMPARATOR_SLOTS] {
    let mut out = [ComparatorParamsCopy::default(); crate::state::COMPARATOR_SLOTS];
    let maps = CvSourceMaps::build(s);
    let ids = &maps.comparator;
    for (i, slot) in s
        .comparator
        .iter()
        .enumerate()
        .take(crate::state::COMPARATOR_SLOTS)
    {
        out[i] = ComparatorParamsCopy {
            enabled: slot.enabled,
            threshold: slot.threshold.clamp(-1.0, 1.5),
            cv_in_buf_idx: u8::MAX,
        };
    }
    for cable in &s.rack.cables {
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(idx) = ids.iter().position(|id| *id == cable.to.module_id) else {
            continue;
        };
        if idx >= crate::state::COMPARATOR_SLOTS {
            continue;
        }
        if let Some(buf_idx) = maps.resolve(s, cable.from.module_id) {
            out[idx].cv_in_buf_idx = buf_idx;
        }
    }
    out
}

/// Walk the rack's cables and produce the Sample-and-hold
/// utility-slot snapshot.
pub fn compile_sample_hold_params(
    s: &AppState,
) -> [SampleHoldParamsCopy; crate::state::SAMPLE_HOLD_SLOTS] {
    let mut out = [SampleHoldParamsCopy::default(); crate::state::SAMPLE_HOLD_SLOTS];
    let maps = CvSourceMaps::build(s);
    let ids = &maps.sample_hold;
    for (i, slot) in s
        .sample_hold
        .iter()
        .enumerate()
        .take(crate::state::SAMPLE_HOLD_SLOTS)
    {
        out[i] = SampleHoldParamsCopy {
            enabled: slot.enabled,
            cv_in_buf_idx: u8::MAX,
        };
    }
    for cable in &s.rack.cables {
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(idx) = ids.iter().position(|id| *id == cable.to.module_id) else {
            continue;
        };
        if idx >= crate::state::SAMPLE_HOLD_SLOTS {
            continue;
        }
        if let Some(buf_idx) = maps.resolve(s, cable.from.module_id) {
            out[idx].cv_in_buf_idx = buf_idx;
        }
    }
    out
}

/// Walk the rack's cables and produce the Math utility-slot
/// snapshot.  The Math module exposes TWO Mod-In ports
/// (index 0 = A, index 1 = B); each cable's `to.index` selects
/// which input the source feeds.
pub fn compile_math_params(s: &AppState) -> [MathParamsCopy; crate::state::MATH_SLOTS] {
    let mut out = [MathParamsCopy::default(); crate::state::MATH_SLOTS];
    let maps = CvSourceMaps::build(s);
    let ids = &maps.math;
    for (i, slot) in s.math.iter().enumerate().take(crate::state::MATH_SLOTS) {
        out[i] = MathParamsCopy {
            enabled: slot.enabled,
            op: slot.op,
            blend: slot.blend.clamp(0.0, 1.0),
            cv_in_a_buf_idx: u8::MAX,
            cv_in_b_buf_idx: u8::MAX,
        };
    }
    for cable in &s.rack.cables {
        if cable.from.kind != PortKind::Cv || cable.to.kind != PortKind::Mod {
            continue;
        }
        let Some(idx) = ids.iter().position(|id| *id == cable.to.module_id) else {
            continue;
        };
        if idx >= crate::state::MATH_SLOTS {
            continue;
        }
        let Some(buf_idx) = maps.resolve(s, cable.from.module_id) else {
            continue;
        };
        // to.index picks which input port (A or B) this cable
        // feeds.  Anything > 1 is silently ignored for now.
        match cable.to.index {
            0 => out[idx].cv_in_a_buf_idx = buf_idx,
            1 => out[idx].cv_in_b_buf_idx = buf_idx,
            _ => {}
        }
    }
    out
}
