// ─── llm/lane_scheduler.rs ───────────────────────────────────────────────────
// Phase 2 of the lane lifecycle: weighted single-lane picker for jam cycles.
//
// Instead of re-firing every voice every jam cycle, pick ONE lane per cycle
// weighted by
//     weight = dynamism(lane) * (1 - score) * recency_decay * heat_jitter
// so high-scoring lanes "live longer" between rewrites and lanes that just
// fired don't immediately come back around.  Scores come from Phase 1's
// `lane_eval` (observation-only until now); Phase 4 will let styles.json
// override the dynamism constants below.

use crate::llm::lanes::LaneKind;
use crate::llm::styles::Style;
use crate::state::{AppState, LaneScore};

/// Baked-in per-lane dynamism defaults — genre-neutral baseline.  Used when
/// the active style has no override for this lane.  0.0 = "the scheduler
/// never picks this", 1.0 = "always in play".  Rack stays at 0.0 because
/// rack composition is user-owned.
pub fn baseline_dynamism(lane: LaneKind) -> f32 {
    match lane {
        LaneKind::Bass(_) => 0.90,
        LaneKind::KitA | LaneKind::KitB => 0.85,
        LaneKind::Amen => 0.80,
        LaneKind::Hoover | LaneKind::An1x => 0.75,
        LaneKind::Modulation => 0.40,
        LaneKind::Fx => 0.45,
        LaneKind::Settings => 0.25,
        LaneKind::Rack => 0.0,
    }
}

/// Group label for a lane — coarser than `lane.label()`.  Lets a style set
/// `"bass": 0.9` once and have it apply to every bass voice rather than
/// listing `bass1`, `bass2`, `bass3`, `bass4` individually.
fn group_label(lane: LaneKind) -> &'static str {
    match lane {
        LaneKind::Bass(_) => "bass",
        LaneKind::KitA => "kit_a",
        LaneKind::KitB => "kit_b",
        LaneKind::Amen => "amen",
        LaneKind::Hoover => "hoover",
        LaneKind::An1x => "an1x",
        LaneKind::Fx => "fx",
        LaneKind::Modulation => "mod",
        LaneKind::Settings => "settings",
        LaneKind::Rack => "rack",
    }
}

/// Resolve the dynamism for `lane` under an optional `style`.  Lookup
/// order: exact label (`"bass1"`) → group label (`"bass"`) → baseline.
/// Rack is hard-capped at 0 regardless of style — user-owned composition.
pub fn effective_dynamism(lane: LaneKind, style: Option<&Style>) -> f32 {
    if matches!(lane, LaneKind::Rack) {
        return 0.0;
    }
    if let Some(st) = style {
        let map = &st.lane_dynamism;
        if let Some(v) = map.get(lane.label()) {
            return v.clamp(0.0, 1.0);
        }
        if let Some(v) = map.get(group_label(lane)) {
            return v.clamp(0.0, 1.0);
        }
    }
    baseline_dynamism(lane)
}

/// Lane-score auto-tuner bias factor.  Multiplies `effective_dynamism`
/// when a long-term running average exists for this (style, lane)
/// pair.  Centred at 1.0 (neutral); a 1.0 average yields ~1.30
/// (boost), a 0.0 average yields ~0.70 (reduce).  Bounded so a poor
/// early run can't permanently disable a lane.  Returns 1.0 when no
/// running average exists yet.
///
/// Pure helper so the bias formula can be unit-tested independent of
/// the larger weight pipeline.
pub fn auto_tuner_bias(avg: Option<f32>, n: u32) -> f32 {
    // Need a few samples before we trust the average; the smoothed
    // weight ramps from 0 (no trust) to 1 (full trust) over ~5
    // observations so a single fluke score barely shifts the bias.
    let trust = (n as f32 / 5.0).clamp(0.0, 1.0);
    let raw = avg.unwrap_or(0.5);
    let bias = 1.0 + (raw - 0.5) * 0.6 * trust;
    bias.clamp(0.7, 1.3)
}

/// Look up the auto-tuner bias for a lane under the active style.
/// Returns `(bias, sample_count)` so callers can both apply the bias
/// and surface the trust level (e.g. in a debug overlay).
pub fn lane_auto_tuner_bias(state: &AppState, lane: LaneKind) -> (f32, u32) {
    let Some(style_id) = state.llm.active_style.as_deref() else {
        return (1.0, 0);
    };
    let key = crate::state::lane_avg_key(style_id, lane.label());
    let Some(avg) = state.llm.lane_avg_per_style.get(&key) else {
        return (1.0, 0);
    };
    (auto_tuner_bias(avg.mean(), avg.n), avg.n)
}

/// Backwards-compat alias — callers that don't know about styles get the
/// baseline.  Kept so existing tests and any downstream code keep working.
pub fn lane_dynamism(lane: LaneKind) -> f32 {
    baseline_dynamism(lane)
}

/// Weight penalty for having fired recently.  `gap` is `current_cycle -
/// last_changed_cycle`; a never-picked lane (score == None) returns 1.0.
///   gap=0 → 0.0   (just changed — don't pick again)
///   gap=1 → 0.5
///   gap=3 → 0.75
///   gap=7 → 0.875
pub fn recency_decay(score: Option<&LaneScore>, current_cycle: u32) -> f32 {
    match score {
        None => 1.0,
        Some(s) => {
            let gap = current_cycle.saturating_sub(s.last_changed_cycle);
            1.0 - 1.0 / (1.0 + gap as f32)
        }
    }
}

/// Per-candidate jitter, scaled by global heat.  At heat=0 this returns
/// 1.0 exactly (deterministic ordering); at heat=1 it can roughly double
/// a candidate's weight so the pick feels looser when the model is hot.
pub fn heat_jitter(heat: f32, rng: &mut Xorshift32) -> f32 {
    let amount = heat.clamp(0.0, 1.0) * 0.6;
    1.0 + rng.unit_f32() * amount
}

/// Pure weight formula — factored out of `pick_jam_lane` for unit tests.
/// Returns 0.0 for any lane with effective dynamism 0 (Rack, or a style
/// override that set this lane to 0).
pub fn compute_weight(
    lane: LaneKind,
    score: Option<&LaneScore>,
    current_cycle: u32,
    heat: f32,
    style: Option<&Style>,
    rng: &mut Xorshift32,
) -> f32 {
    let dyn_w = effective_dynamism(lane, style);
    if dyn_w <= 0.0 {
        return 0.0;
    }
    let score_term = 1.0 - score.map(|s| s.score).unwrap_or(0.5);
    let recency = recency_decay(score, current_cycle);
    let jitter = heat_jitter(heat, rng);
    (dyn_w * score_term * recency * jitter).max(0.0)
}

/// Weighted-random pick from `candidates`.  Returns `None` when
/// `candidates` is empty or every weight collapsed to 0 (e.g. all lanes
/// just updated).  Caller should fall back to `default_plan` on `None`.
/// Consults the active style's `lane_dynamism` map before falling back to
/// `baseline_dynamism` for each candidate.
pub fn pick_jam_lane(
    state: &AppState,
    candidates: &[LaneKind],
    rng: &mut Xorshift32,
) -> Option<LaneKind> {
    if candidates.is_empty() {
        return None;
    }
    let cycle = state.llm.jam_cycle_count;
    let heat = state.llm.heat;
    let style = state
        .llm
        .active_style
        .as_deref()
        .and_then(|id| crate::llm::styles::StyleCatalog::get().find_by_id(id));
    let weights: Vec<f32> = candidates
        .iter()
        .map(|&lane| {
            let score = state.llm.lane_scores.get(lane.label());
            let (bias, _n) = lane_auto_tuner_bias(state, lane);
            compute_weight(lane, score, cycle, heat, style, rng) * bias
        })
        .collect();
    let total: f32 = weights.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let target = rng.unit_f32() * total;
    let mut acc = 0.0_f32;
    for (i, w) in weights.iter().enumerate() {
        acc += *w;
        if target <= acc {
            return Some(candidates[i]);
        }
    }
    // Numerical fallthrough — pick the last candidate.
    Some(*candidates.last().unwrap())
}

/// Tiny xorshift32 PRNG.  No external RNG crate needed for weighted
/// sampling over a handful of lanes; deterministic under a fixed seed
/// so tests can pin the pick.
pub struct Xorshift32 {
    state: u32,
}

impl Xorshift32 {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E3779B9 } else { seed },
        }
    }

    /// Seed from wall-clock nanos — production paths want a fresh
    /// sequence every cycle.
    pub fn from_entropy() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0x9E3779B9);
        Self::new(seed)
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Uniform `[0, 1)` with 24-bit precision — plenty for a handful of
    /// lane weights.
    pub fn unit_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / ((1u32 << 24) as f32))
    }
}
