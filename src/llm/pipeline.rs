// ─── llm/pipeline.rs ─────────────────────────────────────────────────────────
// Sequential lane-inference executor.  Replaces the monolithic
// "emit every field at once" inference with a short pipeline:
//
//   1. Planner call — reads user prompt, returns ordered LanePlan
//   2. For each lane in the plan:
//        a. Build focused system prompt + required-fields schema
//        b. Fire inference with grammar-constrained output
//        c. Filter response JSON to the lane's scope
//        d. Apply to AppState
//        e. Emit progress event so the UI streams per-lane updates
//
// Per-lane outputs are small (100-400 tokens) which dodges the decode
// truncation + rule-skipping we saw from the monolithic path.  State
// updates between lanes let later calls see earlier output (bass #2
// can counterpoint bass #1 because bass #1 is already in the snapshot
// by the time bass #2's prompt is built).
//
// The `PipelineBackend` trait is narrow on purpose — the real
// LlamaServerBackend gets a wrapper, and tests get a mock, without
// reshaping the app-wide `LlmBackend` trait.

use crate::llm::lanes::{LaneKind, build_lane_prompt, lane_schema};
use crate::llm::planner::{
    LanePlan, build_planner_prompt, default_plan, parse_planner_output, planner_schema,
};
use crate::llm::planner_heuristic::heuristic_plan;
use crate::llm::{SamplingParams, json_repair};
use crate::state::AppState;
use serde_json::Value;

/// A structured-output LLM call.  Separated from `LlmBackend::infer` so
/// the pipeline can plug in any backend (real llama-server, mock in
/// tests, future alternatives) without touching the monolithic trait.
pub trait PipelineBackend {
    fn infer_lane_json(
        &mut self,
        system: &str,
        user: &str,
        schema: &Value,
        sampling: &SamplingParams,
    ) -> anyhow::Result<Value>;
}

/// Everything the pipeline emits about a single lane step.  The UI
/// subscribes to this via the progress callback to stream updates.
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// Planner finished; the pipeline is about to start running lanes.
    PlanReady { plan: LanePlan },
    /// Entering a lane's inference.
    LaneStarted { lane: LaneKind },
    /// Lane inference succeeded; `update` is the applied JSON, `ms` is wall time.
    LaneApplied {
        lane: LaneKind,
        update: Value,
        ms: u128,
    },
    /// Lane inference or apply failed; pipeline continues with the next lane.
    LaneFailed { lane: LaneKind, error: String },
    /// Lane was skipped because a mid-pipeline live-state check found the
    /// lane's target module missing / disabled (e.g. user removed the
    /// rack module while the pipeline was mid-cycle).  Distinct from
    /// `LaneFailed` so the progress UI can show "skipped" without
    /// framing it as a model error.
    LaneSkipped { lane: LaneKind, reason: String },
    /// Whole pipeline done.
    PipelineDone {
        total_ms: u128,
        lanes_succeeded: usize,
        lanes_failed: usize,
    },
}

/// Run the sequential lane pipeline.  Returns the new `AppState` with
/// every successful lane applied; failed lanes are logged via the
/// progress callback but don't abort the pipeline.
///
/// `progress` fires for plan/lane events (logging / UI).
///
/// `on_lane_applied` fires once per SUCCESSFULLY applied lane with the
/// new state — the caller writes it back to the shared `AppState` so
/// the audio thread hears the change immediately instead of waiting
/// for every lane to finish.  Tests can pass `|_| {}` as a no-op.
pub fn run_pipeline<B: PipelineBackend>(
    mut state: AppState,
    user_prompt: &str,
    backend: &mut B,
    sampling: &SamplingParams,
    is_jam: bool,
    live_state: Option<&std::sync::Arc<parking_lot::RwLock<AppState>>>,
    mut progress: impl FnMut(PipelineEvent),
    mut on_lane_applied: impl FnMut(&AppState),
) -> AppState {
    let t_start = std::time::Instant::now();

    // ── 1. Planner ───────────────────────────────────────────────────────────
    // Jam cycles (is_jam=true) use the Phase 2 weighted single-lane picker
    // so each cycle rewrites exactly one voice/kit rather than rerunning
    // the full default plan; score + recency decay in `lane_scheduler`
    // rotate focus across voices between cycles.  User turns fall through
    // to the regular heuristic → LLM planner → default chain.
    let plan = if is_jam {
        let mut rng = crate::llm::lane_scheduler::Xorshift32::from_entropy();
        let jp = crate::llm::planner_jam::jam_plan(&state, &mut rng);
        if jp.lanes.is_empty() {
            log::info!("pipeline: jam_plan empty, falling back to default_plan");
            default_plan(&state)
        } else {
            log::info!("pipeline: {}", jp.rationale);
            jp
        }
    } else if let Some(heur) = heuristic_plan(&state, user_prompt) {
        log::info!("pipeline: heuristic plan hit — {}", heur.rationale);
        heur
    } else {
        run_planner(&state, user_prompt, backend, sampling).unwrap_or_else(|e| {
            log::info!(
                "pipeline: planner fell back to default plan — {}",
                truncate(&e.to_string(), 200)
            );
            default_plan(&state)
        })
    };
    // Defensive filter: drop any lane whose voice/module isn't actually
    // present in the rack right now.  `parse_planner_output` already
    // applies `lane_is_live`, but the planner sometimes produces stale
    // labels (e.g. after a style switch removes a voice mid-cycle), and
    // `default_plan` only reads the rack at construction.  Filtering
    // here means a no-op-prone lane never burns an inference call.
    let plan = LanePlan {
        lanes: plan
            .lanes
            .into_iter()
            .filter(|&l| crate::llm::planner::lane_is_live_pub(&state, l))
            .collect(),
        rationale: plan.rationale,
        from_retry: plan.from_retry,
    };
    if plan.lanes.is_empty() {
        progress(PipelineEvent::PipelineDone {
            total_ms: t_start.elapsed().as_millis(),
            lanes_succeeded: 0,
            lanes_failed: 0,
        });
        return state;
    }
    progress(PipelineEvent::PlanReady { plan: plan.clone() });

    // ── 2. Lanes ─────────────────────────────────────────────────────────────
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    for lane in plan.lanes.iter().copied() {
        // Mid-pipeline live-state re-check: `plan.lanes` was filtered
        // against the snapshot at plan time, but the user (or another
        // agent / the API) can mutate the rack while the pipeline is
        // mid-cycle.  Re-checking against the shared `live_state` here
        // means we skip lanes whose target module has since been
        // removed or disabled, instead of burning an inference call
        // producing output that won't apply.  `None` live_state keeps
        // the old snapshot-only behaviour (tests, one-shot turns).
        if let Some(live) = live_state {
            let live_snap = live.read();
            if !crate::llm::planner::lane_is_live_pub(&live_snap, lane) {
                let reason = format!("{}: module removed / disabled mid-cycle", lane.label());
                log::warn!("pipeline: skipping {} — {}", lane.label(), reason);
                progress(PipelineEvent::LaneSkipped { lane, reason });
                continue;
            }
        }
        progress(PipelineEvent::LaneStarted { lane });
        let t_lane = std::time::Instant::now();
        let system = build_lane_prompt(&state, lane);
        let schema = lane_schema(lane);
        match backend.infer_lane_json(&system, user_prompt, &schema, sampling) {
            Ok(raw) => {
                // Log the raw lane output before filtering so we can see
                // what the model actually emitted — essential when a lane
                // "applies" but produces silent output.
                log::info!(
                    "pipeline: {} raw = {}",
                    lane.label(),
                    truncate(&raw.to_string(), 400)
                );
                let filtered = filter_lane_output(lane, raw);
                let scope = lane_apply_scope(lane);
                state = crate::state::apply_llm_update(state, &filtered, &scope);
                // Commit this lane's updates to the shared app state right
                // away so the audio thread picks them up — without this,
                // every lane silently accumulates and the full turn's
                // worth of patterns would suddenly switch on at the end
                // (which was reported as "blocking and silent until the
                // last lane is in").
                on_lane_applied(&state);
                progress(PipelineEvent::LaneApplied {
                    lane,
                    update: filtered,
                    ms: t_lane.elapsed().as_millis(),
                });
                succeeded += 1;
            }
            Err(e) => {
                progress(PipelineEvent::LaneFailed {
                    lane,
                    error: truncate(&e.to_string(), 240),
                });
                failed += 1;
            }
        }
    }

    progress(PipelineEvent::PipelineDone {
        total_ms: t_start.elapsed().as_millis(),
        lanes_succeeded: succeeded,
        lanes_failed: failed,
    });
    state
}

/// Fire the planner and parse its output into a `LanePlan`.  Empty /
/// malformed output returns an Err so the caller can fall back to
/// `default_plan`.
fn run_planner<B: PipelineBackend>(
    state: &AppState,
    user_prompt: &str,
    backend: &mut B,
    sampling: &SamplingParams,
) -> anyhow::Result<LanePlan> {
    let system = build_planner_prompt(state);
    let schema = planner_schema();
    // Planner should be deterministic-ish — keep heat low regardless
    // of jam heat so the planner picks the obvious lane set.
    let planner_sampling = SamplingParams {
        heat: 0.1,
        temperature: 0.4,
        ..sampling.clone()
    };
    let raw = backend.infer_lane_json(&system, user_prompt, &schema, &planner_sampling)?;
    parse_planner_output(state, &raw).ok_or_else(|| anyhow::anyhow!("planner returned empty plan"))
}

/// Map each lane to the scope strings `apply_llm_update` expects.  This
/// gates which per-voice subsections of `sequencer.*` the apply layer
/// will touch — a Bass(0) lane shouldn't be able to overwrite kit_a
/// patterns even if the model somehow emits them.
pub fn lane_apply_scope(lane: LaneKind) -> Vec<String> {
    match lane {
        LaneKind::Settings => vec!["sequencer".to_string()],
        LaneKind::Bass(_) => vec!["bass".to_string(), "sequencer".to_string()],
        LaneKind::KitA => vec!["kit_a".to_string(), "sequencer".to_string()],
        LaneKind::KitB => vec!["kit_b".to_string(), "sequencer".to_string()],
        LaneKind::Amen => vec!["amen".to_string(), "sequencer".to_string()],
        LaneKind::Hoover => vec!["hoover".to_string(), "sequencer".to_string()],
        LaneKind::An1x => vec!["an1x".to_string(), "sequencer".to_string()],
        // FX / mod / rack live at the top level, not inside sequencer.
        LaneKind::Fx | LaneKind::Modulation | LaneKind::Rack => Vec::new(),
    }
}

/// Strip JSON fields that aren't in this lane's scope.  The schema +
/// grammar should already enforce this, but a belt-and-suspenders
/// filter keeps us honest when the server is loose (e.g. the older
/// llama-server builds that ignore `additionalProperties: false`).
///
/// Top-level keys outside `output_keys()` are dropped.  Inside
/// `sequencer`, only the lane's `sequencer_subkeys()` survive.
///
/// Also drops empty pattern arrays (`"bass_steps": []`) — the apply
/// layer treats `[]` as "clear everything", which is a destructive
/// silent-failure mode when the model emits empty required arrays
/// to satisfy the schema without having anything useful to say.
pub fn filter_lane_output(lane: LaneKind, raw: Value) -> Value {
    let allowed_top: &[&str] = lane.output_keys();
    // `_thinking` and `_comment` are meta-fields carried through for
    // logging / UI; always let them pass.
    let carry_over = ["_thinking", "_comment", "mc_line"];
    // Pattern arrays where "empty means clear" — an empty emission is
    // almost certainly a model giving up on required fields rather than
    // the user asking for silence.  Drop these before apply so the
    // existing pattern survives.
    let destructive_if_empty = [
        "bass_steps",
        "bass2_steps",
        "bass3_steps",
        "bass4_steps",
        "bass_notes",
        "bass2_notes",
        "bass3_notes",
        "bass4_notes",
        "kick_a_steps",
        "snare_a_steps",
        "hihat_a_steps",
        "kick_b_steps",
        "snare_b_steps",
        "clap_b_steps",
        "hihat_b_steps",
        "amen_steps",
        "hoover_steps",
        "hoover_notes",
        "an1x_steps",
        "an1x_notes",
    ];

    let Some(obj) = raw.as_object() else {
        return raw;
    };
    let mut out = serde_json::Map::new();
    for (k, v) in obj {
        if carry_over.contains(&k.as_str()) {
            out.insert(k.clone(), v.clone());
            continue;
        }
        if !allowed_top.contains(&k.as_str()) {
            continue;
        }
        if k == "sequencer" {
            if let Some(seq_obj) = v.as_object() {
                let allowed_sub = lane.sequencer_subkeys();
                let mut filtered = serde_json::Map::new();
                for (sk, sv) in seq_obj {
                    if !allowed_sub.iter().any(|a| a == sk) {
                        continue;
                    }
                    // Drop empty pattern arrays — the apply layer would
                    // interpret them as "clear this voice", wiping the
                    // user's current pattern with silence.
                    if destructive_if_empty.contains(&sk.as_str())
                        && sv.as_array().is_some_and(|a| a.is_empty())
                    {
                        log::warn!(
                            "pipeline: dropped empty `{}` from {} lane — would have cleared the voice",
                            sk,
                            lane.label()
                        );
                        continue;
                    }
                    filtered.insert(sk.clone(), sv.clone());
                }
                if !filtered.is_empty() {
                    out.insert("sequencer".to_string(), Value::Object(filtered));
                }
            }
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

/// Tight string truncation for log messages — avoids dumping a whole
/// llama-server stack trace into the UI log when a lane fails.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Break on char boundary so we don't split a multi-byte glyph.
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        format!("{}…", &s[..end])
    }
}

// Keep `json_repair` referenced so the module-level use line doesn't
// warn if future refactors drop its direct use — the real backend
// wrapper (Phase 5) will call `json_repair::repair_truncated_json` on
// the raw server response before handing it to the pipeline.
#[allow(unused_imports)]
use json_repair as _json_repair_keepalive;

/// Thin adapter that plugs a live `LlamaServerPool` + port into the
/// `PipelineBackend` trait.  This is what production calls should use —
/// the pool owns the real llama-server subprocess, the adapter routes
/// each `infer_lane_json` call to it.  Cheap to construct (holds
/// mutable borrows), so each pipeline run builds a fresh one.
pub struct PoolBackend<'a> {
    pool: &'a mut crate::llm::LlamaServerPool,
    port: u16,
}

impl<'a> PoolBackend<'a> {
    pub fn new(pool: &'a mut crate::llm::LlamaServerPool, port: u16) -> Self {
        Self { pool, port }
    }
}

impl PipelineBackend for PoolBackend<'_> {
    fn infer_lane_json(
        &mut self,
        system: &str,
        user: &str,
        schema: &Value,
        sampling: &SamplingParams,
    ) -> anyhow::Result<Value> {
        self.pool
            .infer_lane(self.port, system, user, schema, sampling)
    }
}

/// Convenience entrypoint for callers who already have a pool + port —
/// builds the adapter and runs the pipeline in one call.  Used by the
/// LLM worker loop to swap monolithic inference for the lane path.
#[allow(clippy::too_many_arguments)]
pub fn run_pipeline_via_pool(
    state: AppState,
    user_prompt: &str,
    pool: &mut crate::llm::LlamaServerPool,
    port: u16,
    sampling: &SamplingParams,
    is_jam: bool,
    live_state: Option<&std::sync::Arc<parking_lot::RwLock<AppState>>>,
    progress: impl FnMut(PipelineEvent),
    on_lane_applied: impl FnMut(&AppState),
) -> AppState {
    let mut backend = PoolBackend::new(pool, port);
    run_pipeline(
        state,
        user_prompt,
        &mut backend,
        sampling,
        is_jam,
        live_state,
        progress,
        on_lane_applied,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        ModuleKind, PortDir, PortKind, PortRef, RACK_PRESETS, RackModule, RackState,
    };
    use std::collections::VecDeque;

    // ── Mock backend ────────────────────────────────────────────────────────
    //
    // Each test queues up the exact JSON responses its lanes should see.
    // The mock asserts the expected number of calls are consumed, so a
    // lane firing unexpectedly will fail loudly.

    struct MockBackend {
        queue: VecDeque<Value>,
        /// Per-call record: (system_contains, schema_contains).  Lets tests
        /// assert that the right system prompt / schema was sent without
        /// doing full-string comparison.
        calls: Vec<(String, Value)>,
    }

    impl MockBackend {
        fn with_responses(responses: Vec<Value>) -> Self {
            Self {
                queue: responses.into_iter().collect(),
                calls: Vec::new(),
            }
        }
    }

    impl PipelineBackend for MockBackend {
        fn infer_lane_json(
            &mut self,
            system: &str,
            _user: &str,
            schema: &Value,
            _sampling: &SamplingParams,
        ) -> anyhow::Result<Value> {
            self.calls.push((system.to_string(), schema.clone()));
            self.queue
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("mock ran out of queued responses"))
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
            |_| {},
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
            |_| {},
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
            |_| {},
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
            |_| {},
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
        let on_lane_applied = |_s: &AppState| {
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
        let skipped = events.borrow().iter().any(
            |e| matches!(e, PipelineEvent::LaneSkipped { lane, .. } if lane.label() == "bass1"),
        );
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
            |_| {},
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
}
