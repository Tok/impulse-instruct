// ─── llm_suite.rs ─────────────────────────────────────────────────────────────
// Core LLM integration tests — directional parameter and pattern clearing.
//
// Each test fires the same prompt RUNS times and passes if at least REQUIRED
// responses satisfy the assertion — a probabilistic gate that tolerates LLM
// variance without being brittle.
//
//   Heat 0.2–0.3  →  REQUIRED_TIGHT = 9  (deterministic ops: "remove kick")
//   Heat 0.3      →  REQUIRED_LOOSE = 7  (directional: "more acid")
//
// Suites:
//   llm_suite        — basic parameter direction + pattern clearing + schema
//   llm_suite_style  — artist/genre reference comprehension (Aphex, Bach, etc.)
//   llm_suite_theory — music theory + producer lingo (triads, backbeat, Reese)
//
// Run:   ./scripts/run-llm-tests.sh          # all three suites
//        ./scripts/run-llm-style.sh           # style suite only
//        ./scripts/run-llm-theory.sh          # theory suite only
//        cargo test --features llm-tests      # all (needs LLAMA_SERVER_URL)
//
// Requires: libclang-dev cmake (for llama-cpp-2) + ./download-models.sh
// Skip:     if model file not present, all tests pass silently.

use crate::llm::prompt::build_system_prompt;
use crate::llm::{LlamaServerBackend, LlmBackend};
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

/// Returns `(backend, system_prompt)` if the server is reachable.
///
/// Two modes:
///   • `LLAMA_SERVER_URL=http://127.0.0.1:PORT` — connect to an already-running server
///     (used by `run-llm-tests.sh`, avoids spawning a second instance that would
///     compete for VRAM and cause a 30-second timeout).
///   • No env var — spawn a new server the normal way (slow; each test binary pays
///     the full model load time, but works standalone).
fn setup() -> Option<(LlamaServerBackend, String)> {
    let state = AppState::default();

    let backend = if let Ok(url) = std::env::var("LLAMA_SERVER_URL") {
        LlamaServerBackend::connect(&url)
    } else {
        LlamaServerBackend::new(&state.llm.model_path)
    };

    if !backend.is_live() {
        eprintln!(
            "\n[llm-suite] SKIP — LLM server not available \
             (model: '{}')\n\
             Run ./scripts/run-llm-tests.sh, or set LLAMA_SERVER_URL to a running instance.\n",
            state.llm.model_path
        );
        return None;
    }
    let system = build_system_prompt(&state);
    Some((backend, system))
}

// ANSI dim+italic for thinking tokens — readable but visually subordinate.
const THINK_ON: &str = "\x1b[2;3m";
const THINK_OFF: &str = "\x1b[0m";

/// Truncate `s` to at most `max` chars, appending "…" if cut.
/// When `LLM_SUITE_VERBOSE=1` is set, returns the full string untruncated.
fn trunc(s: &str, max: usize) -> String {
    if std::env::var("LLM_SUITE_VERBOSE").is_ok_and(|v| v == "1") || s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max)])
    }
}

fn infer_json(
    backend: &mut LlamaServerBackend,
    system: &str,
    prompt: &str,
    heat: f32,
) -> Option<Value> {
    match backend.infer(system, prompt, heat) {
        Ok(out) => {
            // Print thinking tokens in dim+italic so they're visible but clearly
            // subordinate to the test result line.
            if let Some(ref think) = out.thinking {
                eprintln!(
                    "{THINK_ON}  <think> {} </think>{THINK_OFF}",
                    trunc(think, 300)
                );
            }
            if out.param_update.is_none() {
                eprintln!(
                    "[llm-suite] infer OK but param_update=None (text: {})",
                    trunc(&out.text, 200)
                );
            }
            out.param_update
        }
        Err(e) => {
            eprintln!("[llm-suite] infer error: {}", trunc(&e.to_string(), 300));
            None
        }
    }
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
        .filter(|_| {
            infer_json(backend, system, prompt, heat)
                .map(|v| check(&v))
                .unwrap_or(false)
        })
        .count();
    // Always print the score so the test line reads "... (7/10) ok" or "(1/10) FAILED".
    let gate_label = if passes >= required { "✓" } else { "✗" };
    eprint!("  {gate_label} {passes}/{RUNS} (need ≥{required})  ");
    assert!(
        passes >= required,
        "[llm-suite] '{}': {}/{} runs passed (need {})\n\
         → model needs tuning or system prompt needs adjustment",
        prompt,
        passes,
        RUNS,
        required
    );
}

// ── Directional assertions (REQUIRED_LOOSE) ───────────────────────────────────
//
// These verify that the model responds in the right direction.
// Some variance is expected — the gate allows 3 misses in 10.

#[test]
fn acid_raises_resonance() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "more acid", 0.3, REQUIRED_LOOSE, |j| {
        num(j, "bass.resonance") >= 0.6
    });
}

#[test]
fn acid_raises_env_mod() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "acid squelch", 0.3, REQUIRED_LOOSE, |j| {
        at(j, "bass.env_mod").is_some() && num(j, "bass.env_mod") >= 0.5
    });
}

#[test]
fn darker_lowers_cutoff() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "make it darker", 0.3, REQUIRED_LOOSE, |j| {
        num(j, "bass.cutoff") <= 0.35
    });
}

#[test]
fn add_reverb_raises_reverb_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "add reverb", 0.3, REQUIRED_LOOSE, |j| {
        at(j, "fx.reverb_mix").is_some() && num(j, "fx.reverb_mix") >= 0.1
    });
}

#[test]
fn remove_reverb_zeroes_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no reverb", 0.3, REQUIRED_TIGHT, |j| {
        num(j, "fx.reverb_mix") < 0.01
    });
}

#[test]
fn harder_raises_distortion() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "make it harder", 0.3, REQUIRED_LOOSE, |j| {
        let bass = at(j, "bass.distortion")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let fx = at(j, "fx.distortion_drive")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        bass > 0.05 || fx > 0.05
    });
}

#[test]
fn add_delay_raises_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "add delay", 0.3, REQUIRED_LOOSE, |j| {
        at(j, "fx.delay_mix").is_some() && num(j, "fx.delay_mix") > 0.0
    });
}

#[test]
fn remove_delay_zeroes_mix() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no delay", 0.3, REQUIRED_TIGHT, |j| {
        num(j, "fx.delay_mix") < 0.01
    });
}

// ── Pattern clearing (REQUIRED_TIGHT) ────────────────────────────────────────
//
// Remove instructions are explicit commands — the model should nearly always
// return an all-false array. 1 miss in 10 is tolerated for tokenisation noise.

#[test]
fn remove_kick_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "remove kick", 0.3, REQUIRED_TIGHT, |j| {
        all_false(j, "sequencer.kick_a_steps")
    });
}

#[test]
fn remove_clap_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no claps", 0.3, REQUIRED_TIGHT, |j| {
        all_false(j, "sequencer.clap_b_steps")
    });
}

#[test]
fn remove_hihat_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no hats", 0.3, REQUIRED_TIGHT, |j| {
        all_false(j, "sequencer.hihat_a_steps")
    });
}

#[test]
fn remove_snare_clears_pattern() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "no snare", 0.3, REQUIRED_TIGHT, |j| {
        all_false(j, "sequencer.snare_a_steps")
    });
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
    for prompt in [
        "more acid",
        "darker",
        "add reverb",
        "remove kick",
        "make it harder",
    ] {
        assert_gate(&mut b, &sys, prompt, 0.3, REQUIRED_TIGHT, |j| {
            j.get("_comment").is_some()
        });
    }
}

#[test]
fn unit_params_always_in_range() {
    let paths = [
        "bass.cutoff",
        "bass.resonance",
        "bass.env_mod",
        "bass.decay",
        "fx.reverb_mix",
        "fx.delay_mix",
        "fx.distortion_drive",
        "fx.distortion_mix",
    ];
    let Some((mut b, sys)) = setup() else { return };
    for prompt in ["more acid", "darker", "harder", "add reverb"] {
        assert_gate(&mut b, &sys, prompt, 0.5, REQUIRED_TIGHT, |j| {
            paths.iter().all(|path| {
                at(j, path)
                    .and_then(|v| v.as_f64())
                    .map_or(true, |v| (0.0..=1.0).contains(&v))
            })
        });
    }
}

#[test]
fn no_unknown_top_level_keys() {
    let known = [
        "_thinking",
        "_comment",
        "mc_line",
        "bass",
        "sequencer",
        "fx",
        "hoover",
        "an1x",
        "free_eg",
        "noise",
        "lfo",
        "kit_a",
        "kit_b",
        "euclidean",
    ];
    let Some((mut b, sys)) = setup() else { return };
    for prompt in ["more acid", "darker", "add reverb", "remove kick"] {
        assert_gate(&mut b, &sys, prompt, 0.3, REQUIRED_TIGHT, |j| {
            j.as_object()
                .map_or(true, |obj| obj.keys().all(|k| known.contains(&k.as_str())))
        });
    }
}
