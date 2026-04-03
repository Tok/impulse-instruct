// ─── llm_suite.rs ─────────────────────────────────────────────────────────────
// Real Bonsai integration tests.
//
// Each test fires the same prompt RUNS times and passes if at least REQUIRED
// responses satisfy the assertion — a probabilistic gate that tolerates LLM
// variance without being brittle.
//
//   Heat 0.2–0.3  →  use REQUIRED = 9  (low variance, tight assertions)
//   Heat 0.5      →  use REQUIRED = 7  (moderate — directional but not rigid)
//
// If the gate fails it means the model needs better tuning or the system
// prompt needs adjustment, not that the test needs softening.
//
// Run:   ./run-llm-tests.sh
//        cargo test --features llm-tests
//
// Requires: libclang-dev cmake (for llama-cpp-2) + ./download-models.sh
// Skip:     if model file not present, all tests pass silently.

use crate::llm::{LlamaServerBackend, LlmBackend};
use crate::llm::prompt::build_system_prompt;
use crate::state::AppState;
use serde_json::Value;

// ── Pass gate ─────────────────────────────────────────────────────────────────

/// Runs at low heat — tight gate.
const RUNS: usize = 10;
const REQUIRED_TIGHT: usize = 9; // for deterministic ops like "remove kick"
const REQUIRED_LOOSE: usize = 7; // for directional / generative prompts

// ── Helpers ───────────────────────────────────────────────────────────────────

fn at<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').fold(Some(json), |v, k| v?.get(k))
}

fn num(json: &Value, path: &str) -> f64 {
    at(json, path).and_then(|v| v.as_f64()).unwrap_or(f64::NAN)
}

fn all_false(json: &Value, path: &str) -> bool {
    at(json, path)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().all(|v| v == &serde_json::json!(false)))
        .unwrap_or(false)
}

/// Returns `(backend, system_prompt)` if the server comes up.
/// Returns `None` and prints a skip message if binary or model are missing.
fn setup() -> Option<(LlamaServerBackend, String)> {
    let state = AppState::default();
    let backend = LlamaServerBackend::new(&state.llm.model_path);
    if !backend.is_live() {
        eprintln!(
            "\n[llm-suite] SKIP — Bonsai server not available \
             (model: '{}')\n\
             Run ./build-bonsai-server.sh + ./download-models.sh.\n",
            state.llm.model_path
        );
        return None;
    }
    let system = build_system_prompt(&state);
    Some((backend, system))
}

fn infer_json(backend: &mut LlamaServerBackend, system: &str, prompt: &str, heat: f32) -> Option<Value> {
    backend.infer(system, prompt, heat).ok()?.param_update
}

/// Run `check` RUNS times; assert at least `required` pass.
fn assert_gate(
    backend: &mut LlamaServerBackend,
    system: &str,
    prompt: &str,
    heat: f32,
    required: usize,
    check: impl Fn(&Value) -> bool,
) {
    let passes = (0..RUNS)
        .filter(|_| infer_json(backend, system, prompt, heat).map_or(false, &check))
        .count();
    assert!(
        passes >= required,
        "[llm-suite] '{}': {}/{} runs passed (need {})\n\
         → model needs tuning or system prompt needs adjustment",
        prompt, passes, RUNS, required
    );
}

// ── Directional assertions (REQUIRED_LOOSE) ───────────────────────────────────
//
// These verify that the model responds in the right direction.
// Some variance is expected — the gate allows 3 misses in 10.

#[test]
fn acid_raises_resonance() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "more acid", 0.3, REQUIRED_LOOSE,
        |j| num(j, "bass.resonance") >= 0.6);
}

#[test]
fn acid_raises_env_mod() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "acid squelch", 0.3, REQUIRED_LOOSE,
        |j| at(j, "bass.env_mod").is_some() && num(j, "bass.env_mod") >= 0.5);
}

#[test]
fn darker_lowers_cutoff() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "make it darker", 0.3, REQUIRED_LOOSE,
        |j| num(j, "bass.cutoff") <= 0.35);
}

#[test]
fn add_reverb_raises_reverb_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "add reverb", 0.3, REQUIRED_LOOSE,
        |j| at(j, "fx.reverb_mix").is_some() && num(j, "fx.reverb_mix") >= 0.1);
}

#[test]
fn remove_reverb_zeroes_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no reverb", 0.3, REQUIRED_TIGHT,
        |j| num(j, "fx.reverb_mix") < 0.01);
}

#[test]
fn harder_raises_distortion() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "make it harder", 0.3, REQUIRED_LOOSE, |j| {
        let bass = at(j, "bass.distortion").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let fx   = at(j, "fx.distortion_drive").and_then(|v| v.as_f64()).unwrap_or(0.0);
        bass > 0.05 || fx > 0.05
    });
}

#[test]
fn add_delay_raises_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "add delay", 0.3, REQUIRED_LOOSE,
        |j| at(j, "fx.delay_mix").is_some() && num(j, "fx.delay_mix") > 0.0);
}

#[test]
fn remove_delay_zeroes_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no delay", 0.3, REQUIRED_TIGHT,
        |j| num(j, "fx.delay_mix") < 0.01);
}

// ── Pattern clearing (REQUIRED_TIGHT) ────────────────────────────────────────
//
// Remove instructions are explicit commands — the model should nearly always
// return an all-false array. 1 miss in 10 is tolerated for tokenisation noise.

#[test]
fn remove_kick_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "remove kick", 0.3, REQUIRED_TIGHT,
        |j| all_false(j, "sequencer.kick_a_steps"));
}

#[test]
fn remove_clap_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no claps", 0.3, REQUIRED_TIGHT,
        |j| all_false(j, "sequencer.clap_b_steps"));
}

#[test]
fn remove_hihat_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no hats", 0.3, REQUIRED_TIGHT,
        |j| all_false(j, "sequencer.hihat_a_steps"));
}

#[test]
fn remove_snare_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no snare", 0.3, REQUIRED_TIGHT,
        |j| all_false(j, "sequencer.snare_a_steps"));
}

#[test]
fn clear_all_drums_clears_all_voices() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "clear all drums", 0.3, REQUIRED_TIGHT, |j| {
        all_false(j, "sequencer.kick_a_steps")
            && all_false(j, "sequencer.snare_a_steps")
            && all_false(j, "sequencer.hihat_a_steps")
            && all_false(j, "sequencer.clap_b_steps")
    });
}

// ── Schema compliance (REQUIRED_TIGHT) ───────────────────────────────────────
//
// The model must always produce valid JSON within the known schema.
// Any violation here is a prompt engineering or grammar-constraint bug.

#[test]
fn responses_always_have_comment() {
    let Some((mut b, sys)) = setup() else { return };
    for prompt in ["more acid", "darker", "add reverb", "remove kick", "make it harder"] {
        assert_gate(&mut b, &sys, prompt, 0.3, REQUIRED_TIGHT,
            |j| j.get("_comment").is_some());
    }
}

#[test]
fn unit_params_always_in_range() {
    let paths = [
        "bass.cutoff", "bass.resonance", "bass.env_mod", "bass.decay",
        "fx.reverb_mix", "fx.delay_mix", "fx.distortion_drive", "fx.distortion_mix",
    ];
    let Some((mut b, sys)) = setup() else { return };
    for prompt in ["more acid", "darker", "harder", "add reverb"] {
        assert_gate(&mut b, &sys, prompt, 0.5, REQUIRED_TIGHT, |j| {
            paths.iter().all(|path| {
                at(j, path).and_then(|v| v.as_f64())
                    .map_or(true, |v| (0.0..=1.0).contains(&v))
            })
        });
    }
}

#[test]
fn no_unknown_top_level_keys() {
    let known = ["_comment", "bass", "sequencer", "fx"];
    let Some((mut b, sys)) = setup() else { return };
    for prompt in ["more acid", "darker", "add reverb", "remove kick"] {
        assert_gate(&mut b, &sys, prompt, 0.3, REQUIRED_TIGHT, |j| {
            j.as_object().map_or(true, |obj| {
                obj.keys().all(|k| known.contains(&k.as_str()))
            })
        });
    }
}
