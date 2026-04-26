// ─── state/llm_helpers_voices_v2.rs ──────────────────────────────────────────
// LLM apply helpers for the four V2 spectrum-shaping voices —
// FM ops, Additive, Modal, Chiptune.  Extracted from
// `llm_helpers.rs` to keep that file under the 1000-line cap.
// All four follow the same shape: a `pub(super)` apply fn that
// consumes a JSON sub-object + the lock set.  Per-osc / per-op
// helpers stay private to this file.

use std::collections::HashSet;

use super::AppState;
use super::MAX_STEPS;
use super::llm_helpers::{unlocked_f32, unlocked_f32_range};

/// Apply per-op fields for one FM operator.  Called four times by
/// `apply_fm_ops_update` with the JSON sub-object for each op.
fn apply_fm_op(
    op: &mut crate::state::FmOp,
    obj: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
    op_path: &str, // e.g. "fm_ops.op1"
) {
    op.ratio = unlocked_f32(op.ratio, obj, "ratio", &format!("{op_path}.ratio"), locked);
    op.level = unlocked_f32(op.level, obj, "level", &format!("{op_path}.level"), locked);
    op.attack = unlocked_f32(
        op.attack,
        obj,
        "attack",
        &format!("{op_path}.attack"),
        locked,
    );
    op.decay = unlocked_f32(op.decay, obj, "decay", &format!("{op_path}.decay"), locked);
    op.sustain = unlocked_f32(
        op.sustain,
        obj,
        "sustain",
        &format!("{op_path}.sustain"),
        locked,
    );
    op.release = unlocked_f32(
        op.release,
        obj,
        "release",
        &format!("{op_path}.release"),
        locked,
    );
}

/// Apply FM operator synth voice fields from an LLM JSON update
/// object.  Voice params + per-op params + sequencer pattern.  Per-
/// op fields nest one level (`fm_ops.op1.ratio`, etc.) so the
/// schema stays readable instead of being a flat 24-field block.
pub(super) fn apply_fm_ops_update(
    s: &mut AppState,
    f: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("fm_ops.enabled")
        && let Some(v) = f.get("enabled").and_then(|v| v.as_bool())
    {
        s.fm_ops.enabled = v;
    }
    s.fm_ops.volume = unlocked_f32_range(
        s.fm_ops.volume,
        f,
        "volume",
        "fm_ops.volume",
        locked,
        0.0,
        1.5,
    );
    if !locked.contains("fm_ops.pan")
        && let Some(v) = f.get("pan").and_then(|v| v.as_f64())
    {
        s.fm_ops.pan = (v as f32).clamp(-1.0, 1.0);
    }
    if !locked.contains("fm_ops.algorithm")
        && let Some(v) = f.get("algorithm").and_then(|v| v.as_u64())
    {
        s.fm_ops.algorithm = (v as u8).min(crate::state::FM_ALGORITHM_COUNT - 1);
    }
    s.fm_ops.feedback = unlocked_f32(s.fm_ops.feedback, f, "feedback", "fm_ops.feedback", locked);
    if let Some(o) = f.get("op1").and_then(|v| v.as_object()) {
        apply_fm_op(&mut s.fm_ops.op1, o, locked, "fm_ops.op1");
    }
    if let Some(o) = f.get("op2").and_then(|v| v.as_object()) {
        apply_fm_op(&mut s.fm_ops.op2, o, locked, "fm_ops.op2");
    }
    if let Some(o) = f.get("op3").and_then(|v| v.as_object()) {
        apply_fm_op(&mut s.fm_ops.op3, o, locked, "fm_ops.op3");
    }
    if let Some(o) = f.get("op4").and_then(|v| v.as_object()) {
        apply_fm_op(&mut s.fm_ops.op4, o, locked, "fm_ops.op4");
    }
    if !locked.contains("sequencer.fm_ops_steps")
        && let Some(arr) = f.get("fm_ops_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(a) = val.as_bool() {
                s.sequencer.fm_ops_pattern[i].active = a;
            }
        }
    }
    if !locked.contains("sequencer.fm_ops_notes")
        && let Some(arr) = f.get("fm_ops_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.fm_ops_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply Additive synth fields from an LLM JSON update object.
/// Voice params + 16-element `levels` array + sequencer pattern.
/// `levels` accepts a JSON array of numbers in `[0, 1]` — shorter
/// arrays leave the trailing partials untouched, longer arrays
/// drop the surplus.
pub(super) fn apply_additive_update(
    s: &mut AppState,
    a: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("additive.enabled")
        && let Some(v) = a.get("enabled").and_then(|v| v.as_bool())
    {
        s.additive.enabled = v;
    }
    s.additive.volume = unlocked_f32_range(
        s.additive.volume,
        a,
        "volume",
        "additive.volume",
        locked,
        0.0,
        1.5,
    );
    if !locked.contains("additive.pan")
        && let Some(v) = a.get("pan").and_then(|v| v.as_f64())
    {
        s.additive.pan = (v as f32).clamp(-1.0, 1.0);
    }
    s.additive.attack = unlocked_f32(s.additive.attack, a, "attack", "additive.attack", locked);
    s.additive.decay = unlocked_f32(s.additive.decay, a, "decay", "additive.decay", locked);
    s.additive.sustain = unlocked_f32(s.additive.sustain, a, "sustain", "additive.sustain", locked);
    s.additive.release = unlocked_f32(s.additive.release, a, "release", "additive.release", locked);
    if !locked.contains("additive.levels")
        && let Some(arr) = a.get("levels").and_then(|v| v.as_array())
    {
        for (i, val) in arr
            .iter()
            .enumerate()
            .take(crate::state::ADDITIVE_HARMONICS)
        {
            if let Some(f) = val.as_f64() {
                s.additive.levels[i] = (f as f32).clamp(0.0, 1.0);
            }
        }
    }
    if !locked.contains("sequencer.additive_steps")
        && let Some(arr) = a.get("additive_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(b) = val.as_bool() {
                s.sequencer.additive_pattern[i].active = b;
            }
        }
    }
    if !locked.contains("sequencer.additive_notes")
        && let Some(arr) = a.get("additive_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.additive_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply Modal voice fields from an LLM JSON update object.
/// Voice params + 8-element `levels` array + sequencer pattern +
/// preset / brightness / decay-scale knobs.  `levels` semantics
/// match the Additive helper: shorter arrays leave trailing
/// modes alone, locks honoured per field.
pub(super) fn apply_modal_update(
    s: &mut AppState,
    m: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("modal.enabled")
        && let Some(v) = m.get("enabled").and_then(|v| v.as_bool())
    {
        s.modal.enabled = v;
    }
    s.modal.volume = unlocked_f32_range(
        s.modal.volume,
        m,
        "volume",
        "modal.volume",
        locked,
        0.0,
        1.5,
    );
    if !locked.contains("modal.pan")
        && let Some(v) = m.get("pan").and_then(|v| v.as_f64())
    {
        s.modal.pan = (v as f32).clamp(-1.0, 1.0);
    }
    s.modal.brightness = unlocked_f32(
        s.modal.brightness,
        m,
        "brightness",
        "modal.brightness",
        locked,
    );
    s.modal.decay_scale = unlocked_f32(
        s.modal.decay_scale,
        m,
        "decay_scale",
        "modal.decay_scale",
        locked,
    );
    if !locked.contains("modal.ratio_preset")
        && let Some(v) = m.get("ratio_preset").and_then(|v| v.as_u64())
    {
        s.modal.ratio_preset = (v as u8).min(crate::state::MODAL_RATIO_PRESETS - 1);
    }
    if !locked.contains("modal.levels")
        && let Some(arr) = m.get("levels").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(crate::state::MODAL_MODES) {
            if let Some(f) = val.as_f64() {
                s.modal.levels[i] = (f as f32).clamp(0.0, 1.0);
            }
        }
    }
    if !locked.contains("sequencer.modal_steps")
        && let Some(arr) = m.get("modal_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(b) = val.as_bool() {
                s.sequencer.modal_pattern[i].active = b;
            }
        }
    }
    if !locked.contains("sequencer.modal_notes")
        && let Some(arr) = m.get("modal_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.modal_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply per-osc fields for one chiptune oscillator.  Called
/// three times by `apply_chiptune_update` with the JSON sub-
/// object for each osc.
fn apply_chiptune_osc(
    osc: &mut crate::state::SidOsc,
    obj: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
    op_path: &str,
) {
    if !locked.contains(&format!("{op_path}.waveform"))
        && let Some(v) = obj.get("waveform").and_then(|v| v.as_u64())
    {
        osc.waveform = (v as u8).min(crate::state::CHIPTUNE_WAVEFORMS - 1);
    }
    osc.level = unlocked_f32(osc.level, obj, "level", &format!("{op_path}.level"), locked);
    osc.attack = unlocked_f32(
        osc.attack,
        obj,
        "attack",
        &format!("{op_path}.attack"),
        locked,
    );
    osc.decay = unlocked_f32(osc.decay, obj, "decay", &format!("{op_path}.decay"), locked);
    osc.sustain = unlocked_f32(
        osc.sustain,
        obj,
        "sustain",
        &format!("{op_path}.sustain"),
        locked,
    );
    osc.release = unlocked_f32(
        osc.release,
        obj,
        "release",
        &format!("{op_path}.release"),
        locked,
    );
}

/// Apply Chiptune (SID-flavoured) voice fields from an LLM JSON
/// update object.  Voice fields + 3 nested osc objects + filter
/// + flags + sequencer pattern.
pub(super) fn apply_chiptune_update(
    s: &mut AppState,
    c: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("chiptune.enabled")
        && let Some(v) = c.get("enabled").and_then(|v| v.as_bool())
    {
        s.chiptune.enabled = v;
    }
    s.chiptune.volume = unlocked_f32_range(
        s.chiptune.volume,
        c,
        "volume",
        "chiptune.volume",
        locked,
        0.0,
        1.5,
    );
    if !locked.contains("chiptune.pan")
        && let Some(v) = c.get("pan").and_then(|v| v.as_f64())
    {
        s.chiptune.pan = (v as f32).clamp(-1.0, 1.0);
    }
    if let Some(o) = c.get("osc1").and_then(|v| v.as_object()) {
        apply_chiptune_osc(&mut s.chiptune.osc1, o, locked, "chiptune.osc1");
    }
    if let Some(o) = c.get("osc2").and_then(|v| v.as_object()) {
        apply_chiptune_osc(&mut s.chiptune.osc2, o, locked, "chiptune.osc2");
    }
    if let Some(o) = c.get("osc3").and_then(|v| v.as_object()) {
        apply_chiptune_osc(&mut s.chiptune.osc3, o, locked, "chiptune.osc3");
    }
    s.chiptune.pulse_width = unlocked_f32_range(
        s.chiptune.pulse_width,
        c,
        "pulse_width",
        "chiptune.pulse_width",
        locked,
        0.05,
        0.95,
    );
    s.chiptune.filter_cutoff = unlocked_f32(
        s.chiptune.filter_cutoff,
        c,
        "filter_cutoff",
        "chiptune.filter_cutoff",
        locked,
    );
    s.chiptune.filter_resonance = unlocked_f32(
        s.chiptune.filter_resonance,
        c,
        "filter_resonance",
        "chiptune.filter_resonance",
        locked,
    );
    if !locked.contains("chiptune.filter_mode")
        && let Some(v) = c.get("filter_mode").and_then(|v| v.as_u64())
    {
        s.chiptune.filter_mode = (v as u8).min(crate::state::CHIPTUNE_FILTER_MODES - 1);
    }
    s.chiptune.filter_mix = unlocked_f32(
        s.chiptune.filter_mix,
        c,
        "filter_mix",
        "chiptune.filter_mix",
        locked,
    );
    if !locked.contains("chiptune.ring_mod")
        && let Some(v) = c.get("ring_mod").and_then(|v| v.as_bool())
    {
        s.chiptune.ring_mod = v;
    }
    if !locked.contains("chiptune.sync")
        && let Some(v) = c.get("sync").and_then(|v| v.as_bool())
    {
        s.chiptune.sync = v;
    }
    if !locked.contains("sequencer.chiptune_steps")
        && let Some(arr) = c.get("chiptune_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(b) = val.as_bool() {
                s.sequencer.chiptune_pattern[i].active = b;
            }
        }
    }
    if !locked.contains("sequencer.chiptune_notes")
        && let Some(arr) = c.get("chiptune_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.chiptune_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}
