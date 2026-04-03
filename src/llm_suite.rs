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
            "\n[llm-suite] SKIP — Bonsai server not available \
             (model: '{}')\n\
             Run ./run-llm-tests.sh, or set LLAMA_SERVER_URL to a running instance.\n",
            state.llm.model_path
        );
        return None;
    }
    let system = build_system_prompt(&state);
    Some((backend, system))
}

fn infer_json(
    backend: &mut LlamaServerBackend,
    system: &str,
    prompt: &str,
    heat: f32,
) -> Option<Value> {
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
        .filter(|_| {
            infer_json(backend, system, prompt, heat)
                .map(|v| check(&v))
                .unwrap_or(false)
        })
        .count();
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

// ── Artist / cultural reference comprehension (REQUIRED_LOOSE) ───────────────
//
// These tests check whether Bonsai maps cultural references to the right
// sonic parameter space.  A consistent FAIL means the reference is not in the
// model's training data and the description in styles.json should be rewritten
// as a plain sonic brief instead.
//
// Artist tiers for Bonsai 8B (likely trained on web text up to ~2023):
//   ✅ Probably known: Aphex Twin, Autechre, Daft Punk, Kraftwerk, Tangerine Dream
//   🟡 Possibly known: Phuture/DJ Pierre, Basic Channel, Venetian Snares, Plastikman
//   ❓ Uncertain: Neophyte, Drexciya, Mixmaster Morris, Gost, Enduser
//
// If a test here fails, update styles.json to drop the name and describe the sound.

/// Classic acid should set high resonance — the squelch IS the point.
/// Phuture and DJ Pierre are foundational acid house; if Bonsai knows "acid house"
/// at all it should know these names.
#[test]
fn classic_acid_phuture_sets_high_resonance() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "classic Chicago acid house — think Phuture, DJ Pierre, Trax Records, pure 303 squelch",
        0.3,
        REQUIRED_LOOSE,
        |j| num(j, "bass.resonance") >= 0.65,
    );
}

/// Classic acid should stay dry — barely any FX.
#[test]
fn classic_acid_phuture_stays_dry() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "classic acid, Phuture style — raw and dry, no reverb, no delay",
        0.3,
        REQUIRED_LOOSE,
        |j| {
            let rmix = at(j, "fx.reverb_mix")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let dmix = at(j, "fx.delay_mix")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            rmix <= 0.2 && dmix <= 0.15
        },
    );
}

/// Autechre = IDM, so kick should NOT be strict four-on-the-floor.
/// This is one of the most famous IDM acts — if Bonsai knows any IDM, it knows Autechre.
#[test]
fn autechre_idm_breaks_four_on_the_floor() {
    let four_floor = serde_json::json!([
        true, false, false, false, true, false, false, false, true, false, false, false, true,
        false, false, false
    ]);
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "go full Autechre IDM — irregular kick, subvert the grid, nothing four-on-the-floor",
        0.3,
        REQUIRED_LOOSE,
        |j| {
            match at(j, "sequencer.kick_a_steps") {
                None => true,                    // no kick at all = fine for IDM
                Some(arr) => arr != &four_floor, // anything but pure 4-on-the-floor
            }
        },
    );
}

/// Aphex Twin Selected Ambient Works Vol 2 = spacious, heavy reverb.
#[test]
fn aphex_twin_ambient_uses_reverb() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "ambient Aphex Twin mood — Selected Ambient Works Vol 2, spacious and ethereal",
        0.3,
        REQUIRED_LOOSE,
        |j| num(j, "fx.reverb_mix") >= 0.25,
    );
}

/// Basic Channel dub techno = FX-heavy (reverb + delay are the music).
/// If Bonsai doesn't know Basic Channel, "dub techno" alone should still trigger FX.
#[test]
fn basic_channel_dub_techno_uses_heavy_fx() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "dub techno in the style of Basic Channel — maximum reverb, ghost delay echoes",
        0.3,
        REQUIRED_LOOSE,
        |j| num(j, "fx.reverb_mix") >= 0.3 || num(j, "fx.delay_mix") >= 0.2,
    );
}

/// Berlin techno = dark filter, deep bass, not much melody.
/// Richie Hawtin / Berghain refs — model should understand "Berlin" if not the names.
#[test]
fn berlin_techno_sets_dark_filter() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "Berlin techno — Berghain floor, deep dark kick, filter nearly closed, Richie Hawtin",
        0.3,
        REQUIRED_LOOSE,
        |j| num(j, "bass.cutoff") <= 0.35,
    );
}

/// Venetian Snares breakcore = very high BPM (if the model touches BPM at all).
/// If Bonsai doesn't know Venetian Snares, this tells us to drop the name.
#[test]
fn venetian_snares_sets_high_bpm() {
    let Some((mut b, sys)) = setup() else { return };
    // Only meaningful if BPM is in the output at all
    assert_gate(
        &mut b,
        &sys,
        "breakcore chaos, Venetian Snares energy — shredded Amen, extreme BPM",
        0.3,
        REQUIRED_LOOSE,
        |j| {
            match at(j, "sequencer.bpm").and_then(|v| v.as_f64()) {
                None => true, // didn't touch BPM — inconclusive, not a failure
                Some(bpm) => bpm >= 160.0,
            }
        },
    );
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
    let known = ["_comment", "bass", "sequencer", "fx"];
    let Some((mut b, sys)) = setup() else { return };
    for prompt in ["more acid", "darker", "add reverb", "remove kick"] {
        assert_gate(&mut b, &sys, prompt, 0.3, REQUIRED_TIGHT, |j| {
            j.as_object()
                .map_or(true, |obj| obj.keys().all(|k| known.contains(&k.as_str())))
        });
    }
}
