// ─── llm/lane_eval.rs ────────────────────────────────────────────────────────
// Lane-output scoring.  After a successful pipeline lane apply, we read the
// resulting state and score how well the LLM followed the rules we encode
// in the system prompt — density, subset rule, full-coverage rule, in-scale
// ratio, etc.  Phase 1: scores are stored on `LlmState.lane_scores` and
// logged; Phase 2 will use them to weight the next jam-cycle's lane pick
// (more recently failed → more eligible for retry; high-scoring lane → live
// longer).
//
// All checks are pure functions of `AppState`.  They never reject the
// output (the pipeline already applied it), they just measure quality.
// Each lane returns 0..1.  Unknown lanes return 0.5 (neutral) so they
// don't accidentally bias the future scheduler.

use crate::llm::lanes::LaneKind;
use crate::state::AppState;

/// Score the lane that was just applied.  Returns a value in `[0.0, 1.0]`.
pub fn evaluate_lane(state: &AppState, lane: LaneKind) -> f32 {
    match lane {
        LaneKind::Bass(idx) => evaluate_bass(state, idx),
        LaneKind::KitA => evaluate_kit(state, true),
        LaneKind::KitB => evaluate_kit(state, false),
        LaneKind::Amen => evaluate_amen(state),
        LaneKind::Hoover => evaluate_melodic(state, MelodicLane::Hoover),
        LaneKind::An1x => evaluate_melodic(state, MelodicLane::An1x),
        LaneKind::Fx => evaluate_fx(state),
        LaneKind::Settings => evaluate_settings(state),
        LaneKind::Modulation | LaneKind::Rack => 0.5,
    }
}

// ─── Bass ─────────────────────────────────────────────────────────────────────

fn evaluate_bass(state: &AppState, idx: usize) -> f32 {
    let seq = &state.sequencer;
    let pattern = if idx == 0 {
        &seq.bass_pattern
    } else if let Some(p) = seq.bass_patterns.get(idx) {
        p
    } else {
        return 0.0;
    };
    let voice_steps = seq
        .bass_voice_steps
        .get(idx)
        .copied()
        .unwrap_or(seq.bass_steps)
        .max(1);
    let active: Vec<&crate::state::TB303Step> = pattern
        .iter()
        .take(voice_steps)
        .filter(|s| s.active)
        .collect();
    let n = active.len();

    let mut score = 0.0_f32;
    let mut weight = 0.0_f32;

    // Density: 3–7 notes is the sweet spot we ask for in the prompt.
    let density = if n == 0 {
        0.0
    } else if (3..=7).contains(&n) {
        1.0
    } else if (2..=10).contains(&n) {
        0.7
    } else {
        0.4
    };
    score += density * 0.40;
    weight += 0.40;

    // Note variety: at least 2 distinct pitches, ideally 3+.
    let distinct: std::collections::HashSet<u8> = active.iter().map(|s| s.note).collect();
    let variety = match distinct.len() {
        0 => 0.0,
        1 => 0.35,
        2 => 0.75,
        _ => 1.0,
    };
    score += variety * 0.25;
    weight += 0.25;

    // Accent presence — system prompt asks for accents on some steps.
    let accent_count = active.iter().filter(|s| s.accent > 0.0).count();
    let accent = if n == 0 {
        0.0
    } else {
        (accent_count as f32 / n as f32 * 3.0).min(1.0)
    };
    score += accent * 0.20;
    weight += 0.20;

    // Slide presence — same prompt directive.
    let slide_count = active.iter().filter(|s| s.slide > 0.0).count();
    let slide = if n == 0 {
        0.0
    } else {
        (slide_count as f32 / n as f32 * 4.0).min(1.0)
    };
    score += slide * 0.15;
    weight += 0.15;

    if weight > 0.0 { score / weight } else { 0.5 }
}

// ─── Drum kits ────────────────────────────────────────────────────────────────

fn evaluate_kit(state: &AppState, voice_a: bool) -> f32 {
    use crate::state::DrumVoice;
    let seq = &state.sequencer;
    let kick = if voice_a {
        DrumVoice::Kick808
    } else {
        DrumVoice::Kick909
    };
    let hat = if voice_a {
        DrumVoice::HihatClosed808
    } else {
        DrumVoice::HihatClosed909
    };

    let count = |v: &DrumVoice| -> usize {
        seq.drum_patterns
            .get(v)
            .map(|p| p.iter().take(seq.steps).filter(|s| s.active).count())
            .unwrap_or(0)
    };
    let kick_n = count(&kick);
    let hat_n = count(&hat);

    let mut score = 0.0_f32;
    let mut weight = 0.0_f32;

    // Full-coverage rule: kick + hihat both populated for beat-driven kits.
    let coverage = match (kick_n > 0, hat_n > 0) {
        (true, true) => 1.0,
        (true, false) | (false, true) => 0.5,
        (false, false) => 0.0,
    };
    score += coverage * 0.50;
    weight += 0.50;

    // Kick density: 4–16 hits per 32 steps reads as a usable kick line.
    let kick_density = if kick_n == 0 {
        0.0
    } else if (4..=16).contains(&kick_n) {
        1.0
    } else if (2..=24).contains(&kick_n) {
        0.6
    } else {
        0.3
    };
    score += kick_density * 0.30;
    weight += 0.30;

    // Hat density: 4–32 hits is the wide-band "playable" range.
    let hat_density = if hat_n == 0 {
        0.0
    } else if (4..=32).contains(&hat_n) {
        1.0
    } else {
        0.5
    };
    score += hat_density * 0.20;
    weight += 0.20;

    if weight > 0.0 { score / weight } else { 0.5 }
}

// ─── Amen ─────────────────────────────────────────────────────────────────────

fn evaluate_amen(state: &AppState) -> f32 {
    use crate::state::DrumVoice;
    let seq = &state.sequencer;
    let active = seq
        .drum_patterns
        .get(&DrumVoice::Amen)
        .map(|p| p.iter().take(seq.steps).filter(|s| s.active).count())
        .unwrap_or(0);
    if active == 0 {
        // Amen lane disabled or didn't produce — neutral, not a failure.
        return 0.5;
    }
    if (4..=24).contains(&active) { 1.0 } else { 0.6 }
}

// ─── Hoover / AN1X — same shape, different state field ────────────────────────

enum MelodicLane {
    Hoover,
    An1x,
}

fn evaluate_melodic(state: &AppState, lane: MelodicLane) -> f32 {
    let seq = &state.sequencer;
    let (pattern, span, enabled) = match lane {
        MelodicLane::Hoover => (
            &seq.hoover_pattern,
            seq.hoover_steps.max(1),
            state.hoover.enabled,
        ),
        MelodicLane::An1x => (&seq.an1x_pattern, seq.an1x_steps.max(1), state.an1x.enabled),
    };
    if !enabled {
        return 0.5;
    }
    let active: Vec<_> = pattern.iter().take(span).filter(|s| s.active).collect();
    let n = active.len();
    if n == 0 {
        return 0.2;
    }
    let density = if (2..=12).contains(&n) { 1.0 } else { 0.6 };
    let distinct: std::collections::HashSet<u8> = active.iter().map(|s| s.note).collect();
    let variety = match distinct.len() {
        0 | 1 => 0.4,
        2 => 0.75,
        _ => 1.0,
    };
    density * 0.6 + variety * 0.4
}

// ─── FX ───────────────────────────────────────────────────────────────────────

fn evaluate_fx(state: &AppState) -> f32 {
    let fx = &state.fx;
    // Read a handful of mix-style knobs; an "all-zero" or "all-saturated"
    // result usually means the LLM punted.  Anything in the broad middle
    // band counts as a usable FX state.
    let knobs = [
        fx.reverb_mix,
        fx.delay_mix,
        fx.chorus_mix,
        fx.distortion_mix,
        fx.bitcrush_mix,
    ];
    let all_zero = knobs.iter().all(|v| *v < 0.01);
    let all_one = knobs.iter().all(|v| *v > 0.99);
    if all_zero || all_one {
        return 0.3;
    }
    let in_band = knobs.iter().filter(|v| (0.05..=0.95).contains(*v)).count();
    (in_band as f32 / knobs.len() as f32 * 0.5 + 0.5).min(1.0)
}

// ─── Settings ─────────────────────────────────────────────────────────────────

fn evaluate_settings(state: &AppState) -> f32 {
    let bpm = state.sequencer.bpm;
    let bpm_ok = (60.0..=200.0).contains(&bpm);
    let swing_ok = (0.0..=1.0).contains(&state.sequencer.swing);
    match (bpm_ok, swing_ok) {
        (true, true) => 1.0,
        (true, false) | (false, true) => 0.6,
        (false, false) => 0.3,
    }
}

// ─── Score-storage helper used by `pipeline_events::handle_pipeline_event` ───

/// Record a score for `lane` against the current state.  Bumps `change_count`,
/// updates `last_changed_cycle`, and overwrites `score`.  The tag emitted to
/// the log is short (`lane_eval: bass1 → 0.72 [#3]`) so it's easy to scan.
pub fn record_lane_score(state: &mut crate::state::LlmState, lane: LaneKind, score: f32) {
    use crate::state::LaneScore;
    let key = lane.label().to_string();
    let entry = state.lane_scores.entry(key.clone()).or_insert(LaneScore {
        score: 0.5,
        last_changed_cycle: 0,
        change_count: 0,
    });
    entry.score = score.clamp(0.0, 1.0);
    entry.last_changed_cycle = state.jam_cycle_count;
    entry.change_count = entry.change_count.saturating_add(1);
    log::info!(
        "lane_eval: {} → {:.2} [#{}]",
        key,
        entry.score,
        entry.change_count
    );
}
