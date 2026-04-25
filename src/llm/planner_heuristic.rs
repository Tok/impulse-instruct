// ─── llm/planner_heuristic.rs ────────────────────────────────────────────────
// `heuristic_plan` + its keyword-matching helpers.  Separates the
// prompt-classification fast path (narrow single-topic commands that
// should skip the planner LLM entirely) from the planner prompt /
// schema / parse machinery in planner.rs.

use crate::state::AppState;

use super::lanes::LaneKind;
use super::planner::{LanePlan, lane_is_live_pub};

/// Try to resolve the user prompt to a lane plan without calling the
/// planner LLM.  Handles the common narrow single-topic phrasings:
///
///   "rewrite bass 2"          → [Bass(1)]
///   "BASS 2"                  → [Bass(1)]
///   "second bass voice"       → [Bass(1)]
///   "change bass3"            → [Bass(2)]
///   "add reverb"              → [Fx]
///   "more delay"              → [Fx]
///   "remove kick"             → [KitA]
///   "change the 909 kick"     → [KitB]
///
/// Returns `None` when the prompt either matches nothing or mentions
/// more than one narrow topic — the caller falls through to the LLM
/// planner for anything more nuanced than a single-voice command.
pub fn heuristic_plan(state: &AppState, user_prompt: &str) -> Option<LanePlan> {
    let lower = user_prompt.to_lowercase();
    // Don't even try on long / multi-clause prompts.  The heuristic is
    // for short narrow commands; broader prompts deserve the planner
    // LLM's reasoning.  ~120 chars is roughly a "sentence".
    if lower.len() > 120 {
        return None;
    }

    // ── Single bass voice ────────────────────────────────────────────────
    if let Some(idx) = detect_bass_voice(&lower)
        && lane_is_live_pub(state, LaneKind::Bass(idx))
        && !mentions_other_lane(&lower, MentionKind::Bass)
    {
        return Some(LanePlan {
            lanes: vec![LaneKind::Bass(idx)],
            rationale: format!("narrow command → bass voice {} (heuristic)", idx + 1),
            from_retry: false,
        });
    }

    // ── FX-only commands ────────────────────────────────────────────────
    // "add reverb", "more delay", "no reverb", "reverb 40%", "chorus on".
    if mentions_fx(&lower) && !mentions_other_lane(&lower, MentionKind::Fx) {
        return Some(LanePlan {
            lanes: vec![LaneKind::Fx],
            rationale: "narrow command → fx (heuristic)".into(),
            from_retry: false,
        });
    }

    // ── Kit-only commands ───────────────────────────────────────────────
    // `kit a`, `808`, `kick a`, `snare a`, `hihat a` → KitA.
    // `kit b`, `909`, `kick b`, `snare b`, `clap b` → KitB.
    // Bare `kick`, `snare`, `hats` with no disambiguator default to
    // whichever kit is live (preferring KitA).  A prompt mentioning
    // both kits or a voice name from the other side falls through to
    // the LLM planner.
    let wants_kit_a = lower.contains("kit a")
        || lower.contains("kit_a")
        || lower.contains("808")
        || lower.contains("kick a")
        || lower.contains("kick_a")
        || lower.contains("snare a")
        || lower.contains("snare_a");
    let wants_kit_b = lower.contains("kit b")
        || lower.contains("kit_b")
        || lower.contains("909")
        || lower.contains("kick b")
        || lower.contains("kick_b")
        || lower.contains("snare b")
        || lower.contains("snare_b")
        || lower.contains("clap");
    if wants_kit_a
        && !wants_kit_b
        && lane_is_live_pub(state, LaneKind::KitA)
        && !mentions_other_lane(&lower, MentionKind::KitA)
    {
        return Some(LanePlan {
            lanes: vec![LaneKind::KitA],
            rationale: "narrow command → kit_a (heuristic)".into(),
            from_retry: false,
        });
    }
    if wants_kit_b
        && !wants_kit_a
        && lane_is_live_pub(state, LaneKind::KitB)
        && !mentions_other_lane(&lower, MentionKind::KitB)
    {
        return Some(LanePlan {
            lanes: vec![LaneKind::KitB],
            rationale: "narrow command → kit_b (heuristic)".into(),
            from_retry: false,
        });
    }

    // ── Hoover / An1x / Amen — straight name matches ────────────────────
    for (keyword, lane) in &[
        ("hoover", LaneKind::Hoover),
        ("an1x", LaneKind::An1x),
        ("amen", LaneKind::Amen),
    ] {
        if lower.contains(keyword)
            && lane_is_live_pub(state, *lane)
            && !mentions_other_lane(&lower, MentionKind::from_lane(*lane))
        {
            return Some(LanePlan {
                lanes: vec![*lane],
                rationale: format!("narrow command → {} (heuristic)", lane.label()),
                from_retry: false,
            });
        }
    }

    None
}

/// Which lane a mention-check excludes from the "other lane" probe.
/// Used so "bass 2" doesn't accidentally trigger on "the bass" when
/// the rest of the prompt already mentions bass.
#[derive(Copy, Clone)]
enum MentionKind {
    Bass,
    Fx,
    KitA,
    KitB,
    Hoover,
    An1x,
    Amen,
}

impl MentionKind {
    fn from_lane(lane: LaneKind) -> Self {
        match lane {
            LaneKind::Bass(_) => MentionKind::Bass,
            LaneKind::Fx => MentionKind::Fx,
            LaneKind::KitA => MentionKind::KitA,
            LaneKind::KitB => MentionKind::KitB,
            LaneKind::Hoover => MentionKind::Hoover,
            LaneKind::An1x => MentionKind::An1x,
            LaneKind::Amen => MentionKind::Amen,
            _ => MentionKind::Fx, // lanes without a specific mention-set
        }
    }
}

/// Return true when the prompt mentions a DIFFERENT lane than `self_kind`.
/// Used to gate the narrow-command heuristic: if a bass voice is named
/// but the prompt also mentions drums or FX, the user wants a broader
/// turn and the LLM planner should take over.
fn mentions_other_lane(lower: &str, self_kind: MentionKind) -> bool {
    let bass = lower.contains("bass");
    let fx = mentions_fx(lower);
    let kit_a = lower.contains("808")
        || lower.contains("kit a")
        || lower.contains("kit_a")
        || lower.contains("kick a")
        || lower.contains("snare a");
    let kit_b = lower.contains("909")
        || lower.contains("kit b")
        || lower.contains("kit_b")
        || lower.contains("kick b")
        || lower.contains("snare b")
        || lower.contains("clap");
    let hoover = lower.contains("hoover");
    let an1x = lower.contains("an1x");
    let amen = lower.contains("amen");
    // Bare drum words with no A/B side indicator are ambiguous — don't
    // count them so "remove kick" doesn't match "bass".
    match self_kind {
        MentionKind::Bass => fx || kit_a || kit_b || hoover || an1x || amen,
        MentionKind::Fx => bass || kit_a || kit_b || hoover || an1x || amen,
        MentionKind::KitA => bass || fx || kit_b || hoover || an1x || amen,
        MentionKind::KitB => bass || fx || kit_a || hoover || an1x || amen,
        MentionKind::Hoover => bass || fx || kit_a || kit_b || an1x || amen,
        MentionKind::An1x => bass || fx || kit_a || kit_b || hoover || amen,
        MentionKind::Amen => bass || fx || kit_a || kit_b || hoover || an1x,
    }
}

/// Look for an FX noun in a way that doesn't match plain English.
/// "reverb", "delay" (but NOT "delays the pattern"), "distortion",
/// "chorus", "bitcrush", "compressor", "bitcrusher".
fn mentions_fx(lower: &str) -> bool {
    const FX_WORDS: &[&str] = &[
        "reverb",
        "delay",
        "distort",
        "chorus",
        "bitcrush",
        "bitcrusher",
        "phaser",
        "flanger",
        "flange",
        "filter",
        "svf",
        "comb",
        "resonator",
        "tilt",
        "transient",
        "exciter",
        "limiter",
        "brick wall",
        "brickwall",
        "multitap",
        "ping pong",
        "revdelay",
        "reverse delay",
        "tape stop",
        "tapestop",
        "stutter",
        "glitch",
        "freeze",
        "spectral freeze",
        "drone",
        "compressor",
        "tape sat",
        "tapesat",
        "flutter",
    ];
    FX_WORDS.iter().any(|w| lower.contains(w))
}

/// Detect a specific bass voice index (0..=3) from common phrasings:
/// "bass 2", "bass2", "bass #2", "bass voice 2", "second bass",
/// "second bass voice", "2nd bass", "bass two".  Returns the 0-based
/// index, or `None` when no specific voice is named.
fn detect_bass_voice(lower: &str) -> Option<usize> {
    // Each inner slice lists distinct phrasings for a given voice index.
    // Order: index 0 = voice #1, index 3 = voice #4.
    let phrasings: [&[&str]; 4] = [
        &[
            "bass 1",
            "bass1",
            "bass #1",
            "bass#1",
            "bass voice 1",
            "bass voice one",
            "first bass",
            "1st bass",
            "bass one",
        ],
        &[
            "bass 2",
            "bass2",
            "bass #2",
            "bass#2",
            "bass voice 2",
            "bass voice two",
            "second bass",
            "2nd bass",
            "bass two",
        ],
        &[
            "bass 3",
            "bass3",
            "bass #3",
            "bass#3",
            "bass voice 3",
            "bass voice three",
            "third bass",
            "3rd bass",
            "bass three",
        ],
        &[
            "bass 4",
            "bass4",
            "bass #4",
            "bass#4",
            "bass voice 4",
            "bass voice four",
            "fourth bass",
            "4th bass",
            "bass four",
        ],
    ];
    for (idx, list) in phrasings.iter().enumerate() {
        if list.iter().any(|p| lower.contains(p)) {
            return Some(idx);
        }
    }
    None
}
