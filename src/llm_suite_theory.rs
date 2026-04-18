// ─── llm_suite_theory.rs ──────────────────────────────────────────────────────
// Music theory and producer lingo integration tests.
//
// These probe whether the model understands practical music theory vocabulary
// that any producer is expected to know:
//   • Rhythm patterns: four-on-the-floor, backbeat, half-time, one-drop
//   • Harmonic knowledge: triads, natural minor, pentatonic scales
//   • Producer lingo: Reese bass, sub bass, chord stabs, slow attack pad
//
// REQUIRED gate is 7/10 throughout — theory knowledge has more variance than
// simple parameter tweaks.
//
// Run:   ./scripts/run-llm-theory.sh
//        cargo test --features llm-tests -- llm_suite_theory

use crate::llm::prompt::build_system_prompt;
use crate::llm::{LlamaServerBackend, LlmBackend, SamplingParams};
use crate::state::AppState;
use serde_json::Value;

const RUNS: usize = 10;
const REQUIRED: usize = 7;

fn at<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').fold(Some(json), |v, k| v?.get(k))
}

fn num(json: &Value, path: &str) -> f64 {
    at(json, path).and_then(|v| v.as_f64()).unwrap_or(f64::NAN)
}

/// Extract pitch classes (note mod 12) from a MIDI note array at `path`.
/// Zeros are treated as rests and filtered out.
fn pitch_classes(json: &Value, path: &str) -> Vec<u8> {
    at(json, path)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64())
                .filter(|&n| n > 0 && n < 128)
                .map(|n| (n % 12) as u8)
                .collect()
        })
        .unwrap_or_default()
}

/// Returns true if step `s` (0-based 16th-note index) is active in a step array.
/// Handles both index-list format `[0,4,8,12]` and bool-array format.
fn step_on(json: &Value, path: &str, s: usize) -> bool {
    let Some(val) = at(json, path) else {
        return false;
    };
    let Some(arr) = val.as_array() else {
        return false;
    };
    if arr.is_empty() {
        return false;
    }
    if arr[0].is_boolean() {
        arr.get(s).and_then(|v| v.as_bool()).unwrap_or(false)
    } else {
        arr.iter().any(|v| v.as_u64() == Some(s as u64))
    }
}

fn setup() -> Option<(LlamaServerBackend, String)> {
    let state = AppState::default();
    let backend = if let Ok(url) = std::env::var("LLAMA_SERVER_URL") {
        LlamaServerBackend::connect(&url)
    } else {
        LlamaServerBackend::new(&state.llm.model_path, 8192, 18080)
    };
    if !backend.is_live() {
        eprintln!(
            "\n[llm-theory] SKIP — server not available (model: '{}')\n\
             Run ./scripts/run-llm-theory.sh or set LLAMA_SERVER_URL.\n",
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
            if out.param_update.is_none() {
                eprintln!(
                    "[llm-theory] infer OK but param_update=None (text: {})",
                    trunc(&out.text, 200)
                );
            }
            out.param_update
        }
        Err(e) => {
            eprintln!("[llm-theory] infer error: {}", trunc(&e.to_string(), 300));
            None
        }
    }
}

fn assert_gate(
    backend: &mut LlamaServerBackend,
    system: &str,
    prompt: &str,
    heat: f32,
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
    let gate_label = if passes >= REQUIRED { "✓" } else { "✗" };
    eprint!("  {gate_label} {passes}/{RUNS} (need ≥{REQUIRED}) ~{avg_ms}ms/req  ");
    assert!(
        passes >= REQUIRED,
        "[llm-theory] '{}': {}/{} runs passed (need {})\n\
         → model lacks music theory knowledge or prompt needs adjustment",
        prompt,
        passes,
        RUNS,
        REQUIRED
    );
}

// ── Rhythm / groove lingo ─────────────────────────────────────────────────────

/// "Four-on-the-floor" is the most fundamental club-music term.
/// The kick must land on all four quarter-note downbeats.
#[test]
fn four_on_the_floor_kick() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(&mut b, &sys, "four-on-the-floor kick pattern", 0.3, |j| {
        step_on(j, "sequencer.kick_a_steps", 0)
            && step_on(j, "sequencer.kick_a_steps", 4)
            && step_on(j, "sequencer.kick_a_steps", 8)
            && step_on(j, "sequencer.kick_a_steps", 12)
    });
}

/// "Backbeat" = snare on beats 2 and 4 — universal producer vocabulary.
/// Steps 4 and 12 in a 16th-note grid correspond to beats 2 and 4.
#[test]
fn backbeat_snare() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "snare on the backbeat — beats 2 and 4",
        0.3,
        |j| {
            let snare = |s| {
                step_on(j, "sequencer.snare_a_steps", s) || step_on(j, "sequencer.clap_b_steps", s)
            };
            snare(4) && snare(12)
        },
    );
}

/// Half-time feel: snare drops from beat 2+4 to beat 3 only (step 8).
/// Common in trap, hip-hop breaks, DnB intros.
#[test]
fn half_time_snare_on_beat_three() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "half-time feel — snare only on beat 3, not on beat 2",
        0.3,
        |j| {
            let snare_beat2 =
                step_on(j, "sequencer.snare_a_steps", 4) || step_on(j, "sequencer.clap_b_steps", 4);
            let snare_beat3 =
                step_on(j, "sequencer.snare_a_steps", 8) || step_on(j, "sequencer.clap_b_steps", 8);
            snare_beat3 && !snare_beat2
        },
    );
}

/// Offbeat hi-hats = upbeats between main beats: steps 2, 6, 10, 14.
/// Standard ska/reggae feel. At least 3 of 4 upbeats on, downbeats off.
#[test]
fn offbeat_hihat_upbeats() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "hi-hat on the offbeats — only on the upbeats between the main beats",
        0.3,
        |j| {
            let path = "sequencer.hihat_a_steps";
            let upbeats = [2usize, 6, 10, 14]
                .iter()
                .filter(|&&s| step_on(j, path, s))
                .count();
            let downbeats_on = [0usize, 4, 8, 12]
                .iter()
                .filter(|&&s| step_on(j, path, s))
                .count();
            upbeats >= 3 && downbeats_on <= 1
        },
    );
}

/// Reggae one-drop: kick absent on beat 1 (step 0) — the silence is the groove.
#[test]
fn reggae_one_drop_no_kick_on_one() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "reggae one-drop pattern — no kick on beat 1, the drop is the silence on one",
        0.3,
        |j| !step_on(j, "sequencer.kick_a_steps", 0),
    );
}

/// Straight 16th hi-hat roll: all (or nearly all) 16 steps active.
/// Standard house/techno/electro — dense, mechanical hats.
#[test]
fn straight_16th_hihat_roll() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "straight 16th note hi-hat roll — every single 16th note",
        0.3,
        |j| {
            let active = (0usize..16)
                .filter(|&s| step_on(j, "sequencer.hihat_a_steps", s))
                .count();
            active >= 12
        },
    );
}

// ── Harmonic theory ───────────────────────────────────────────────────────────

/// D minor triad = D F A (pitch classes 2, 5, 9).
/// At least 2 of 3 chord tones must appear in the AN1X or bass notes.
#[test]
fn d_minor_chord_correct_notes() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "set the AN1X to a D minor chord — D F A",
        0.3,
        |j| {
            let classes: Vec<u8> = pitch_classes(j, "an1x.an1x_notes")
                .into_iter()
                .chain(pitch_classes(j, "sequencer.bass_notes"))
                .collect();
            if classes.is_empty() {
                return false;
            }
            // D=2, F=5, A=9
            let chord = [2u8, 5, 9];
            chord.iter().filter(|n| classes.contains(n)).count() >= 2
        },
    );
}

/// A minor pentatonic: A C D E G = pitch classes {0, 2, 4, 7, 9}.
/// All bass notes should fall within this scale (1 wrong note tolerated).
#[test]
fn a_minor_pentatonic_bass() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "bass line using A minor pentatonic — only the notes A C D E G",
        0.3,
        |j| {
            let classes = pitch_classes(j, "sequencer.bass_notes");
            if classes.is_empty() {
                return false;
            }
            let pent: [u8; 5] = [0, 2, 4, 7, 9]; // C D E G A
            let wrong = classes.iter().filter(|n| !pent.contains(n)).count();
            wrong == 0 || (classes.len() >= 4 && wrong == 1)
        },
    );
}

/// A natural minor scale: A B C D E F G = pitch classes {0,2,4,5,7,9,11}.
/// All bass notes should stay in scale (1 chromatic slip tolerated).
#[test]
fn a_natural_minor_bass_notes() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "bass melody in A natural minor — use only A B C D E F G, stay in key",
        0.3,
        |j| {
            let classes = pitch_classes(j, "sequencer.bass_notes");
            if classes.is_empty() {
                return false;
            }
            let scale: [u8; 7] = [0, 2, 4, 5, 7, 9, 11]; // C D E F G A B
            let wrong = classes.iter().filter(|n| !scale.contains(n)).count();
            wrong == 0 || (classes.len() >= 4 && wrong == 1)
        },
    );
}

// ── Producer lingo ────────────────────────────────────────────────────────────

/// Reese bass = two detuned oscillators — the signature DnB/jungle sub.
/// Should trigger any oscillator detune parameter.
#[test]
fn reese_bass_detuned_oscillators() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "Reese bass — two detuned oscillators, classic DnB/jungle sub bass",
        0.3,
        |j| {
            let supersaw = num(j, "bass.supersaw_detune");
            let osc_det = num(j, "bass.osc_detune").abs();
            let an1x_det = num(j, "an1x.osc2_detune");
            // an1x osc2_detune: 0.5 = unison, >0.52 = detuned
            supersaw > 0.05 || osc_det > 0.02 || (!an1x_det.is_nan() && an1x_det > 0.52)
        },
    );
}

/// Sub bass = only the fundamental — filter nearly closed, no harmonics.
#[test]
fn sub_bass_closes_filter() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "pure sub bass — only the fundamental, close the filter to remove all harmonics",
        0.3,
        |j| num(j, "bass.cutoff") < 0.25,
    );
}

/// Chord stabs = short, punchy transients — decay must be low on bass or AN1X.
#[test]
fn chord_stabs_short_decay() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "short chord stabs — punchy, very short decay, tight rhythmic hits",
        0.3,
        |j| {
            let bass_dec = num(j, "bass.decay");
            let an1x_dec = num(j, "an1x.amp_decay");
            (!bass_dec.is_nan() && bass_dec < 0.3) || (!an1x_dec.is_nan() && an1x_dec < 0.3)
        },
    );
}

/// Slow attack pad = gradual swell — AN1X amp_attack should be high (≥0.3).
#[test]
fn slow_attack_pad_swell() {
    let Some((mut b, sys)) = setup() else { return };
    assert_gate(
        &mut b,
        &sys,
        "slow attack pad swell — very gradual fade-in, lush ambient texture",
        0.3,
        |j| {
            let atk = num(j, "an1x.amp_attack");
            !atk.is_nan() && atk >= 0.3
        },
    );
}
