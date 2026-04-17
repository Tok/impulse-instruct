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
use super::transitions::{expand_sequencer_steps, set_lane_steps};
use super::{AppState, DrumVoice, MAX_STEPS, Scale, snap_to_scale};
use crate::sequencer::euclidean_rhythm;

/// Per-voice scope flags for the `sequencer.*` block of the LLM JSON.
/// A bare "sequencer" scope grants every per-voice subsection; otherwise
/// each voice key (`bass`, `hoover`, `an1x`, `kit_a`, `kit_b`, `amen`)
/// only unlocks its own slice of the sequencer object.  Centralising the
/// resolution stops the same boolean fan-out from being recomputed at
/// every helper call site.
#[derive(Clone, Copy, Debug)]
pub struct SeqScope {
    pub seq: bool,
    pub bass: bool,
    pub hoover: bool,
    pub an1x: bool,
    pub kit_a: bool,
    pub kit_b: bool,
    pub amen: bool,
}

impl SeqScope {
    /// Resolve scope flags from the agent-supplied scope list.  An empty
    /// scope means "all keys allowed" (the unscoped, full-stack agent path).
    pub fn from_scope(scope: &[String]) -> Self {
        let in_scope = |k: &str| scope.is_empty() || scope.iter().any(|s| s == k);
        let seq = in_scope("sequencer");
        Self {
            seq,
            bass: seq || in_scope("bass"),
            hoover: seq || in_scope("hoover"),
            an1x: seq || in_scope("an1x"),
            kit_a: seq || in_scope("kit_a"),
            kit_b: seq || in_scope("kit_b"),
            amen: seq || in_scope("amen"),
        }
    }

    /// True when at least one sequencer-related key is in scope — used to
    /// short-circuit the whole `sequencer.*` block when nothing inside it
    /// is reachable for the current agent.
    pub fn any(&self) -> bool {
        self.seq || self.bass || self.hoover || self.an1x || self.kit_a || self.kit_b || self.amen
    }

    /// Resolve scope for a preecho voice key.  Unknown voice keys fall
    /// back to the global `sequencer` scope (matching the behaviour the
    /// inline match used to have).
    pub fn for_preecho_voice(&self, voice: &str) -> bool {
        match voice {
            "bass" => self.bass,
            "hoover" => self.hoover,
            "an1x" => self.an1x,
            "kit_a" => self.kit_a,
            "kit_b" => self.kit_b,
            "amen" => self.amen,
            _ => self.seq,
        }
    }
}

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

/// Apply per-melodic-voice lane lengths from the `sequencer` JSON object:
/// `bass_len`, `hoover_len`, `an1x_len`.  Each respects its own scope
/// flag and the matching `sequencer.<voice>_len` lock path.
pub(super) fn apply_melodic_lane_lens(
    s: &mut AppState,
    seq: &Map<String, Value>,
    scope: SeqScope,
    locked: &HashSet<String>,
) {
    if scope.bass
        && !locked.contains("sequencer.bass_len")
        && let Some(n) = seq.get("bass_len").and_then(|v| v.as_u64())
    {
        let taken = std::mem::take(s);
        *s = set_lane_steps(taken, "bass", n as usize);
    }
    if scope.hoover
        && !locked.contains("sequencer.hoover_len")
        && let Some(n) = seq.get("hoover_len").and_then(|v| v.as_u64())
    {
        let taken = std::mem::take(s);
        *s = set_lane_steps(taken, "hoover", n as usize);
    }
    if scope.an1x
        && !locked.contains("sequencer.an1x_len")
        && let Some(n) = seq.get("an1x_len").and_then(|v| v.as_u64())
    {
        let taken = std::mem::take(s);
        *s = set_lane_steps(taken, "an1x", n as usize);
    }
}

/// Apply the bass `bass_notes` array (per-step MIDI notes).  Notes are
/// clamped to 0..=127 and snapped to the active scale when
/// `scale_snap` is on.  Voice-0's mirror pattern is updated in lockstep so
/// the legacy single-voice fields and per-voice fields stay coherent.
pub(super) fn apply_bass_notes(
    s: &mut AppState,
    seq: &Map<String, Value>,
    scope: SeqScope,
    locked: &HashSet<String>,
) {
    if !scope.bass || locked.contains("sequencer.bass_notes") {
        return;
    }
    let Some(arr) = seq.get("bass_notes").and_then(|v| v.as_array()) else {
        return;
    };
    let snap = s.sequencer.scale_snap;
    let root = s.sequencer.root_note;
    let scale = s.sequencer.scale;
    for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
        if let Some(note) = val.as_u64() {
            let note = note.clamp(0, 127) as u8;
            let snapped = if snap {
                snap_to_scale(note, root, scale)
            } else {
                note
            };
            s.sequencer.bass_pattern[i].note = snapped;
            if let Some(pat) = s.sequencer.bass_patterns.get_mut(0) {
                pat[i].note = snapped;
            }
        }
    }
}

/// Apply per-step pan offsets (`bass_pans`, parallel to `bass_notes`).
/// Each value is clamped to `-1..=1`; `0.0` means "use the voice's
/// static pan".  Mirrors voice 0's bass_patterns slot in lockstep.
pub(super) fn apply_bass_pans(
    s: &mut AppState,
    seq: &Map<String, Value>,
    scope: SeqScope,
    locked: &HashSet<String>,
) {
    if !scope.bass || locked.contains("sequencer.bass_pans") {
        return;
    }
    let Some(arr) = seq.get("bass_pans").and_then(|v| v.as_array()) else {
        return;
    };
    for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
        if let Some(p) = val.as_f64() {
            let p = (p as f32).clamp(-1.0, 1.0);
            s.sequencer.bass_pattern[i].pan = p;
            if let Some(pat) = s.sequencer.bass_patterns.get_mut(0) {
                pat[i].pan = p;
            }
        }
    }
}

/// Apply the `preecho` map: per-voice anchor lead-in configuration.
/// JSON shape:
///   `"preecho": { "kit_a": { "anchors": [0,16], "length": 4,
///     "velocity_ramp": true, "ratchet_ramp": true, "enabled": true } }`.
/// `null` clears that voice; voice keys match the regular scope naming
/// (`bass` / `hoover` / `an1x` / `kit_a` / `kit_b` / `amen`).
pub(super) fn apply_preecho_voices(
    s: &mut AppState,
    seq: &Map<String, Value>,
    scope: SeqScope,
    locked: &HashSet<String>,
) {
    let Some(obj) = seq.get("preecho").and_then(|v| v.as_object()) else {
        return;
    };
    for (voice_key, val) in obj {
        let lock_key = format!("sequencer.preecho.{}", voice_key);
        if locked.contains(&lock_key) {
            continue;
        }
        if !scope.for_preecho_voice(voice_key) {
            continue;
        }
        if val.is_null() {
            s.sequencer.preecho.remove(voice_key);
            continue;
        }
        let Some(cfg_obj) = val.as_object() else {
            continue;
        };
        let mut cfg = s
            .sequencer
            .preecho
            .get(voice_key)
            .cloned()
            .unwrap_or_default();
        if let Some(v) = cfg_obj.get("enabled").and_then(|v| v.as_bool()) {
            cfg.enabled = v;
        }
        if let Some(arr) = cfg_obj.get("anchors").and_then(|v| v.as_array()) {
            cfg.anchors = arr
                .iter()
                .filter_map(|x| x.as_u64())
                .map(|n| (n as u8).min(63))
                .collect();
        }
        if let Some(v) = cfg_obj.get("length").and_then(|v| v.as_u64()) {
            cfg.length = (v as u8).min(16);
        }
        if let Some(v) = cfg_obj.get("velocity_ramp").and_then(|v| v.as_bool()) {
            cfg.velocity_ramp = v;
        }
        if let Some(v) = cfg_obj.get("ratchet_ramp").and_then(|v| v.as_bool()) {
            cfg.ratchet_ramp = v;
        }
        s.sequencer.preecho.insert(voice_key.clone(), cfg);
    }
}
