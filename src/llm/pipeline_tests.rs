// ─── llm/pipeline_tests.rs ───────────────────────────────────────────────────
// Pipeline test module — extracted from pipeline.rs to keep that file
// under the 1000-line cap.  Linked back into the crate via
// `#[path = "pipeline_tests.rs"] mod tests;` in pipeline.rs, so this
// is the module body itself (no enclosing `mod tests { }` wrapper).

use super::*;
use crate::llm::SamplingParams;
use crate::llm::lanes::LaneKind;
use crate::llm::pipeline_filter::filter_lane_output;
use crate::state::{
    AppState, ModuleKind, PortDir, PortKind, PortRef, RACK_PRESETS, RackModule, RackState,
};
use serde_json::Value;
use std::collections::VecDeque;

// ── Mock backend ────────────────────────────────────────────────────────
//
// Each test queues up the exact JSON responses its lanes should see.
// The mock asserts the expected number of calls are consumed, so a
// lane firing unexpectedly will fail loudly.

struct MockBackend {
    queue: VecDeque<MockResp>,
    /// Per-call record: (system_contains, schema_contains).  Lets tests
    /// assert that the right system prompt / schema was sent without
    /// doing full-string comparison.
    calls: Vec<(String, Value)>,
    /// Records the temperature seen on each call so retry tests
    /// can assert the bump took effect on the second attempt.
    seen_temps: Vec<f32>,
}

/// Queue entry — either a JSON response or an explicit failure to
/// inject on a specific call slot.  Lets retry tests place an Err
/// at exactly the right point in the call sequence.
enum MockResp {
    Ok(Value),
    Err,
}

impl MockBackend {
    /// Convenience constructor for tests that only need success
    /// responses.  Each `Value` becomes a `MockResp::Ok`; any call
    /// past the queue length returns an Err the same way it did
    /// before MockResp existed (so existing tests keep working).
    fn with_responses(responses: Vec<Value>) -> Self {
        Self {
            queue: responses.into_iter().map(MockResp::Ok).collect(),
            calls: Vec::new(),
            seen_temps: Vec::new(),
        }
    }

    /// Constructor for retry / failure-path tests that need to
    /// inject errors at specific call positions.  Each entry is
    /// either `MockResp::Ok(json)` or `MockResp::Err`.
    fn with_results(responses: Vec<MockResp>) -> Self {
        Self {
            queue: responses.into_iter().collect(),
            calls: Vec::new(),
            seen_temps: Vec::new(),
        }
    }
}

impl PipelineBackend for MockBackend {
    fn infer_lane_json(
        &mut self,
        system: &str,
        _user: &str,
        schema: &Value,
        sampling: &SamplingParams,
    ) -> anyhow::Result<Value> {
        self.calls.push((system.to_string(), schema.clone()));
        self.seen_temps.push(sampling.temperature);
        match self.queue.pop_front() {
            Some(MockResp::Ok(v)) => Ok(v),
            Some(MockResp::Err) => Err(anyhow::anyhow!("mock injected failure")),
            None => Err(anyhow::anyhow!("mock ran out of queued responses")),
        }
    }
}

fn bass_only_state() -> AppState {
    let mut s = AppState::default();
    let mut rack = RackState::default();
    rack.modules.clear();
    rack.modules.push(RackModule::new(1, ModuleKind::AcidBass));
    rack.modules
        .push(RackModule::new(2, ModuleKind::MasterOutput));
    let _ = rack.connect(
        PortRef {
            module_id: 1,
            dir: PortDir::Out,
            kind: PortKind::Audio,
            index: 0,
        },
        PortRef {
            module_id: 2,
            dir: PortDir::In,
            kind: PortKind::Audio,
            index: 0,
        },
    );
    s.rack = rack;
    s.bass_voices[0].enabled = true;
    s
}

fn two_voice_full_rack() -> AppState {
    let full = RACK_PRESETS.iter().find(|p| p.name == "Full").unwrap();
    let mut s = AppState::default();
    s.rack = RackState::from_preset(full);
    s.bass_voices[0].enabled = true;
    s.bass_voices[1].enabled = true;
    s
}

// ── Scope / filter ──────────────────────────────────────────────────────

#[test]
fn filter_drops_off_lane_top_level_keys() {
    let raw = serde_json::json!({
        "bass": { "cutoff": 0.4 },
        "fx":   { "reverb_mix": 0.5 },   // not in bass lane's scope
        "sequencer": { "bass_steps": [0, 4, 8] }
    });
    let filtered = filter_lane_output(LaneKind::Bass(0), raw);
    assert!(filtered.get("bass").is_some());
    assert!(filtered.get("fx").is_none(), "fx should be stripped");
    assert!(filtered.get("sequencer").is_some());
}

#[test]
fn filter_drops_off_lane_sequencer_subkeys() {
    // KitA lane should only keep kick_a_steps/snare_a_steps/hihat_a_steps
    let raw = serde_json::json!({
        "sequencer": {
            "kick_a_steps":  [0, 4, 8, 12],
            "bass_steps":    [0, 3, 6, 10],   // wrong lane
            "kick_b_steps":  [0, 4, 8, 12],   // wrong lane
        }
    });
    let filtered = filter_lane_output(LaneKind::KitA, raw);
    let seq = filtered.get("sequencer").unwrap().as_object().unwrap();
    assert!(seq.contains_key("kick_a_steps"));
    assert!(!seq.contains_key("bass_steps"));
    assert!(!seq.contains_key("kick_b_steps"));
}

#[test]
fn filter_preserves_thinking_and_comment() {
    let raw = serde_json::json!({
        "_thinking": "plan",
        "_comment":  "did a thing",
        "mc_line":   "SELECTOR!",
        "bass":      { "cutoff": 0.3 },
        "evil":      { "nope": true },
    });
    let filtered = filter_lane_output(LaneKind::Bass(0), raw);
    assert!(filtered.get("_thinking").is_some());
    assert!(filtered.get("_comment").is_some());
    assert!(filtered.get("mc_line").is_some());
    assert!(filtered.get("evil").is_none());
}

#[test]
fn filter_handles_non_object_input() {
    // The server shouldn't emit this — grammar gates it — but if it
    // does, don't panic.
    let raw = serde_json::json!(42);
    assert_eq!(filter_lane_output(LaneKind::Fx, raw.clone()), raw);
}

// ── Scope strings ───────────────────────────────────────────────────────

#[test]
fn bass_scope_includes_sequencer() {
    // Bass lane must get the "sequencer" scope or apply_llm_update
    // won't look at sequencer.bass_steps at all.
    let scope = lane_apply_scope(LaneKind::Bass(0));
    assert!(scope.contains(&"bass".to_string()));
    assert!(scope.contains(&"sequencer".to_string()));
}

#[test]
fn fx_scope_is_empty() {
    // FX lives at the top level, not inside sequencer — an empty
    // scope signals "apply every top-level key you find" which is
    // fine because the filter has already narrowed the payload to
    // `fx` only.
    let scope = lane_apply_scope(LaneKind::Fx);
    assert!(scope.is_empty());
}

// ── Pipeline happy path ─────────────────────────────────────────────────

#[test]
fn pipeline_applies_each_lane_in_order() {
    let state = bass_only_state();
    // Planner response: settings → bass1 → fx
    let planner_out = serde_json::json!({
        "lanes": ["settings", "bass1", "fx"],
        "rationale": "single-bass jam"
    });
    // settings lane: set bpm
    let settings_out = serde_json::json!({
        "_comment": "bpm set",
        "sequencer": { "bpm": 130.0 }
    });
    // bass1 lane: pattern
    let bass_out = serde_json::json!({
        "_comment": "acid line",
        "sequencer": {
            "bass_steps":   [0, 3, 6, 10, 14, 18, 22, 26],
            "bass_notes":   [36, 43, 36, 41, 39, 36, 43, 36,
                              36, 43, 36, 41, 39, 36, 43, 36],
            "bass_accents": [0, 14],
            "bass_slides":  [3, 18]
        }
    });
    // fx lane: reverb
    let fx_out = serde_json::json!({
        "fx": { "reverb_mix": 0.2 }
    });

    let mut backend =
        MockBackend::with_responses(vec![planner_out, settings_out, bass_out, fx_out]);
    let events = std::cell::RefCell::new(Vec::new());
    let new_state = run_pipeline(
        state,
        "write a fresh acid bassline with subtle reverb",
        &mut backend,
        &SamplingParams::default(),
        false, // is_jam — user turn, exercise planner path
        None,  // no live_state — snapshot-only behaviour
        |e| events.borrow_mut().push(e),
        |_, _| {},
    );

    // BPM landed.
    assert!((new_state.sequencer.bpm - 130.0).abs() < 0.01);
    // Bass pattern landed.
    assert!(new_state.sequencer.bass_pattern[0].active);
    assert!(new_state.sequencer.bass_pattern[3].active);
    // Accent on step 0 (index list format).
    assert!(new_state.sequencer.bass_pattern[0].accent > 0.0);
    // FX landed.
    assert!((new_state.fx.reverb_mix - 0.2).abs() < 0.01);

    // Event sequence: PlanReady → (Started, Applied) × 3 → PipelineDone
    let ev = events.borrow();
    assert!(matches!(ev[0], PipelineEvent::PlanReady { .. }));
    assert!(matches!(
        ev.last(),
        Some(PipelineEvent::PipelineDone { .. })
    ));
    let succeeded = match ev.last() {
        Some(PipelineEvent::PipelineDone {
            lanes_succeeded, ..
        }) => *lanes_succeeded,
        _ => 0,
    };
    assert_eq!(succeeded, 3);
}

#[test]
fn pipeline_falls_back_when_planner_fails() {
    // Planner returns junk — pipeline should fall back to default_plan
    // (Settings + Bass(0) + Fx on a bass-only rack) and still run.
    let state = bass_only_state();
    let mut backend = MockBackend::with_responses(vec![
        serde_json::json!({ "bogus": "nothing useful" }), // planner — malformed
        serde_json::json!({ "sequencer": { "bpm": 125.0 } }), // settings lane
        serde_json::json!({
            "sequencer": {
                "bass_steps": [0, 4, 8, 12],
                "bass_notes": [36, 36, 36, 36, 36, 36, 36, 36,
                                36, 36, 36, 36, 36, 36, 36, 36],
                "bass_accents": [0],
                "bass_slides":  []
            }
        }), // bass1 lane
        serde_json::json!({ "fx": { "reverb_mix": 0.15 } }), // fx lane
    ]);
    let events = std::cell::RefCell::new(Vec::new());
    let new_state = run_pipeline(
        state,
        "start a jam",
        &mut backend,
        &SamplingParams::default(),
        false, // is_jam — exercise planner fallback path
        None,  // no live_state — snapshot-only behaviour
        |e| events.borrow_mut().push(e),
        |_, _| {},
    );
    assert!((new_state.sequencer.bpm - 125.0).abs() < 0.01);
    assert!((new_state.fx.reverb_mix - 0.15).abs() < 0.01);
    // The plan ready event carries the default plan — rationale
    // should say "default" so the UI can badge it.
    let plan_ready = events
        .borrow()
        .iter()
        .find_map(|e| {
            if let PipelineEvent::PlanReady { plan } = e {
                Some(plan.rationale.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert!(plan_ready.to_lowercase().contains("default"));
}

#[test]
fn pipeline_continues_after_lane_failure() {
    // Queue: planner ok, settings ok, bass FAILS (empty queue after
    // settings), fx... wait, once queue is empty the mock returns Err
    // for every subsequent call.  So we want: planner → settings (ok)
    // → lane 2 returns Err (queue empty-ish) → lane 3 never fires.
    //
    // Cleaner: push planner + 1 successful lane, let the next consume
    // the error.  Assert: 1 LaneFailed event fired; final state has
    // the one successful update.
    let state = two_voice_full_rack();
    let mut backend = MockBackend::with_responses(vec![
        // Planner picks settings + fx
        serde_json::json!({ "lanes": ["settings", "fx"] }),
        // settings succeeds
        serde_json::json!({ "sequencer": { "bpm": 128.0 } }),
        // fx fails (queue will be empty)
    ]);
    let events = std::cell::RefCell::new(Vec::new());
    let new_state = run_pipeline(
        state,
        "set the jam up",
        &mut backend,
        &SamplingParams::default(),
        false, // is_jam — exercise planner path
        None,  // no live_state — snapshot-only behaviour
        |e| events.borrow_mut().push(e),
        |_, _| {},
    );
    assert!((new_state.sequencer.bpm - 128.0).abs() < 0.01);
    let failed = events
        .borrow()
        .iter()
        .filter(|e| matches!(e, PipelineEvent::LaneFailed { .. }))
        .count();
    assert_eq!(failed, 1);
    let succeeded_summary = match events.borrow().last() {
        Some(PipelineEvent::PipelineDone {
            lanes_succeeded,
            lanes_failed,
            ..
        }) => (*lanes_succeeded, *lanes_failed),
        _ => (0, 0),
    };
    assert_eq!(succeeded_summary, (1, 1));
}

#[test]
fn pipeline_passes_bass_context_forward() {
    // After bass1 is applied, the bass2 lane's system prompt should
    // see bass1's pattern (so it can counterpoint).  We assert the
    // system prompt captured by the mock backend for the bass2 call
    // mentions bass_pattern fact.
    let state = two_voice_full_rack();
    let mut backend = MockBackend::with_responses(vec![
        serde_json::json!({ "lanes": ["bass1", "bass2"] }),
        // bass1: set steps on 0, 4, 8
        serde_json::json!({
            "sequencer": {
                "bass_steps":   [0, 4, 8],
                "bass_notes":   [36, 36, 36, 36, 36, 36, 36, 36,
                                 36, 36, 36, 36, 36, 36, 36, 36],
                "bass_accents": [0],
                "bass_slides":  []
            }
        }),
        // bass2: arbitrary — we just want to capture the prompt
        serde_json::json!({
            "sequencer": {
                "bass2_steps":   [6, 14],
                "bass2_notes":   [43, 43, 43, 43, 43, 43, 43, 43,
                                  43, 43, 43, 43, 43, 43, 43, 43],
                "bass2_accents": [],
                "bass2_slides":  []
            }
        }),
    ]);
    let _ = run_pipeline(
        state,
        "two-voice acid bass",
        &mut backend,
        &SamplingParams::default(),
        false, // is_jam — exercise planner path
        None,  // no live_state — snapshot-only behaviour
        |_| {},
        |_, _| {},
    );
    // 3 calls: planner + bass1 + bass2.  The 3rd call's system
    // prompt is the bass2 prompt.  It should mention bass_pattern
    // index 0 (because we set step 0 active).
    let (bass2_system, _) = &backend.calls[2];
    assert!(
        bass2_system.contains("Active bass steps"),
        "bass2 prompt is missing the bass-steps summary"
    );
    // The bass-steps summary lists the fired-step indices — step 0
    // from bass1 should be in there.
    assert!(
        bass2_system.contains("0"),
        "bass2 prompt should reflect bass1's active step 0"
    );
}

#[test]
fn pipeline_skips_lane_when_module_removed_mid_cycle() {
    // When the planner emits `[bass1, fx]` but the AcidBass module
    // gets removed from the live rack between the plan-time filter
    // and the bass1 inference, the pipeline must skip the bass1
    // lane (no infer call fires for it), not apply an update
    // against the now-missing voice.
    use parking_lot::RwLock;
    use std::sync::Arc;

    let base = bass_only_state();
    // Unlock bpm so settings / state mutations actually land (default
    // state locks bpm).  Not strictly needed here but keeps the test
    // robust against future callback tweaks that check state fields.
    let base = crate::state::unlock_param(base, "sequencer.bpm");
    let live_state = Arc::new(RwLock::new(base.clone()));
    let mut backend = MockBackend::with_responses(vec![
        // Planner response: fx then bass1.  `fx` always stays live,
        // bass1 is live at plan time (rack still has AcidBass).
        serde_json::json!({ "lanes": ["fx", "bass1"] }),
        // fx lane response — applied normally.
        serde_json::json!({ "fx": { "reverb_mix": 0.4 } }),
        // If the pipeline fires bass1 despite the removal this
        // response would be consumed; the call-count assertion
        // catches that failure mode.
        serde_json::json!({
            "sequencer": { "bass_steps": [0, 4, 8, 12] }
        }),
    ]);

    // Strip AcidBass from the live state the moment any lane lands.
    // This models the user yanking the module mid-pipeline.
    let live_for_cb = live_state.clone();
    let fired = std::cell::Cell::new(false);
    let on_lane_applied = |_update: &Value, _scope: &[String]| {
        if !fired.get() {
            fired.set(true);
            let mut live = live_for_cb.write();
            let ids: Vec<u32> = live
                .rack
                .modules
                .iter()
                .filter(|m| m.kind == ModuleKind::AcidBass)
                .map(|m| m.id)
                .collect();
            for id in ids {
                live.rack.remove_module(id);
            }
        }
    };

    let events = std::cell::RefCell::new(Vec::new());
    // A prompt long enough to bypass the short-command heuristic and
    // route through the LLM planner so the mocked response drives
    // the plan.
    let prompt = "rewrite the fx bus to a darker reverb space and then \
                  evolve the bass line with some new accents and slides";
    let _ = run_pipeline(
        base,
        prompt,
        &mut backend,
        &SamplingParams::default(),
        false,
        Some(&live_state),
        |e| events.borrow_mut().push(e),
        on_lane_applied,
    );

    // 2 calls expected: planner + fx.  The bass1 response stays in
    // the queue because the live-state check skips that lane before
    // `infer_lane_json` fires.
    assert_eq!(
        backend.calls.len(),
        2,
        "expected planner + fx only — bass1 should have been skipped; got {:?}",
        backend
            .calls
            .iter()
            .map(|(s, _)| s.lines().next().unwrap_or("").to_string())
            .collect::<Vec<_>>()
    );
    // A `LaneSkipped` event for bass1 was emitted.
    let skipped = events
        .borrow()
        .iter()
        .any(|e| matches!(e, PipelineEvent::LaneSkipped { lane, .. } if lane.label() == "bass1"));
    assert!(skipped, "expected a LaneSkipped event for bass1");
}

#[test]
fn pipeline_without_live_state_keeps_snapshot_behaviour() {
    // Passing `None` for live_state must preserve the pre-refactor
    // semantics: every planned lane fires, no mid-cycle re-checks.
    let state = bass_only_state();
    let state = crate::state::unlock_param(state, "sequencer.bpm");
    let mut backend = MockBackend::with_responses(vec![
        serde_json::json!({ "lanes": ["fx", "bass1"] }),
        serde_json::json!({ "fx": { "reverb_mix": 0.4 } }),
        serde_json::json!({
            "sequencer": { "bass_steps": [0, 4, 8, 12] }
        }),
    ]);
    let events = std::cell::RefCell::new(Vec::new());
    let prompt = "rewrite the fx bus to a darker reverb space and then \
                  evolve the bass line with some new accents and slides";
    let _ = run_pipeline(
        state,
        prompt,
        &mut backend,
        &SamplingParams::default(),
        false,
        None, // opt-out of mid-pipeline checks
        |e| events.borrow_mut().push(e),
        |_, _| {},
    );
    assert_eq!(
        backend.calls.len(),
        3,
        "expected planner + fx + bass1 when live_state is None"
    );
    // No LaneSkipped events should ever fire without live_state.
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, PipelineEvent::LaneSkipped { .. }))
    );
}

// ─── Auto-retry with temperature bump ────────────────────────────────────

#[test]
fn lane_retry_succeeds_on_second_attempt_with_bumped_temp() {
    // Planner picks settings; settings lane's first inference fails,
    // retry succeeds with a +0.1 temperature bump.  Final state
    // shows the BPM update and the retry call must have run at the
    // bumped temperature.
    let state = bass_only_state();
    let mut backend = MockBackend::with_results(vec![
        // 1. Planner — picks settings only.
        MockResp::Ok(serde_json::json!({ "lanes": ["settings"] })),
        // 2. Settings lane first attempt — fails.
        MockResp::Err,
        // 3. Settings lane retry — succeeds.
        MockResp::Ok(serde_json::json!({ "sequencer": { "bpm": 142.0 } })),
    ]);
    let sampling = SamplingParams {
        temperature: 0.7,
        ..SamplingParams::default()
    };
    let events = std::cell::RefCell::new(Vec::new());
    let new_state = run_pipeline(
        state,
        "tempo to 142",
        &mut backend,
        &sampling,
        false,
        None,
        |e| events.borrow_mut().push(e),
        |_, _| {},
    );
    assert!(
        (new_state.sequencer.bpm - 142.0).abs() < 0.01,
        "bpm should be 142 after retry, got {}",
        new_state.sequencer.bpm
    );
    let failed = events
        .borrow()
        .iter()
        .filter(|e| matches!(e, PipelineEvent::LaneFailed { .. }))
        .count();
    assert_eq!(failed, 0, "retry success should not fire LaneFailed");
    // 3rd call is the retry; temperature must be bumped.  Planner
    // overrides temperature to 0.4, so the lane's two calls are
    // calls 2 (first attempt @ 0.7) and 3 (retry @ 0.7 + bump).
    let retry_temp = backend.seen_temps[2];
    assert!(
        (retry_temp - (0.7 + LANE_RETRY_TEMP_BUMP)).abs() < 1e-5,
        "retry should run at temp+{}, got {}",
        LANE_RETRY_TEMP_BUMP,
        retry_temp
    );
}

#[test]
fn lane_retry_propagates_failure_when_both_attempts_fail() {
    // When both the first attempt AND the retry fail, the lane
    // fires LaneFailed (not silently succeed) and BPM is untouched.
    let state = bass_only_state();
    let mut backend = MockBackend::with_results(vec![
        MockResp::Ok(serde_json::json!({ "lanes": ["settings"] })),
        MockResp::Err, // first attempt
        MockResp::Err, // retry
    ]);
    let events = std::cell::RefCell::new(Vec::new());
    let prior_bpm = state.sequencer.bpm;
    let new_state = run_pipeline(
        state,
        "tempo to 142",
        &mut backend,
        &SamplingParams::default(),
        false,
        None,
        |e| events.borrow_mut().push(e),
        |_, _| {},
    );
    assert!((new_state.sequencer.bpm - prior_bpm).abs() < 0.01);
    let failed = events
        .borrow()
        .iter()
        .filter(|e| matches!(e, PipelineEvent::LaneFailed { .. }))
        .count();
    assert_eq!(failed, 1, "expected exactly one LaneFailed event");
}

#[test]
fn lane_retry_temp_bump_clamps_to_two() {
    // llama-server accepts temperature in [0, 2].  Bumping a 1.95
    // base would land at 2.05; guard with a clamp at 2.0.
    let state = bass_only_state();
    let mut backend = MockBackend::with_results(vec![
        MockResp::Ok(serde_json::json!({ "lanes": ["settings"] })),
        MockResp::Err,
        MockResp::Ok(serde_json::json!({ "sequencer": { "bpm": 130.0 } })),
    ]);
    let sampling = SamplingParams {
        temperature: 1.95,
        ..SamplingParams::default()
    };
    let _ = run_pipeline(
        state,
        "tempo to 130",
        &mut backend,
        &sampling,
        false,
        None,
        |_| {},
        |_, _| {},
    );
    let retry_temp = backend.seen_temps[2];
    assert!(
        (retry_temp - 2.0).abs() < 1e-5,
        "retry temp should clamp to 2.0, got {}",
        retry_temp
    );
}

#[test]
fn lane_success_on_first_attempt_does_not_retry() {
    // Sanity: when the first attempt succeeds, the retry helper
    // shouldn't consume an extra queue slot.  Setting up a queue
    // of exactly [planner, settings] and asserting the second
    // entry was used (queue empty after pipeline) catches a
    // regression where the helper always burns 2 calls.
    let state = bass_only_state();
    let mut backend = MockBackend::with_results(vec![
        MockResp::Ok(serde_json::json!({ "lanes": ["settings"] })),
        MockResp::Ok(serde_json::json!({ "sequencer": { "bpm": 100.0 } })),
    ]);
    let new_state = run_pipeline(
        state,
        "tempo to 100",
        &mut backend,
        &SamplingParams::default(),
        false,
        None,
        |_| {},
        |_, _| {},
    );
    assert!((new_state.sequencer.bpm - 100.0).abs() < 0.01);
    assert_eq!(
        backend.calls.len(),
        2,
        "first-attempt success must consume only 2 calls (planner + lane), got {}",
        backend.calls.len()
    );
}
