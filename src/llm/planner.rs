// ─── llm/planner.rs ──────────────────────────────────────────────────────────
// Lane-plan prompt + parser for the sequential inference pipeline.
//
// A user turn like "make it acid" shouldn't crash the model with eight
// concurrent rules.  The planner is a tiny LLM call that reads the user's
// prompt plus a compact state summary and returns an ordered list of lanes
// to fire:
//
//   "rewrite bass 2"          → [Bass(1)]
//   "acid house"              → [Settings, KitA, KitB, Bass(0), Bass(1), Fx]
//   "add reverb"              → [Fx]
//   "start a jam"             → Settings + every active voice + Fx
//
// When the planner fails, times out, or emits an empty list, the pipeline
// falls back to `default_plan(state)` — a deterministic order based on the
// active rack so the user still gets sensible behaviour.

use crate::llm::lanes::LaneKind;
use crate::state::{AppState, ModuleKind};

/// Output of the planner: an ordered lane list plus a human-readable
/// rationale that gets forwarded to the UI log.
#[derive(Debug, Clone, Default)]
pub struct LanePlan {
    pub lanes: Vec<LaneKind>,
    pub rationale: String,
    /// True when `jam_plan` pulled this lane off `LlmState.retry_queue`
    /// (Phase 3).  `pipeline_events::handle_pipeline_event` reads this on
    /// `PlanReady` so it can pop the matching entry off the shared queue
    /// before the next jam cycle runs.
    pub from_retry: bool,
}

impl LanePlan {
    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }
}

/// Build the planner system prompt.  Deliberately tiny — the planner's
/// only job is to decide which lanes to fire, not to generate patterns.
/// Expected output size is ~50-150 tokens.
pub fn build_planner_prompt(state: &AppState) -> String {
    let active_voices = describe_active_voices(state);
    let style = match state.llm.active_style.as_deref() {
        None => "none",
        Some("__free__") => "free",
        Some("__custom__") => "custom",
        Some(id) => id,
    };

    format!(
        "You are the PLANNER inside Impulse Instruct. Given the user's prompt, \
         return an ordered list of LANES to fire — one focused LLM call per \
         lane writes that slice of the synth.\n\
         \n\
         STATE: style={style} active_voices=[{active_voices}]\n\
         STYLE IS USER-OWNED — never request a lane that changes it.\n\
         \n\
         AVAILABLE LANES:\n\
         • settings  — bpm / swing / key / scale (NOT style — style is user-\
           owned).  Fire first on an initial jam or when bpm/key should change.\n\
         • bass1 / bass2 / bass3 / bass4 — per-voice bass patterns + synth\n\
         • kit_a     — 808 drums (kick_a / snare_a / hihat_a)\n\
         • kit_b     — 909 drums (kick_b / snare_b / clap_b / hihat_b)\n\
         • amen      — amen break sampler\n\
         • hoover    — hoover lead\n\
         • an1x      — AN1X pad / lead\n\
         • fx        — reverb / delay / distortion / chorus / bitcrush\n\
         • mod       — LFOs + free_eg modulation\n\
         • rack      — add / remove / wire modules\n\
         \n\
         RULES:\n\
         1. Only include lanes whose voice/module is ACTIVE (see the \
            active_voices list above). Exception: `settings`, `fx`, `mod` \
            are always available.\n\
         2. Narrow user commands pick ONE lane. `rewrite bass 2` → [bass2]. \
            `add reverb` → [fx]. `change the kick` → [kit_a] or [kit_b].\n\
         3. Broad commands (`start a jam`, `make a track`, \"make a pattern\") \
            pick a FULL jam: drums, bass, fx. Skip `settings` unless the user \
            explicitly asks for a tempo/key change.\n\
         4. BASS VOICES — if ANY bass lane is in the plan, include EVERY \
            bass voice listed in active_voices (bass1, bass2, bass3, bass4 \
            as applicable).  Skipping a voice leaves it silent.  The only \
            exception is a user command that names a single voice \
            (\"rewrite bass 2\" → just [bass2]).\n\
         5. `rack` and `mod` are NICHE — only include them if the user \
            explicitly asks (\"add an 808\", \"wire the delay\", \"add an LFO\"). \
            Never fire them as part of a generic groove.\n\
         6. Order matters — settings (if included) first, drums before bass \
            (bass can reference the kick grid), fx last.\n\
         7. Keep it minimal. Fewer lanes = faster response. Don't fire \
            lanes the user didn't ask for.\n\
         \n\
         Output JSON only: {{\"lanes\": [\"settings\", \"kit_a\", \"bass1\", \"fx\"], \
         \"rationale\": \"one short sentence explaining the plan\"}}",
    )
}

/// JSON schema for the planner output.  Grammar-constrained generation
/// will force the model into this exact shape.
pub fn planner_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema",
        "type": "object",
        "properties": {
            "lanes": {
                "type": "array",
                "minItems": 1,
                "maxItems": 12,
                "items": {
                    "type": "string",
                    "enum": [
                        "settings", "bass1", "bass2", "bass3", "bass4",
                        "kit_a", "kit_b", "amen", "hoover", "an1x",
                        "fx", "mod", "rack"
                    ]
                }
            },
            "rationale": { "type": "string", "maxLength": 200 }
        },
        "required": ["lanes"],
        "additionalProperties": false
    })
}

/// Parse planner JSON output into a `LanePlan`.  Filters out lanes for
/// disabled voices / unwired modules so the pipeline doesn't fire a
/// lane that would be a no-op.  Returns `None` when the lane list is
/// empty after filtering — the caller should fall back to `default_plan`.
///
/// Also auto-expands a bass-containing plan to cover every active bass
/// voice.  The planner LLM has a habit of picking `bass1` alone even
/// when `bass2` is also live; skipping a voice leaves it silent, which
/// is always a musical regression for a multi-voice rack.  A rule in
/// the prompt reinforces this, but we enforce it in code so the
/// pipeline doesn't ship a half-configured bass on a model hiccup.
pub fn parse_planner_output(state: &AppState, json: &serde_json::Value) -> Option<LanePlan> {
    let obj = json.as_object()?;
    let lanes_arr = obj.get("lanes")?.as_array()?;
    let raw_labels: Vec<String> = lanes_arr
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    let mut lanes: Vec<LaneKind> = Vec::new();
    for s in &raw_labels {
        if let Some(lane) = lane_from_label(s)
            && lane_is_live(state, lane)
            && !lanes.contains(&lane)
        {
            lanes.push(lane);
        }
    }
    if lanes.is_empty() {
        return None;
    }
    // Auto-expand bass lanes.  If the plan mentions any bass at all BUT
    // the raw planner output didn't explicitly name a single voice in a
    // narrow command (e.g. "rewrite bass 2"), extend the bass set to
    // every active bass voice — preserving the order the planner chose
    // for the one it did name, and appending the rest.
    let any_bass = lanes.iter().any(|l| matches!(l, LaneKind::Bass(_)));
    let named_bass_count = raw_labels.iter().filter(|s| s.starts_with("bass")).count();
    let single_voice_command = named_bass_count == 1 && raw_labels.len() <= 2;
    if any_bass && !single_voice_command {
        let existing: std::collections::BTreeSet<usize> = lanes
            .iter()
            .filter_map(|l| {
                if let LaneKind::Bass(i) = l {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect();
        // Walk enabled voices in order; insert any that aren't already in
        // the plan right after the last existing bass lane so drums still
        // come before bass and FX after.
        let mut to_insert: Vec<LaneKind> = Vec::new();
        for (idx, voice) in state.bass_voices.iter().enumerate() {
            if voice.enabled && !existing.contains(&idx) && lane_is_live(state, LaneKind::Bass(idx))
            {
                to_insert.push(LaneKind::Bass(idx));
            }
        }
        if !to_insert.is_empty() {
            // Find the index just after the last existing Bass lane so
            // the new voices cluster with the rest and keep the
            // drums-before-bass / bass-before-fx ordering intact.
            let last_bass = lanes
                .iter()
                .rposition(|l| matches!(l, LaneKind::Bass(_)))
                .unwrap_or(lanes.len() - 1);
            for (offset, lane) in to_insert.into_iter().enumerate() {
                lanes.insert(last_bass + 1 + offset, lane);
            }
        }
    }
    let rationale = obj
        .get("rationale")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Some(LanePlan {
        lanes,
        rationale,
        from_retry: false,
    })
}

/// Deterministic full-jam plan built from the current rack + active bass
/// voices.  Used as the fallback when the planner fails, and as the
/// "start a jam" expansion.
///
/// Order: `Settings → KitA → KitB → Amen → Bass(0..N) → Hoover → An1x
/// → Fx → Mod`.  Voices/modules absent from the rack are skipped, so a
/// bass-only rack yields `[Settings, Bass(0), Fx]`.
pub fn default_plan(state: &AppState) -> LanePlan {
    let mut lanes = Vec::new();
    lanes.push(LaneKind::Settings);
    if rack_has(state, ModuleKind::DrumKit808) {
        lanes.push(LaneKind::KitA);
    }
    if rack_has(state, ModuleKind::DrumKit909) {
        lanes.push(LaneKind::KitB);
    }
    if rack_has(state, ModuleKind::AmenSampler) {
        lanes.push(LaneKind::Amen);
    }
    if rack_has(state, ModuleKind::AcidBass) {
        for (idx, voice) in state.bass_voices.iter().enumerate() {
            if voice.enabled {
                lanes.push(LaneKind::Bass(idx));
            }
        }
    }
    if rack_has(state, ModuleKind::HooverLead) {
        lanes.push(LaneKind::Hoover);
    }
    if rack_has(state, ModuleKind::An1xVoice) {
        lanes.push(LaneKind::An1x);
    }
    lanes.push(LaneKind::Fx);
    LanePlan {
        lanes,
        rationale: "default full-jam plan (planner fallback)".to_string(),
        from_retry: false,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

pub(crate) fn lane_from_label(label: &str) -> Option<LaneKind> {
    match label {
        "settings" => Some(LaneKind::Settings),
        "bass1" => Some(LaneKind::Bass(0)),
        "bass2" => Some(LaneKind::Bass(1)),
        "bass3" => Some(LaneKind::Bass(2)),
        "bass4" => Some(LaneKind::Bass(3)),
        "kit_a" => Some(LaneKind::KitA),
        "kit_b" => Some(LaneKind::KitB),
        "amen" => Some(LaneKind::Amen),
        "hoover" => Some(LaneKind::Hoover),
        "an1x" => Some(LaneKind::An1x),
        "fx" => Some(LaneKind::Fx),
        "mod" => Some(LaneKind::Modulation),
        "rack" => Some(LaneKind::Rack),
        _ => None,
    }
}

fn rack_has(state: &AppState, kind: ModuleKind) -> bool {
    state
        .rack
        .modules
        .iter()
        .any(|m| m.kind == kind && m.enabled && state.rack.reaches_master(m.id))
}

/// Is the lane actually wired up / enabled on the current state?  Used to
/// filter planner output so we don't fire a no-op bass3 lane because the
/// model guessed wrong.
fn lane_is_live(state: &AppState, lane: LaneKind) -> bool {
    lane_is_live_impl(state, lane)
}

/// Re-export of `lane_is_live` for the pipeline run-loop's defensive
/// double-check (filters stale lane labels right before execution, in
/// case the rack changed since the planner picked the lane).
pub fn lane_is_live_pub(state: &AppState, lane: LaneKind) -> bool {
    lane_is_live_impl(state, lane)
}

fn lane_is_live_impl(state: &AppState, lane: LaneKind) -> bool {
    match lane {
        // Always-available lanes.
        LaneKind::Settings | LaneKind::Fx | LaneKind::Modulation | LaneKind::Rack => true,
        LaneKind::Bass(idx) => {
            idx < state.bass_voices.len()
                && state.bass_voices[idx].enabled
                && rack_has(state, ModuleKind::AcidBass)
        }
        LaneKind::KitA => rack_has(state, ModuleKind::DrumKit808),
        LaneKind::KitB => rack_has(state, ModuleKind::DrumKit909),
        LaneKind::Amen => rack_has(state, ModuleKind::AmenSampler),
        LaneKind::Hoover => rack_has(state, ModuleKind::HooverLead),
        LaneKind::An1x => rack_has(state, ModuleKind::An1xVoice),
    }
}

/// Short comma-separated list of active voice labels for the planner
/// state header — e.g. "bass1, bass2, kit_a, kit_b".  Keeps the prompt
/// data tight so the planner's own prefill stays cheap.
fn describe_active_voices(state: &AppState) -> String {
    let mut v: Vec<&'static str> = Vec::new();
    for (idx, voice) in state.bass_voices.iter().enumerate() {
        if voice.enabled && rack_has(state, ModuleKind::AcidBass) {
            v.push(match idx {
                0 => "bass1",
                1 => "bass2",
                2 => "bass3",
                3 => "bass4",
                _ => "bassN",
            });
        }
    }
    if rack_has(state, ModuleKind::DrumKit808) {
        v.push("kit_a");
    }
    if rack_has(state, ModuleKind::DrumKit909) {
        v.push("kit_b");
    }
    if rack_has(state, ModuleKind::AmenSampler) {
        v.push("amen");
    }
    if rack_has(state, ModuleKind::HooverLead) {
        v.push("hoover");
    }
    if rack_has(state, ModuleKind::An1xVoice) {
        v.push("an1x");
    }
    if v.is_empty() {
        "(none)".to_string()
    } else {
        v.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::planner_heuristic::heuristic_plan;
    use crate::state::{
        ModuleKind, PortDir, PortKind, PortRef, RACK_PRESETS, RackModule, RackState,
    };

    fn state_with_full_rack() -> AppState {
        let full = RACK_PRESETS.iter().find(|p| p.name == "Full").unwrap();
        let mut s = AppState::default();
        s.rack = RackState::from_preset(full);
        s.bass_voices[0].enabled = true;
        s.bass_voices[1].enabled = true;
        s
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

    #[test]
    fn planner_prompt_lists_active_voices() {
        let s = state_with_full_rack();
        let prompt = build_planner_prompt(&s);
        assert!(prompt.contains("bass1"));
        assert!(prompt.contains("bass2"));
        assert!(prompt.contains("kit_a"));
        assert!(prompt.contains("kit_b"));
    }

    #[test]
    fn planner_prompt_is_tiny() {
        // The planner's own prompt should stay small — its whole point is
        // that the per-lane calls fall out of cheap prefill.  3 k chars
        // is a generous ceiling (the body is ~1.5 k).
        let s = state_with_full_rack();
        let prompt = build_planner_prompt(&s);
        assert!(
            prompt.len() < 3000,
            "planner prompt is {} chars, should be < 3000",
            prompt.len()
        );
    }

    #[test]
    fn parse_simple_lane_list() {
        // Two bass voices are enabled in state_with_full_rack() — the
        // auto-expansion kicks in, so bass2 is filled in after bass1.
        let s = state_with_full_rack();
        let j = serde_json::json!({
            "lanes": ["settings", "kit_a", "bass1", "fx"],
            "rationale": "acid jam"
        });
        let plan = parse_planner_output(&s, &j).unwrap();
        assert_eq!(
            plan.lanes,
            vec![
                LaneKind::Settings,
                LaneKind::KitA,
                LaneKind::Bass(0),
                LaneKind::Bass(1),
                LaneKind::Fx,
            ]
        );
        assert_eq!(plan.rationale, "acid jam");
    }

    #[test]
    fn parse_filters_disabled_lanes() {
        // bass-only state: planner asks for bass2 + kit_a, both absent.
        let s = bass_only_state();
        let j = serde_json::json!({
            "lanes": ["settings", "bass1", "bass2", "kit_a", "fx"]
        });
        let plan = parse_planner_output(&s, &j).unwrap();
        assert_eq!(
            plan.lanes,
            vec![LaneKind::Settings, LaneKind::Bass(0), LaneKind::Fx]
        );
    }

    #[test]
    fn parse_deduplicates() {
        // Raw planner output has dupes — filter collapses them, then
        // expansion adds bass2 since state_with_full_rack() has it active.
        let s = state_with_full_rack();
        let j = serde_json::json!({
            "lanes": ["bass1", "bass1", "fx", "bass1", "fx"]
        });
        let plan = parse_planner_output(&s, &j).unwrap();
        assert_eq!(
            plan.lanes,
            vec![LaneKind::Bass(0), LaneKind::Bass(1), LaneKind::Fx]
        );
    }

    #[test]
    fn bass_plan_expands_to_cover_all_active_voices() {
        // Planner LLM picks just bass1 but both voices are enabled —
        // post-processing should add bass2 right after bass1.
        let s = state_with_full_rack();
        let j = serde_json::json!({
            "lanes": ["kit_a", "bass1", "fx"]
        });
        let plan = parse_planner_output(&s, &j).unwrap();
        assert_eq!(
            plan.lanes,
            vec![
                LaneKind::KitA,
                LaneKind::Bass(0),
                LaneKind::Bass(1),
                LaneKind::Fx,
            ]
        );
    }

    #[test]
    fn narrow_bass_command_stays_single_voice() {
        // "rewrite bass 2" → [bass2] only.  Post-processing must NOT
        // auto-expand when the planner clearly picked one specific voice.
        let s = state_with_full_rack();
        let j = serde_json::json!({ "lanes": ["bass2"] });
        let plan = parse_planner_output(&s, &j).unwrap();
        assert_eq!(plan.lanes, vec![LaneKind::Bass(1)]);
    }

    #[test]
    fn single_bass_rack_no_expansion() {
        // One voice enabled — expansion finds nothing to add.
        let s = bass_only_state();
        let j = serde_json::json!({ "lanes": ["bass1", "fx"] });
        let plan = parse_planner_output(&s, &j).unwrap();
        assert_eq!(plan.lanes, vec![LaneKind::Bass(0), LaneKind::Fx]);
    }

    #[test]
    fn parse_returns_none_when_all_filtered_out() {
        // Only bass2 requested on a single-voice state — nothing survives.
        let s = bass_only_state();
        let j = serde_json::json!({ "lanes": ["bass2", "kit_a", "amen"] });
        assert!(parse_planner_output(&s, &j).is_none());
    }

    #[test]
    fn parse_returns_none_on_malformed_json() {
        let s = bass_only_state();
        assert!(parse_planner_output(&s, &serde_json::json!({})).is_none());
        assert!(parse_planner_output(&s, &serde_json::json!([])).is_none());
        assert!(parse_planner_output(&s, &serde_json::json!(null)).is_none());
    }

    #[test]
    fn default_plan_follows_voice_order() {
        let s = state_with_full_rack();
        let plan = default_plan(&s);
        // Settings must come first.
        assert_eq!(plan.lanes.first(), Some(&LaneKind::Settings));
        // Fx must come last.
        assert_eq!(plan.lanes.last(), Some(&LaneKind::Fx));
        // Drums before bass.
        let kit_a_pos = plan.lanes.iter().position(|&l| l == LaneKind::KitA);
        let bass_pos = plan.lanes.iter().position(|&l| l == LaneKind::Bass(0));
        if let (Some(k), Some(b)) = (kit_a_pos, bass_pos) {
            assert!(k < b, "kit_a ({k}) should come before bass1 ({b})");
        }
    }

    #[test]
    fn default_plan_skips_missing_voices() {
        let s = bass_only_state();
        let plan = default_plan(&s);
        // Only Settings + Bass(0) + Fx should appear.
        assert_eq!(
            plan.lanes,
            vec![LaneKind::Settings, LaneKind::Bass(0), LaneKind::Fx]
        );
    }

    #[test]
    fn planner_schema_accepts_valid_output() {
        // Round-trip sanity check: the schema should parse as valid JSON.
        let schema = planner_schema();
        assert!(schema.is_object());
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["lanes"].is_object());
    }

    // ── Heuristic planner tests ─────────────────────────────────────────

    #[test]
    fn heuristic_numbered_bass_forms() {
        let s = state_with_full_rack();
        // Every numeric-ish phrasing must resolve to the same voice index.
        for prompt in [
            "rewrite bass 2",
            "change BASS 2",
            "update bass2",
            "modify bass #2",
            "tweak bass voice 2",
            "fix the bass voice two",
            "SECOND BASS please",
            "2nd bass — make it wider",
            "redo bass two with more slide",
        ] {
            let plan = heuristic_plan(&s, prompt)
                .unwrap_or_else(|| panic!("heuristic missed prompt: {:?}", prompt));
            assert_eq!(
                plan.lanes,
                vec![LaneKind::Bass(1)],
                "prompt {:?} didn't resolve to bass2",
                prompt
            );
        }
    }

    #[test]
    fn heuristic_first_and_other_bass_voices() {
        let s = state_with_full_rack();
        let pairs = [
            ("rewrite bass 1", 0),
            ("first bass", 0),
            ("BASS1", 0),
            ("rewrite bass 3", 2),
            ("third bass voice", 2),
            ("fourth bass please", 3),
        ];
        for (prompt, expected_idx) in pairs {
            let plan = heuristic_plan(&s, prompt);
            // Voice 3 / 4 aren't enabled in the default full-rack fixture,
            // so those should return None even though the phrasing matches.
            if expected_idx > 1 {
                assert!(
                    plan.is_none(),
                    "voice {} not enabled, heuristic should fall through for {:?}",
                    expected_idx,
                    prompt
                );
            } else {
                let plan = plan.unwrap_or_else(|| panic!("missed {:?}", prompt));
                assert_eq!(plan.lanes, vec![LaneKind::Bass(expected_idx)]);
            }
        }
    }

    #[test]
    fn heuristic_falls_through_on_multi_topic() {
        // "make bass 2 more squelchy and add reverb" mentions both a
        // specific bass voice AND fx — the heuristic should refuse and
        // let the LLM planner handle the compound command.
        let s = state_with_full_rack();
        assert!(heuristic_plan(&s, "make bass 2 more squelchy and add reverb").is_none());
        // Same if the prompt asks for bass + drums.
        assert!(heuristic_plan(&s, "rewrite bass 2 and the 808 kick").is_none());
    }

    #[test]
    fn heuristic_fx_only_commands() {
        let s = state_with_full_rack();
        for prompt in [
            "add reverb",
            "more delay",
            "no reverb",
            "turn the chorus up",
            "remove distortion",
            "more bitcrush",
        ] {
            let plan = heuristic_plan(&s, prompt).unwrap_or_else(|| panic!("missed {:?}", prompt));
            assert_eq!(plan.lanes, vec![LaneKind::Fx]);
        }
    }

    #[test]
    fn heuristic_kit_only_commands() {
        let s = state_with_full_rack();
        let kit_a_prompts = ["change the 808", "kit a groove", "rewrite kick a"];
        for prompt in kit_a_prompts {
            let plan = heuristic_plan(&s, prompt).unwrap_or_else(|| panic!("missed {:?}", prompt));
            assert_eq!(plan.lanes, vec![LaneKind::KitA]);
        }
        let kit_b_prompts = [
            "change the 909",
            "kit b groove",
            "rewrite clap",
            "909 kick harder",
        ];
        for prompt in kit_b_prompts {
            let plan = heuristic_plan(&s, prompt).unwrap_or_else(|| panic!("missed {:?}", prompt));
            assert_eq!(plan.lanes, vec![LaneKind::KitB]);
        }
    }

    #[test]
    fn heuristic_no_match_on_broad_prompts() {
        let s = state_with_full_rack();
        // Broad jam requests must fall through so the LLM planner picks
        // the full lane set.
        assert!(heuristic_plan(&s, "start a jam").is_none());
        assert!(heuristic_plan(&s, "make a pattern").is_none());
        assert!(heuristic_plan(&s, "pick a style and make something wild").is_none());
    }

    #[test]
    fn heuristic_skips_long_prompts() {
        let s = state_with_full_rack();
        // Long, multi-clause prompts go straight to the LLM planner.
        let long = "please rewrite bass 2 with a really long prompt that explains why the \
                    second voice should counterpoint voice one using a specific approach \
                    involving the Dorian mode and a mix of accents and slides";
        assert!(heuristic_plan(&s, long).is_none());
    }

    #[test]
    fn heuristic_respects_voice_liveness() {
        // Voice 2 disabled + "rewrite bass 2" → heuristic should return
        // None so the LLM planner (or default_plan) handles it gracefully.
        let s = bass_only_state();
        assert!(heuristic_plan(&s, "rewrite bass 2").is_none());
        // Voice 1 is alive, so "rewrite bass 1" still resolves.
        let plan = heuristic_plan(&s, "rewrite bass 1").unwrap();
        assert_eq!(plan.lanes, vec![LaneKind::Bass(0)]);
    }

    #[test]
    fn lane_from_label_round_trip() {
        for label in [
            "settings", "bass1", "bass2", "bass3", "bass4", "kit_a", "kit_b", "amen", "hoover",
            "an1x", "fx", "mod", "rack",
        ] {
            let lane = lane_from_label(label).unwrap_or_else(|| panic!("unknown label: {label}"));
            assert_eq!(
                lane.label(),
                label,
                "round-trip failed for {label} → {:?}",
                lane
            );
        }
    }
}
