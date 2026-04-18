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
         \n\
         AVAILABLE LANES:\n\
         • settings  — bpm / swing / key / scale / style (fire first on a \
           style change or initial jam)\n\
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
            active_voices list above). Exception: `settings`, `fx`, `mod`, \
            `rack` are always available.\n\
         2. Narrow user commands pick ONE lane. `rewrite bass 2` → [bass2]. \
            `add reverb` → [fx]. `change the kick` → [kit_a] or [kit_b].\n\
         3. Broad commands (`acid house`, `start a jam`, `make a track`, \
            style change) pick a FULL jam: settings first, then drums, \
            then bass, then fx.\n\
         4. Order matters — settings always first, drums before bass \
            (bass can reference the kick grid), fx last.\n\
         5. Keep it minimal. Fewer lanes = faster response. Don't fire \
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
pub fn parse_planner_output(state: &AppState, json: &serde_json::Value) -> Option<LanePlan> {
    let obj = json.as_object()?;
    let lanes_arr = obj.get("lanes")?.as_array()?;
    let mut lanes: Vec<LaneKind> = Vec::new();
    for val in lanes_arr {
        if let Some(s) = val.as_str()
            && let Some(lane) = lane_from_label(s)
            && lane_is_live(state, lane)
            && !lanes.contains(&lane)
        {
            lanes.push(lane);
        }
    }
    if lanes.is_empty() {
        return None;
    }
    let rationale = obj
        .get("rationale")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Some(LanePlan { lanes, rationale })
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
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn lane_from_label(label: &str) -> Option<LaneKind> {
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
        let s = state_with_full_rack();
        let j = serde_json::json!({
            "lanes": ["bass1", "bass1", "fx", "bass1", "fx"]
        });
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
