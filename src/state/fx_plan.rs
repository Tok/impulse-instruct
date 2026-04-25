// ─── state/fx_plan.rs ─────────────────────────────────────────────────────────
// Compile a cable graph into an ordered FX processing plan.
// Extracted from rack.rs to keep file sizes under 1000 lines.

use std::collections::{HashMap, HashSet, VecDeque};

use super::fx_types::{FeedbackRoute, VoiceSend};
use super::rack::FEEDBACK_GAIN_MAX;
use super::{FxPlan, FxStep, ModuleKind, PortKind, RackState};

pub(crate) fn kind_to_fx_step(kind: ModuleKind) -> Option<FxStep> {
    match kind {
        ModuleKind::FxWaveshaper => Some(FxStep::Waveshaper),
        ModuleKind::FxReverb => Some(FxStep::Reverb),
        ModuleKind::FxDelay => Some(FxStep::Delay),
        ModuleKind::FxBitcrush => Some(FxStep::Bitcrush),
        ModuleKind::FxChorus => Some(FxStep::Chorus),
        ModuleKind::FxPhaser => Some(FxStep::Phaser),
        ModuleKind::FxFlanger => Some(FxStep::Flanger),
        ModuleKind::FxLimiter => Some(FxStep::Limiter),
        ModuleKind::FxFilter => Some(FxStep::Filter),
        ModuleKind::FxComb => Some(FxStep::Comb),
        ModuleKind::FxTilt => Some(FxStep::Tilt),
        ModuleKind::FxTransient => Some(FxStep::Transient),
        ModuleKind::FxExciter => Some(FxStep::Exciter),
        ModuleKind::FxRingMod => Some(FxStep::RingMod),
        ModuleKind::FxEq => Some(FxStep::Eq),
        ModuleKind::FxCompressor => Some(FxStep::Compressor),
        ModuleKind::FxTapeSat => Some(FxStep::TapeSat),
        ModuleKind::FxDrive => Some(FxStep::Drive),
        ModuleKind::FxAutotune => Some(FxStep::Autotune),
        ModuleKind::FxPan => Some(FxStep::Pan),
        ModuleKind::FxConvReverb => Some(FxStep::ConvReverb),
        ModuleKind::FxParamEq => Some(FxStep::ParamEq),
        ModuleKind::FxPitchShift => Some(FxStep::PitchShift),
        _ => None,
    }
}

/// Whether a `ModuleKind` maps to an `FxStep` — used by the rack's
/// cycle check to scope feedback permissions to FX-only cycles.
pub fn kind_is_fx(kind: ModuleKind) -> bool {
    kind_to_fx_step(kind).is_some()
}

/// True if the directed graph with edges `(u -> v)` for every `(u, v)` in
/// `edges` would have a cycle once the single edge `(from, to)` is added.
/// `nodes` is the full node set so we can seed in-degree for Kahn's sort.
fn edge_creates_cycle(nodes: &HashSet<u32>, edges: &[(u32, u32)], from: u32, to: u32) -> bool {
    if from == to {
        return true;
    }
    // BFS from `to` — if we can reach `from` already, the new edge closes a cycle.
    let mut adj: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(u, v) in edges {
        adj.entry(u).or_default().push(v);
    }
    let mut visited: HashSet<u32> = HashSet::new();
    let mut queue = VecDeque::from([to]);
    while let Some(n) = queue.pop_front() {
        if n == from {
            return true;
        }
        if visited.insert(n)
            && let Some(neigh) = adj.get(&n)
        {
            queue.extend(neigh);
        }
    }
    let _ = nodes;
    false
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
    //
    // FX→FX cables are classified greedily: each cable is added to the
    // forward graph if doing so wouldn't close a cycle.  Cables that
    // WOULD close a cycle are lifted out as back-edges and compiled
    // into `feedback_routes`.  Cable order here is the rack's cable
    // insertion order — the same across saves, so the choice of which
    // edge becomes the feedback edge is deterministic.
    let mut fx_adj: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut fwd_edges: Vec<(u32, u32)> = Vec::new();
    let mut connected_fx: HashSet<u32> = HashSet::new();
    let mut feedback_edges: Vec<(u32, u32, f32)> = Vec::new();

    for cable in &rack.cables {
        if cable.from.kind != PortKind::Audio {
            continue;
        }
        let from_fx = fx_map.contains_key(&cable.from.module_id);
        let to_fx = fx_map.contains_key(&cable.to.module_id);
        if from_fx {
            connected_fx.insert(cable.from.module_id);
        }
        if to_fx {
            connected_fx.insert(cable.to.module_id);
        }
        if from_fx && to_fx {
            let from_id = cable.from.module_id;
            let to_id = cable.to.module_id;
            if edge_creates_cycle(&connected_fx, &fwd_edges, from_id, to_id) {
                // Back-edge → feedback route.  Clamp the user-supplied
                // gain to FEEDBACK_GAIN_MAX here so the audio thread
                // never has to re-check.
                let gain = cable.audio_gain.clamp(0.0, FEEDBACK_GAIN_MAX);
                feedback_edges.push((from_id, to_id, gain));
            } else {
                fwd_edges.push((from_id, to_id));
                fx_adj.entry(from_id).or_default().push(to_id);
            }
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

    // Each Voice→FX cable becomes its own `VoiceSend` (independent
    // chain + gain), not just the first.  Voices can now drive N
    // parallel FX sub-chains and the audio thread mixes their outputs
    // — classic "voice → reverb @ 30% + voice → delay @ 50%" patches
    // work naturally.
    let mut voice_routes: HashMap<ModuleKind, Vec<VoiceSend>> = HashMap::new();

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
        // Per-send wet gain from the cable's `audio_gain` (already
        // clamped at cable creation; clamp again defensively in case a
        // hand-edited project file sneaks past).  Empty chains can
        // happen if the target FX kind isn't mapped — skip them.
        if chain.is_empty() {
            continue;
        }
        let gain = cable.audio_gain.clamp(0.0, 1.5);
        voice_routes
            .entry(voice_kind)
            .or_default()
            .push(VoiceSend { chain, gain });
    }

    // Translate back-edges (module-id-keyed) into FeedbackRoutes keyed by FxStep.
    let feedback_routes: Vec<FeedbackRoute> = feedback_edges
        .into_iter()
        .filter_map(|(from_id, to_id, gain)| {
            let src_kind = fx_map.get(&from_id).copied()?;
            let tgt_kind = fx_map.get(&to_id).copied()?;
            let source = kind_to_fx_step(src_kind)?;
            let target = kind_to_fx_step(tgt_kind)?;
            Some(FeedbackRoute {
                source,
                target,
                gain,
            })
        })
        .collect();

    FxPlan {
        steps: ordered,
        voice_routes,
        feedback_routes,
    }
}
