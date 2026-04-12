// ─── state/fx_plan.rs ─────────────────────────────────────────────────────────
// Compile a cable graph into an ordered FX processing plan.
// Extracted from rack.rs to keep file sizes under 1000 lines.

use std::collections::{HashMap, HashSet};

use super::{FxPlan, FxStep, ModuleKind, PortKind, RackState};

pub(crate) fn kind_to_fx_step(kind: ModuleKind) -> Option<FxStep> {
    match kind {
        ModuleKind::FxWaveshaper => Some(FxStep::Waveshaper),
        ModuleKind::FxReverb => Some(FxStep::Reverb),
        ModuleKind::FxDelay => Some(FxStep::Delay),
        ModuleKind::FxBitcrush => Some(FxStep::Bitcrush),
        ModuleKind::FxChorus => Some(FxStep::Chorus),
        ModuleKind::FxPhaser => Some(FxStep::Phaser),
        ModuleKind::FxRingMod => Some(FxStep::RingMod),
        ModuleKind::FxEq => Some(FxStep::Eq),
        ModuleKind::FxCompressor => Some(FxStep::Compressor),
        ModuleKind::FxTapeSat => Some(FxStep::TapeSat),
        ModuleKind::FxDrive => Some(FxStep::Drive),
        ModuleKind::FxAutotune => Some(FxStep::Autotune),
        _ => None,
    }
}

/// Build an `FxPlan` from the rack cable graph using a topological sort
/// (Kahn's algorithm) over FX-to-FX audio cable connections.
///
/// Only enabled FX modules that are connected to at least one other FX
/// module (or are solo in the graph) are included.  Modules with no
/// FX-to-FX cables are excluded so the plan is empty when nothing is patched.
pub fn compile_fx_plan(rack: &RackState) -> FxPlan {
    // Collect enabled FX module id → kind.
    let fx_map: HashMap<u32, ModuleKind> = rack
        .modules
        .iter()
        .filter(|m| m.enabled && kind_to_fx_step(m.kind).is_some())
        .map(|m| (m.id, m.kind))
        .collect();

    if fx_map.is_empty() {
        return FxPlan::default();
    }

    // Build FX→FX adjacency and collect FX modules that participate in any
    // audio cable (either FX→FX or voice→FX). FX modules with zero audio
    // cables are excluded — they're orphaned and shouldn't process.
    let mut fx_adj: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut connected_fx: HashSet<u32> = HashSet::new();

    for cable in &rack.cables {
        if cable.from.kind != PortKind::Audio {
            continue;
        }
        let from_fx = fx_map.contains_key(&cable.from.module_id);
        let to_fx = fx_map.contains_key(&cable.to.module_id);
        // Any audio cable touching an FX module marks it as connected
        if from_fx {
            connected_fx.insert(cable.from.module_id);
        }
        if to_fx {
            connected_fx.insert(cable.to.module_id);
        }
        // FX→FX adjacency for topological sort
        if from_fx && to_fx {
            fx_adj
                .entry(cable.from.module_id)
                .or_default()
                .push(cable.to.module_id);
        }
    }

    // In-degree only for connected FX modules
    let mut in_degree: HashMap<u32, usize> = connected_fx.iter().map(|&id| (id, 0)).collect();
    for neighbors in fx_adj.values() {
        for &nid in neighbors {
            if let Some(deg) = in_degree.get_mut(&nid) {
                *deg += 1;
            }
        }
    }

    // Kahn's topological sort — stable ordering via sorted initial queue.
    let mut queue: Vec<u32> = {
        let mut q: Vec<u32> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| *id)
            .collect();
        q.sort_unstable();
        q
    };

    let mut ordered: Vec<FxStep> = Vec::with_capacity(connected_fx.len());
    while !queue.is_empty() {
        let id = queue.remove(0);
        if let Some(&kind) = fx_map.get(&id)
            && let Some(step) = kind_to_fx_step(kind)
        {
            ordered.push(step);
        }
        if let Some(neighbors) = fx_adj.get(&id) {
            let mut next_ready: Vec<u32> = Vec::new();
            for &nid in neighbors {
                if let Some(deg) = in_degree.get_mut(&nid) {
                    *deg -= 1;
                    if *deg == 0 {
                        next_ready.push(nid);
                    }
                }
            }
            next_ready.sort_unstable();
            queue.extend(next_ready);
        }
    }

    // ── Per-voice FX routes (Voice→FX cables) ────────────────────────────────
    // Build a map: voice module kind → ordered list of FX steps reachable via
    // explicit Voice→FX audio cables.  A voice without such cables has no entry
    // here and falls through to the global chain in the DSP.

    // Voice module kinds that map to DSP buses.
    const VOICE_KINDS: &[ModuleKind] = &[
        ModuleKind::AcidBass,
        ModuleKind::DrumKit808,
        ModuleKind::DrumKit909,
        ModuleKind::HooverLead,
        ModuleKind::An1xVoice,
        ModuleKind::AmenSampler,
        ModuleKind::NoiseVoice,
        ModuleKind::GranularTexture,
        ModuleKind::NeuTts,
    ];

    // Map voice module id → kind for quick lookup.
    let voice_id_map: HashMap<u32, ModuleKind> = rack
        .modules
        .iter()
        .filter(|m| m.enabled && VOICE_KINDS.contains(&m.kind))
        .map(|m| (m.id, m.kind))
        .collect();

    let mut voice_routes: HashMap<ModuleKind, Vec<FxStep>> = HashMap::new();

    for cable in &rack.cables {
        if cable.from.kind != PortKind::Audio {
            continue;
        }
        let voice_kind = match voice_id_map.get(&cable.from.module_id) {
            Some(&k) => k,
            None => continue,
        };
        let first_fx = match fx_map.get(&cable.to.module_id) {
            Some(&k) => k,
            None => continue,
        };
        // BFS/DFS from first_fx through FX→FX adjacency to collect the sub-chain
        // reachable from this voice.
        let first_step = match kind_to_fx_step(first_fx) {
            Some(s) => s,
            None => continue,
        };
        let mut chain: Vec<FxStep> = vec![first_step];
        // Walk FX→FX adjacency from first FX module id.
        let mut cur_id = cable.to.module_id;
        let mut visited = HashSet::new();
        visited.insert(cur_id);
        // Follow the single-output chain (linear FX→FX adjacency).
        loop {
            let next_ids = fx_adj.get(&cur_id).map(|v| v.as_slice()).unwrap_or(&[]);
            match next_ids {
                [next_id] if !visited.contains(next_id) => {
                    if let Some(&kind) = fx_map.get(next_id)
                        && let Some(step) = kind_to_fx_step(kind)
                    {
                        visited.insert(*next_id);
                        chain.push(step);
                        cur_id = *next_id;
                    } else {
                        break;
                    }
                }
                _ => break, // fan-out, cycle, or end of chain
            }
        }
        voice_routes.entry(voice_kind).or_insert(chain);
    }

    FxPlan {
        steps: ordered,
        voice_routes,
    }
}
