// ─── llm_suite_bass.rs ───────────────────────────────────────────────────────
// Integration tests for the bass-voice directives added in the recent
// per-voice / accent-slide / coverage prompt work.
//
// The other three suites (`llm_suite`, `llm_suite_style`, `llm_suite_theory`)
// exercise directional behaviour, genre comprehension, and music theory.
// This suite verifies the three hard rules that the prompt introduces:
//
//   • MULTI-VOICE RULE    — populate each active bass voice
//   • GROOVE CHECKLIST    — emit bass_accents / bass_slides (not just steps)
//   • SUBSET RULE         — accent/slide indices must appear in steps
//   • FULL-COVERAGE RULE  — beat-driven styles populate drums, not just bass
//
// Run via `./scripts/run-llm-tests.sh` (same harness as the other suites).
//
// Tests silently pass when `LLAMA_SERVER_URL` is unset and no local model is
// available, matching the existing harness.

use crate::llm::prompt::build_system_prompt;
use crate::llm::{LlamaServerBackend, LlmBackend, SamplingParams};
use crate::state::AppState;
use serde_json::Value;

const RUNS: usize = 10;
const REQUIRED_LOOSE: usize = 7;

fn at<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').fold(Some(json), |v, k| v?.get(k))
}

/// Does `key` exist under `sequencer.*` as a non-empty array?  Accepts
/// both boolean arrays (inline format) and integer index lists.
fn has_nonempty_step_array(json: &Value, key: &str) -> bool {
    at(json, &format!("sequencer.{}", key))
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
}

/// Indices covered by a step array — handles both formats.  For inline
/// bool arrays, every index where the value is `true` (or non-zero int)
/// counts; for index lists, every listed integer counts.  Used to check
/// SUBSET RULE and to count active steps.
fn covered_indices(json: &Value, key: &str) -> std::collections::BTreeSet<u64> {
    let mut out = std::collections::BTreeSet::new();
    let Some(arr) = at(json, &format!("sequencer.{}", key)).and_then(|v| v.as_array()) else {
        return out;
    };
    if arr.is_empty() {
        return out;
    }
    if arr.len() < 16 {
        // Treat as index list.
        for v in arr {
            if let Some(n) = v.as_u64() {
                out.insert(n);
            }
        }
    } else {
        // Inline bool / 0/1 array.
        for (i, v) in arr.iter().enumerate() {
            let truthy =
                v.as_bool().unwrap_or(false) || v.as_u64().map(|n| n != 0).unwrap_or(false);
            if truthy {
                out.insert(i as u64);
            }
        }
    }
    out
}

/// Build a two-bass-voice initial state: both voices enabled, default
/// rack (which wires bass + kits to MASTER through `wire_default_cables`
/// in `RackState::default`).  Style set to Acid House so the FULL-COVERAGE
/// rule applies (beat-driven).
fn two_voice_acid_state() -> AppState {
    let mut s = AppState::default();
    s.bass_voices[0].enabled = true;
    if s.bass_voices.len() > 1 {
        s.bass_voices[1].enabled = true;
    }
    if s.sequencer.bass_voice_enabled.len() > 1 {
        s.sequencer.bass_voice_enabled[1] = true;
    }
    s.llm.active_style = Some("acid_house".to_string());
    s
}

fn setup(state: AppState) -> Option<(LlamaServerBackend, String)> {
    let backend = if let Ok(url) = std::env::var("LLAMA_SERVER_URL") {
        LlamaServerBackend::connect(&url)
    } else {
        LlamaServerBackend::new(&state.llm.model_path, 8192, 18080)
    };
    if !backend.is_live() {
        eprintln!(
            "\n[llm-suite-bass] SKIP — LLM server not available \
             (model: '{}')\n\
             Run ./scripts/run-llm-tests.sh, or set LLAMA_SERVER_URL.\n",
            state.llm.model_path
        );
        return None;
    }
    let system = build_system_prompt(&state, &[]);
    Some((backend, system))
}

const THINK_ON: &str = "\x1b[2;3m";
const THINK_OFF: &str = "\x1b[0m";

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
    let sampling = SamplingParams {
        heat,
        ..SamplingParams::default()
    };
    match backend.infer(system, prompt, &sampling) {
        Ok(out) => {
            if let Some(ref think) = out.thinking {
                eprintln!(
                    "{THINK_ON}  <think> {} </think>{THINK_OFF}",
                    trunc(think, 300)
                );
            }
            out.param_update
        }
        Err(e) => {
            eprintln!(
                "[llm-suite-bass] infer error: {}",
                trunc(&e.to_string(), 300)
            );
            None
        }
    }
}

fn assert_gate(
    backend: &mut LlamaServerBackend,
    system: &str,
    prompt: &str,
    heat: f32,
    required: usize,
    check: impl Fn(&Value) -> bool,
) {
    let mut passes = 0usize;
    let mut total_ms = 0u128;
    for _ in 0..RUNS {
        let t0 = std::time::Instant::now();
        let ok = infer_json(backend, system, prompt, heat)
            .map(|v| check(&v))
            .unwrap_or(false);
        total_ms += t0.elapsed().as_millis();
        if ok {
            passes += 1;
        }
    }
    let avg_ms = total_ms / RUNS as u128;
    let gate_label = if passes >= required { "✓" } else { "✗" };
    eprint!("  {gate_label} {passes}/{RUNS} (need ≥{required}) ~{avg_ms}ms/req  ");
    assert!(
        passes >= required,
        "[llm-suite-bass] '{}': {}/{} runs passed (need {})\n\
         → prompt engineering likely out of sync with schema",
        prompt,
        passes,
        RUNS,
        required
    );
}

// ── MULTI-VOICE RULE ─────────────────────────────────────────────────────────

#[test]
fn two_voices_active_rewrite_bass_populates_both() {
    // With both bass voices enabled, a generic "rewrite the bass" prompt
    // should emit step + note arrays for BOTH voices.
    let Some((mut b, sys)) = setup(two_voice_acid_state()) else {
        return;
    };
    assert_gate(
        &mut b,
        &sys,
        "rewrite the bass with a fresh acid line",
        0.4,
        REQUIRED_LOOSE,
        |j| {
            has_nonempty_step_array(j, "bass_steps")
                && has_nonempty_step_array(j, "bass_notes")
                && has_nonempty_step_array(j, "bass2_steps")
                && has_nonempty_step_array(j, "bass2_notes")
        },
    );
}

// ── GROOVE CHECKLIST ─────────────────────────────────────────────────────────

#[test]
fn bass_rewrite_includes_accents() {
    // Any bass-rewrite response should include bass_accents with at least
    // one hit — the prompt asks for 3–5 per 32 steps but we keep the gate
    // loose (1+) to tolerate sparse-bass styles.
    let Some((mut b, sys)) = setup(AppState::default()) else {
        return;
    };
    assert_gate(
        &mut b,
        &sys,
        "write an acid bassline",
        0.4,
        REQUIRED_LOOSE,
        |j| !covered_indices(j, "bass_accents").is_empty(),
    );
}

#[test]
fn bass_rewrite_includes_slides() {
    let Some((mut b, sys)) = setup(AppState::default()) else {
        return;
    };
    assert_gate(
        &mut b,
        &sys,
        "write an acid bassline with slides",
        0.4,
        REQUIRED_LOOSE,
        |j| !covered_indices(j, "bass_slides").is_empty(),
    );
}

// ── SUBSET RULE ──────────────────────────────────────────────────────────────

#[test]
fn bass_accents_are_subset_of_bass_steps() {
    // When both arrays are emitted in one response, every accent index
    // must also appear as an active step.  The apply layer scrubs
    // inactive accents anyway, but a compliant prompt avoids the scrub.
    let Some((mut b, sys)) = setup(AppState::default()) else {
        return;
    };
    assert_gate(
        &mut b,
        &sys,
        "fresh acid bassline with accents on the strong beats",
        0.3,
        REQUIRED_LOOSE,
        |j| {
            let steps = covered_indices(j, "bass_steps");
            let accents = covered_indices(j, "bass_accents");
            if steps.is_empty() || accents.is_empty() {
                // Either array missing — subset rule is trivially satisfied
                // but we still want both populated.  Fail softly so the
                // test focuses on the subset property.
                return accents.is_empty();
            }
            accents.iter().all(|a| steps.contains(a))
        },
    );
}

// ── FULL-COVERAGE RULE ───────────────────────────────────────────────────────

#[test]
fn initial_jam_on_beat_driven_style_populates_drums() {
    // Acid House is beat-driven → an initial "start a jam" prompt with
    // drum kits in the rack MUST produce a kick pattern, not just bass.
    let Some((mut b, sys)) = setup(two_voice_acid_state()) else {
        return;
    };
    assert_gate(&mut b, &sys, "start a jam", 0.4, REQUIRED_LOOSE, |j| {
        has_nonempty_step_array(j, "kick_a_steps") || has_nonempty_step_array(j, "kick_b_steps")
    });
}
