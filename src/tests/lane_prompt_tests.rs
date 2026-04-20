// ─── tests/lane_prompt_tests.rs ──────────────────────────────────────────────
// Covers `build_lane_prompt` — the per-lane system prompt used to focus
// a single LLM inference on one slice of the synth.  Testable without
// ossifying wording by asserting on structural / data invariants:
// the STATE header carries BPM / steps / key, the HARMONY block fires
// only for melodic lanes, the bass context lands only on Bass lanes,
// the coverage summary shows up only on Settings, and the task
// description footer mentions the lane's label.

use crate::llm::lanes::{LaneKind, build_lane_prompt};
use crate::state::AppState;

// ─── STATE header ───────────────────────────────────────────────────────────

#[test]
fn state_header_shows_bpm_steps_and_key() {
    let mut s = AppState::default();
    s.sequencer.bpm = 138.0;
    s.sequencer.steps = 32;
    s.sequencer.root_note = 5; // F
    let prompt = build_lane_prompt(&s, LaneKind::Bass(0));
    // The STATE line should carry current BPM / step count / root name.
    assert!(
        prompt.contains("bpm=138"),
        "STATE must surface BPM (got prompt snippet: {:?})",
        prompt
            .lines()
            .find(|l| l.starts_with("STATE:"))
            .unwrap_or(""),
    );
    assert!(prompt.contains("steps=32"));
    // Root note 5 → "F" via ROOT_NAMES.
    assert!(prompt.contains("key=F"));
}

#[test]
fn locked_params_line_reflects_locked_set() {
    let mut s = AppState::default();
    s.llm.locked_params.insert("bass.cutoff".to_string());
    s.llm.locked_params.insert("fx.reverb_mix".to_string());
    let prompt = build_lane_prompt(&s, LaneKind::Fx);
    assert!(
        prompt.contains("bass.cutoff"),
        "LOCKED line must list each locked param path",
    );
    assert!(prompt.contains("fx.reverb_mix"));
}

#[test]
fn locked_params_none_renders_as_none_literal() {
    // Empty locked set must render as "none" — a blank would confuse
    // the model about whether the feature exists.
    let s = AppState::default();
    let prompt = build_lane_prompt(&s, LaneKind::Fx);
    assert!(
        prompt.contains("LOCKED (never overwrite): none"),
        "empty locked set must render as 'none'",
    );
}

// ─── HARMONY block ──────────────────────────────────────────────────────────

#[test]
fn harmony_block_appears_only_for_melodic_lanes() {
    // Bass / Hoover / An1x get the scale-note palette.  Drum / Settings /
    // FX / Modulation / Rack do NOT — adding one to a drum prompt would
    // confuse the model about whether drums should be "in key".
    let s = AppState::default();
    for lane in [LaneKind::Bass(0), LaneKind::Hoover, LaneKind::An1x] {
        let p = build_lane_prompt(&s, lane);
        assert!(
            p.contains("HARMONY:"),
            "{lane:?} should get a HARMONY block",
        );
    }
    for lane in [
        LaneKind::KitA,
        LaneKind::KitB,
        LaneKind::Settings,
        LaneKind::Fx,
        LaneKind::Modulation,
        LaneKind::Rack,
    ] {
        let p = build_lane_prompt(&s, lane);
        assert!(
            !p.contains("HARMONY:"),
            "{lane:?} must NOT get a HARMONY block",
        );
    }
}

// ─── Bass context block ─────────────────────────────────────────────────────

#[test]
fn bass_voices_summary_appears_only_on_bass_lanes() {
    // The "Active bass voices" summary is there for bass writers to
    // see what the other voices are doing.  FX / drums / modulation
    // don't need it and including it wastes tokens.
    let s = AppState::default();
    let bass = build_lane_prompt(&s, LaneKind::Bass(0));
    let fx = build_lane_prompt(&s, LaneKind::Fx);
    let kit = build_lane_prompt(&s, LaneKind::KitA);
    assert!(bass.contains("Active bass voices"));
    assert!(!fx.contains("Active bass voices"));
    assert!(!kit.contains("Active bass voices"));
}

// ─── Coverage block ─────────────────────────────────────────────────────────

#[test]
fn voice_coverage_summary_appears_only_on_settings_lane() {
    // The rack-voice coverage line is a hint for the initial settings
    // pass — every other lane already knows its scope.
    let s = AppState::default();
    let settings = build_lane_prompt(&s, LaneKind::Settings);
    let bass = build_lane_prompt(&s, LaneKind::Bass(0));
    assert!(
        settings.contains("Active voices (wired to MASTER):"),
        "Settings lane must include the coverage summary",
    );
    assert!(
        !bass.contains("Active voices (wired to MASTER):"),
        "Non-Settings lanes must not include the coverage summary",
    );
}

// ─── Style hint ─────────────────────────────────────────────────────────────

#[test]
fn no_style_hint_when_no_active_style() {
    // Active style = None → no "STYLE:" line.  A blank line would
    // still occupy tokens and confuse the model.
    let s = AppState::default();
    let prompt = build_lane_prompt(&s, LaneKind::Fx);
    assert!(
        !prompt.contains("\nSTYLE:"),
        "no-style case must NOT emit a STYLE line",
    );
}

#[test]
fn free_style_renders_as_explicit_free_hint() {
    // "__free__" is the sentinel for "let the model pick".  The prompt
    // must surface this rather than silently dropping it, otherwise
    // the model falls back to whatever prior context bled through.
    let mut s = AppState::default();
    s.llm.active_style = Some("__free__".to_string());
    let prompt = build_lane_prompt(&s, LaneKind::Fx);
    assert!(prompt.contains("STYLE: free"));
}

#[test]
fn custom_style_surfaces_the_custom_brief_text() {
    // __custom__ + llm.custom_style_text → STYLE (custom): <text>.
    let mut s = AppState::default();
    s.llm.active_style = Some("__custom__".to_string());
    s.llm.custom_style_text = "dusty lo-fi hip hop with tape wobble".to_string();
    let prompt = build_lane_prompt(&s, LaneKind::Fx);
    assert!(prompt.contains("STYLE (custom):"));
    assert!(prompt.contains("dusty lo-fi hip hop"));
}

// ─── Task footer ────────────────────────────────────────────────────────────

#[test]
fn task_footer_mentions_lane_label() {
    // "You are the {label} writer..." anchor — the model reads this to
    // understand which slice of the synth it's driving.
    let s = AppState::default();
    for lane in [
        LaneKind::Bass(0),
        LaneKind::Bass(1),
        LaneKind::KitA,
        LaneKind::Fx,
        LaneKind::Modulation,
        LaneKind::Rack,
    ] {
        let p = build_lane_prompt(&s, lane);
        let label = lane.label();
        assert!(
            p.contains(label),
            "{lane:?} prompt must mention its label {label:?}",
        );
    }
}

#[test]
fn prompt_warns_against_empty_array_emissions() {
    // Every lane prompt includes the anti-empty-array rule so the model
    // doesn't silence a voice by emitting `[]`.  Losing this rule
    // regressed a recurrent bug in past versions.
    let s = AppState::default();
    for lane in [LaneKind::Bass(0), LaneKind::KitA, LaneKind::Fx] {
        let p = build_lane_prompt(&s, lane);
        assert!(
            p.to_lowercase().contains("empty array") || p.contains("`[]`"),
            "{lane:?} prompt must warn against emitting empty arrays",
        );
    }
}
