// ─── state/llm_apply_seq.rs ──────────────────────────────────────────────────
// Per-section LLM apply helpers for sequencer globals, the Amen sampler, and
// the Euclidean rhythm shortcut.  Extracted from apply_llm_update so the
// mega-function in llm_apply.rs stays readable and so each section can be
// unit-tested in isolation against a crafted JSON object.
//
// Helpers take `&mut AppState` because the caller has already taken
// ownership of a snapshot inside `apply_llm_update` (the public function
// remains pure: AppState in → AppState out).

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::llm_helpers::{unlocked_f32, unlocked_f32_range};
use super::transitions::expand_sequencer_steps;
use super::{AppState, DrumVoice, MAX_STEPS, Scale};
use crate::sequencer::euclidean_rhythm;

/// Apply the global sequencer fields (`bpm`, `swing`, `steps`,
/// `time_sig_num`, `root_note`, `scale`) from the `sequencer.*` LLM JSON
/// object.  Each field respects its lock path (`sequencer.bpm`, etc).
///
/// `steps` triggers a length-resize via `expand_sequencer_steps` because
/// changing the step count needs to grow / shrink every per-voice pattern,
/// not just the scalar.
pub(super) fn apply_sequencer_globals(
    s: &mut AppState,
    seq: &Map<String, Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("sequencer.bpm")
        && let Some(bpm) = seq.get("bpm").and_then(|v| v.as_f64())
    {
        s.sequencer.bpm = (bpm as f32).clamp(40.0, 250.0);
    }
    if !locked.contains("sequencer.swing")
        && let Some(v) = seq.get("swing").and_then(|v| v.as_f64())
    {
        s.sequencer.swing = (v as f32).clamp(0.0, 1.0);
    }
    if !locked.contains("sequencer.steps")
        && let Some(steps) = seq.get("steps").and_then(|v| v.as_u64())
    {
        // expand_sequencer_steps takes ownership; swap through a temporary.
        let taken = std::mem::take(s);
        *s = expand_sequencer_steps(taken, steps as usize);
    }
    if !locked.contains("sequencer.time_sig_num")
        && let Some(v) = seq.get("time_sig_num").and_then(|v| v.as_u64())
    {
        s.sequencer.time_sig_num = (v as u8).clamp(2, 9);
    }
    if !locked.contains("sequencer.root_note")
        && let Some(v) = seq.get("root_note").and_then(|v| v.as_u64())
    {
        s.sequencer.root_note = (v as u8).clamp(0, 11);
    }
    if !locked.contains("sequencer.scale")
        && let Some(v) = seq.get("scale").and_then(|v| v.as_str())
        && let Some(sc) = Scale::from_str(v)
    {
        s.sequencer.scale = sc;
    }
}

/// Apply the `amen.*` LLM JSON object: pitch, volume, loop_mode,
/// slice_count, start/end_offset, reverse, gate, stutter, source_bpm,
/// bpm_stretch, slice_pitches, slice_volumes.
///
/// Notable behaviours:
/// - `end_offset` is bumped above `start_offset` if the LLM crosses them.
/// - Per-slice arrays clear when the JSON value is `null` (or skip if
///   the corresponding lock path is set).
/// - `slice_pitches` clamp to ±24 semitones, `slice_volumes` to 0..2;
///   both truncate to 16 entries to match `slice_count`'s upper bound.
pub(super) fn apply_amen_update(
    s: &mut AppState,
    a: &Map<String, Value>,
    locked: &HashSet<String>,
) {
    s.amen.pitch = unlocked_f32_range(s.amen.pitch, a, "pitch", "amen.pitch", locked, -24.0, 24.0);
    s.amen.volume = unlocked_f32(s.amen.volume, a, "volume", "amen.volume", locked);
    if let Some(v) = a.get("loop_mode").and_then(|v| v.as_bool())
        && !locked.contains("amen.loop_mode")
    {
        s.amen.loop_mode = v;
    }
    if let Some(v) = a.get("slice_count").and_then(|v| v.as_u64())
        && !locked.contains("amen.slice_count")
    {
        s.amen.slice_count = (v as u8).clamp(1, 16);
    }
    s.amen.start_offset = unlocked_f32(
        s.amen.start_offset,
        a,
        "start_offset",
        "amen.start_offset",
        locked,
    )
    .clamp(0.0, 1.0);
    s.amen.end_offset = unlocked_f32(
        s.amen.end_offset,
        a,
        "end_offset",
        "amen.end_offset",
        locked,
    )
    .clamp(0.0, 1.0);
    if s.amen.end_offset <= s.amen.start_offset {
        s.amen.end_offset = (s.amen.start_offset + 0.01).min(1.0);
    }
    if let Some(v) = a.get("reverse").and_then(|v| v.as_bool())
        && !locked.contains("amen.reverse")
    {
        s.amen.reverse = v;
    }
    s.amen.gate = unlocked_f32(s.amen.gate, a, "gate", "amen.gate", locked).clamp(0.05, 1.0);
    if let Some(v) = a.get("stutter").and_then(|v| v.as_u64())
        && !locked.contains("amen.stutter")
    {
        s.amen.stutter = (v as u8).min(4);
    }
    s.amen.source_bpm = unlocked_f32_range(
        s.amen.source_bpm,
        a,
        "source_bpm",
        "amen.source_bpm",
        locked,
        40.0,
        300.0,
    );
    if let Some(v) = a.get("bpm_stretch").and_then(|v| v.as_bool())
        && !locked.contains("amen.bpm_stretch")
    {
        s.amen.bpm_stretch = v;
    }
    if !locked.contains("amen.slice_pitches")
        && let Some(v) = a.get("slice_pitches")
    {
        if let Some(arr) = v.as_array() {
            s.amen.slice_pitches = arr
                .iter()
                .filter_map(|x| x.as_f64())
                .map(|x| (x as f32).clamp(-24.0, 24.0))
                .take(16)
                .collect();
        } else if v.is_null() {
            s.amen.slice_pitches.clear();
        }
    }
    if !locked.contains("amen.slice_volumes")
        && let Some(v) = a.get("slice_volumes")
    {
        if let Some(arr) = v.as_array() {
            s.amen.slice_volumes = arr
                .iter()
                .filter_map(|x| x.as_f64())
                .map(|x| (x as f32).clamp(0.0, 2.0))
                .take(16)
                .collect();
        } else if v.is_null() {
            s.amen.slice_volumes.clear();
        }
    }
}

/// Map a textual voice id (`"kick_a"`, `"snare_b"`, …) to its `DrumVoice`
/// variant.  Pure helper — handy for reuse and for asserting the mapping
/// in tests independently of any apply call.
pub fn drum_voice_from_str(name: &str) -> Option<DrumVoice> {
    match name {
        "kick_a" => Some(DrumVoice::Kick808),
        "snare_a" => Some(DrumVoice::Snare808),
        "hihat_a" | "closed_hat_a" => Some(DrumVoice::HihatClosed808),
        "hihat_a_open" | "open_hat_a" => Some(DrumVoice::HihatOpen808),
        "kick_b" => Some(DrumVoice::Kick909),
        "snare_b" => Some(DrumVoice::Snare909),
        "hihat_b" | "closed_hat_b" => Some(DrumVoice::HihatClosed909),
        "hihat_b_open" | "open_hat_b" => Some(DrumVoice::HihatOpen909),
        "clap_b" => Some(DrumVoice::Clap909),
        _ => None,
    }
}

/// Apply the `euclidean.*` shortcut: write a Bjorklund-style pulse pattern
/// to a single drum lane.  `pulses` defaults to 4, `steps` defaults to the
/// current sequencer step count, and the lock path
/// `sequencer.<voice>_steps` is honoured (matching the regular per-voice
/// `*_steps` write).  An unknown voice id is a no-op.
pub(super) fn apply_euclidean_update(
    s: &mut AppState,
    e: &Map<String, Value>,
    locked: &HashSet<String>,
) {
    let voice_str = e.get("voice").and_then(|v| v.as_str()).unwrap_or("");
    let pulses = e.get("pulses").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
    let n_steps = e
        .get("steps")
        .and_then(|v| v.as_u64())
        .unwrap_or(s.sequencer.steps as u64) as usize;
    let Some(voice) = drum_voice_from_str(voice_str) else {
        return;
    };
    let lock_path = format!("sequencer.{}_steps", voice_str);
    if locked.contains(&lock_path) {
        return;
    }
    let pattern = euclidean_rhythm(pulses, n_steps);
    if let Some(row) = s.sequencer.drum_patterns.get_mut(&voice) {
        let usable = pattern.len().min(row.len()).min(MAX_STEPS);
        for (i, &active) in pattern.iter().enumerate().take(usable) {
            row[i].active = active;
        }
    }
}
