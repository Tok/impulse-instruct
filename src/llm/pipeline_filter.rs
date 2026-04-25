// ─── llm/pipeline_filter.rs ──────────────────────────────────────────────────
// Lane scope + output filtering — extracted from pipeline.rs to keep that
// file under the 1000-line cap.  Two pure functions:
//
//   • `lane_apply_scope` maps a `LaneKind` to the dot-paths
//     `apply_llm_update` is allowed to write.  Stops a Bass(0) lane
//     from accidentally overwriting `kit_a` patterns even if the model
//     somehow emits them.
//
//   • `filter_lane_output` strips any JSON keys outside the lane's
//     scope, drops `sequencer` subkeys outside the lane's allowed
//     subkeys, and discards empty pattern arrays (treating
//     `"bass_steps": []` as "clear voice" was a destructive silent-
//     failure mode when the model emitted required arrays without
//     having anything useful to say).
//
// `pipeline.rs` re-exports both via `pub use` so existing imports
// (`crate::llm::pipeline::filter_lane_output`) keep working.

use crate::llm::lanes::LaneKind;
use serde_json::Value;

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
/// filter keeps us honest when the server is loose (e.g. older
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
