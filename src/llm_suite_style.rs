// ─── llm_suite_style.rs ───────────────────────────────────────────────────────
// Artist and genre reference comprehension tests.
//
// These test whether the model maps cultural/artist references to the correct
// sonic parameter space.  A consistent FAIL means the reference is not in the
// model's training data — update styles.json to use a plain sonic description
// instead of the artist name.
//
// Artist tiers for Bonsai 8B (likely trained on web text up to ~2023):
//   ✅ Probably known: Aphex Twin, Autechre, Daft Punk, Kraftwerk, Tangerine Dream
//   🟡 Possibly known: Phuture/DJ Pierre, Basic Channel, Venetian Snares, Plastikman
//   ❓ Uncertain: Neophyte, Drexciya, Mixmaster Morris, Gost, Enduser
//
// Run:   ./scripts/run-llm-style.sh
//        cargo test --features llm-tests -- llm_suite_style

use crate::llm::prompt::build_system_prompt;
use crate::llm::{LlamaServerBackend, LlmBackend};
use crate::state::AppState;
use serde_json::Value;

const RUNS: usize = 10;
const REQUIRED_TIGHT: usize = 9;
const REQUIRED_LOOSE: usize = 7;

fn at<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').fold(Some(json), |v, k| v?.get(k))
}

fn num(json: &Value, path: &str) -> f64 {
    at(json, path).and_then(|v| v.as_f64()).unwrap_or(f64::NAN)
}

fn setup() -> Option<(LlamaServerBackend, String)> {
    let state = AppState::default();
    let backend = if let Ok(url) = std::env::var("LLAMA_SERVER_URL") {
        LlamaServerBackend::connect(&url)
    } else {
        LlamaServerBackend::new(&state.llm.model_path)
    };
    if !backend.is_live() {
        eprintln!(
            "\n[llm-style] SKIP — server not available (model: '{}')\n\
             Run ./scripts/run-llm-style.sh or set LLAMA_SERVER_URL.\n",
            state.llm.model_path
        );
        return None;
    }
    let system = build_system_prompt(&state);
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
    match backend.infer(system, prompt, heat) {
        Ok(out) => {
            if let Some(ref think) = out.thinking {
                eprintln!(
                    "{THINK_ON}  <think> {} </think>{THINK_OFF}",
                    trunc(think, 300)
                );
            }
            if out.param_update.is_none() {
                eprintln!(
                    "[llm-style] infer OK but param_update=None (text: {})",
                    trunc(&out.text, 200)
                );
            }
            out.param_update
        }
        Err(e) => {
            eprintln!("[llm-style] infer error: {}", trunc(&e.to_string(), 300));
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
    let passes = (0..RUNS)
        .filter(|_| {
            infer_json(backend, system, prompt, heat)
                .map(|v| check(&v))
                .unwrap_or(false)
        })
        .count();
    let gate_label = if passes >= required { "✓" } else { "✗" };
    eprint!("  {gate_label} {passes}/{RUNS} (need ≥{required})  ");
    assert!(
        passes >= required,
        "[llm-style] '{}': {}/{} runs passed (need {})\n\
         → model lacks this reference; update styles.json to use plain sonic description",
        prompt,
        passes,
        RUNS,
        required
    );
}

// ── Artist / cultural reference tests ────────────────────────────────────────

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
        |j| match at(j, "sequencer.kick_a_steps") {
            None => true,
            Some(arr) => arr != &four_floor,
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

/// Berlin techno = dark filter, deep bass.
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
#[test]
fn venetian_snares_sets_high_bpm() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "breakcore chaos, Venetian Snares energy — shredded Amen, extreme BPM",
        0.3,
        REQUIRED_LOOSE,
        |j| match at(j, "sequencer.bpm").and_then(|v| v.as_f64()) {
            None => true,
            Some(bpm) => bpm >= 160.0,
        },
    );
}

// ── Baroque / Bach style ──────────────────────────────────────────────────────
//
// These test whether the model understands classical melodic structure:
// stepwise voice leading, correct tempo range, no drums, piano-like voice.

/// A Bach melody should move mostly by step (conjunct motion ≤5 semitones).
#[test]
fn bach_melody_is_mostly_stepwise() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "compose a Bach-style Baroque melody in D minor — \
         dense stepwise piano melody, no drums, no bass",
        0.3,
        REQUIRED_LOOSE,
        |j| {
            let notes_arr = at(j, "an1x.an1x_notes")
                .or_else(|| at(j, "sequencer.bass_notes"))
                .or_else(|| at(j, "hoover.hoover_notes"))
                .and_then(|v| v.as_array());
            let Some(arr) = notes_arr else {
                return false;
            };
            let notes: Vec<u8> = arr
                .iter()
                .filter_map(|n| n.as_u64().map(|v| v as u8))
                .collect();
            if notes.len() < 3 {
                return false;
            }
            let stepwise = notes
                .windows(2)
                .filter(|w| (w[0] as i16 - w[1] as i16).unsigned_abs() <= 5)
                .count();
            stepwise as f64 / (notes.len() - 1) as f64 >= 0.55
        },
    );
}

/// Baroque Bach should NOT be at club tempo.
#[test]
fn bach_not_club_tempo() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "compose a Bach-style Baroque melody in D minor",
        0.3,
        REQUIRED_LOOSE,
        |j| match at(j, "sequencer.bpm").and_then(|v| v.as_f64()) {
            None => true,
            Some(bpm) => bpm <= 140.0,
        },
    );
}

/// Bach piano needs AN1X enabled; bass should be silent or absent.
#[test]
fn bach_enables_an1x_not_bass() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "FULL RESET to Baroque Bach piano — dense D minor melody, no drums, no bass",
        0.3,
        REQUIRED_LOOSE,
        |j| {
            let an1x_on = at(j, "an1x.enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let bass_silent = at(j, "bass.volume")
                .and_then(|v| v.as_f64())
                .map_or(true, |v| v <= 0.1);
            an1x_on && bass_silent
        },
    );
}
