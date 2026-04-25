// ─── state/llm_helpers.rs ────────────────────────────────────────────────────
// Per-voice LLM update helpers extracted from apply_llm_update to keep
// transitions.rs under the 1000-line limit.

use std::collections::HashSet;

use super::{AppState, FilterMode, MAX_STEPS, Waveform};

/// Returns the updated value if not locked, otherwise returns the original.
/// Clamps to `[0, 1]` — the right call for the bulk of synth knobs.  For
/// fields outside that range (semitone offsets, BPM, etc.) use
/// [`unlocked_f32_range`] instead so the value isn't pre-clamped to `[0, 1]`
/// and then re-clamped to its real range (which silently pinned every
/// non-unit field to its lower bound — see history of this function).
pub(super) fn unlocked_f32(
    current: f32,
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
    locked: &HashSet<String>,
) -> f32 {
    unlocked_f32_range(current, obj, key, path, locked, 0.0, 1.0)
}

/// Generic version of [`unlocked_f32`] that clamps the parsed value to a
/// caller-supplied range.  Use this for any field whose valid range is not
/// `[0, 1]` (e.g. `amen.pitch ∈ [-24, 24]`, `amen.source_bpm ∈ [40, 300]`).
pub(crate) fn unlocked_f32_range(
    current: f32,
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
    locked: &HashSet<String>,
    min: f32,
    max: f32,
) -> f32 {
    if locked.contains(path) {
        return current;
    }
    obj.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| (v as f32).clamp(min, max))
        .unwrap_or(current)
}

/// Apply bass synth fields from an LLM JSON update object for a given voice index.
/// The lock path prefix is "bass" for voice 0 (legacy compat) or "bass_voices.N" for N>0.
pub(super) fn apply_bass_update(
    s: &mut AppState,
    b: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
    voice_idx: usize,
) {
    if voice_idx >= s.bass_voices.len() {
        return;
    }
    // Use "bass" prefix for voice 0 (legacy compat), "bass_voices.N" for others
    let prefix = if voice_idx == 0 {
        "bass".to_string()
    } else {
        format!("bass_voices.{}", voice_idx)
    };
    let bp = format!("{}.cutoff", prefix);
    let v = s.bass_voices[voice_idx].synth.cutoff;
    s.bass_voices[voice_idx].synth.cutoff = unlocked_f32(v, b, "cutoff", &bp, locked);
    let bp = format!("{}.resonance", prefix);
    let v = s.bass_voices[voice_idx].synth.resonance;
    s.bass_voices[voice_idx].synth.resonance = unlocked_f32(v, b, "resonance", &bp, locked);
    let bp = format!("{}.env_mod", prefix);
    let v = s.bass_voices[voice_idx].synth.env_mod;
    s.bass_voices[voice_idx].synth.env_mod = unlocked_f32(v, b, "env_mod", &bp, locked);
    let bp = format!("{}.decay", prefix);
    let v = s.bass_voices[voice_idx].synth.decay;
    s.bass_voices[voice_idx].synth.decay = unlocked_f32(v, b, "decay", &bp, locked);
    let bp = format!("{}.accent_level", prefix);
    let v = s.bass_voices[voice_idx].synth.accent_level;
    s.bass_voices[voice_idx].synth.accent_level = unlocked_f32(v, b, "accent_level", &bp, locked);
    let bp = format!("{}.distortion", prefix);
    let v = s.bass_voices[voice_idx].synth.distortion;
    s.bass_voices[voice_idx].synth.distortion = unlocked_f32(v, b, "distortion", &bp, locked);
    let bp = format!("{}.volume", prefix);
    let v = s.bass_voices[voice_idx].synth.volume;
    s.bass_voices[voice_idx].synth.volume = unlocked_f32(v, b, "volume", &bp, locked);
    let bp = format!("{}.supersaw_detune", prefix);
    let v = s.bass_voices[voice_idx].synth.supersaw_detune;
    s.bass_voices[voice_idx].synth.supersaw_detune =
        unlocked_f32(v, b, "supersaw_detune", &bp, locked);
    let bp = format!("{}.supersaw_voices", prefix);
    if let Some(v) = b.get("supersaw_voices").and_then(|v| v.as_u64())
        && !locked.contains(&bp)
    {
        s.bass_voices[voice_idx].synth.supersaw_voices = (v as u8).clamp(2, 7);
    }
    let bp = format!("{}.sub_osc_level", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("sub_osc_level").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.sub_osc_level = (v as f32).clamp(0.0, 1.0);
    }
    let bp = format!("{}.portamento_time", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("portamento_time").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.portamento_time = (v as f32).clamp(0.0, 1.0);
    }
    let bp = format!("{}.noise_mix", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("noise_mix").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.noise_mix = (v as f32).clamp(0.0, 1.0);
    }
    let bp = format!("{}.osc_detune", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("osc_detune").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.osc_detune = (v as f32).clamp(-1.0, 1.0);
    }
    let bp = format!("{}.fm_ratio", prefix);
    let v = s.bass_voices[voice_idx].synth.fm_ratio;
    s.bass_voices[voice_idx].synth.fm_ratio = unlocked_f32(v, b, "fm_ratio", &bp, locked);
    let bp = format!("{}.fm_depth", prefix);
    let v = s.bass_voices[voice_idx].synth.fm_depth;
    s.bass_voices[voice_idx].synth.fm_depth = unlocked_f32(v, b, "fm_depth", &bp, locked);
    let bp = format!("{}.pan", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("pan").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.pan = (v as f32).clamp(-1.0, 1.0);
    }
    let bp = format!("{}.waveform", prefix);
    if !locked.contains(&bp)
        && let Some(w) = b.get("waveform").and_then(|v| v.as_str())
    {
        s.bass_voices[voice_idx].synth.waveform = match w {
            "Square" => Waveform::Square,
            "Supersaw" => Waveform::Supersaw,
            _ => Waveform::Saw,
        };
    }
    let bp = format!("{}.filter_mode", prefix);
    if !locked.contains(&bp)
        && let Some(m) = b.get("filter_mode").and_then(|v| v.as_str())
    {
        s.bass_voices[voice_idx].synth.filter_mode = match m {
            "Highpass" | "HP" => FilterMode::Highpass,
            "Bandpass" | "BP" => FilterMode::Bandpass,
            _ => FilterMode::Lowpass,
        };
    }
    // Handle enabled flag for voices 1-3
    let bp = format!("{}.enabled", prefix);
    if voice_idx > 0
        && !locked.contains(&bp)
        && let Some(v) = b.get("enabled").and_then(|v| v.as_bool())
    {
        s.bass_voices[voice_idx].enabled = v;
    }
    // ── ADSR + PWM (101-style shaping) ───────────────────────────────────
    let bp = format!("{}.amp_attack", prefix);
    let v = s.bass_voices[voice_idx].synth.amp_attack;
    s.bass_voices[voice_idx].synth.amp_attack = unlocked_f32(v, b, "amp_attack", &bp, locked);
    let bp = format!("{}.amp_sustain", prefix);
    let v = s.bass_voices[voice_idx].synth.amp_sustain;
    s.bass_voices[voice_idx].synth.amp_sustain = unlocked_f32(v, b, "amp_sustain", &bp, locked);
    let bp = format!("{}.amp_release", prefix);
    let v = s.bass_voices[voice_idx].synth.amp_release;
    s.bass_voices[voice_idx].synth.amp_release = unlocked_f32(v, b, "amp_release", &bp, locked);
    let bp = format!("{}.filter_attack", prefix);
    let v = s.bass_voices[voice_idx].synth.filter_attack;
    s.bass_voices[voice_idx].synth.filter_attack = unlocked_f32(v, b, "filter_attack", &bp, locked);
    let bp = format!("{}.filter_sustain", prefix);
    let v = s.bass_voices[voice_idx].synth.filter_sustain;
    s.bass_voices[voice_idx].synth.filter_sustain =
        unlocked_f32(v, b, "filter_sustain", &bp, locked);
    let bp = format!("{}.filter_release", prefix);
    let v = s.bass_voices[voice_idx].synth.filter_release;
    s.bass_voices[voice_idx].synth.filter_release =
        unlocked_f32(v, b, "filter_release", &bp, locked);
    let bp = format!("{}.pulse_width", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("pulse_width").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.pulse_width = (v as f32).clamp(0.05, 0.95);
    }

    // ── Per-voice LFO (101-style) ────────────────────────────────────────
    // "lfo_target": "off" | "pitch" | "pwm" | "cutoff" | "amp"
    let bp = format!("{}.lfo_target", prefix);
    if !locked.contains(&bp)
        && let Some(t) = b.get("lfo_target").and_then(|v| v.as_str())
    {
        use crate::state::BassLfoTarget;
        s.bass_voices[voice_idx].synth.lfo_target = match t.to_ascii_lowercase().as_str() {
            "pitch" => BassLfoTarget::Pitch,
            "pwm" | "pulse_width" | "pulse" => BassLfoTarget::PulseWidth,
            "cutoff" | "filter" | "filter_cutoff" => BassLfoTarget::FilterCutoff,
            "amp" | "amplitude" | "tremolo" => BassLfoTarget::Amplitude,
            _ => BassLfoTarget::Off,
        };
    }
    let bp = format!("{}.lfo_rate", prefix);
    let v = s.bass_voices[voice_idx].synth.lfo_rate;
    s.bass_voices[voice_idx].synth.lfo_rate = unlocked_f32(v, b, "lfo_rate", &bp, locked);
    let bp = format!("{}.lfo_depth", prefix);
    let v = s.bass_voices[voice_idx].synth.lfo_depth;
    s.bass_voices[voice_idx].synth.lfo_depth = unlocked_f32(v, b, "lfo_depth", &bp, locked);
    let bp = format!("{}.lfo_delay", prefix);
    let v = s.bass_voices[voice_idx].synth.lfo_delay;
    s.bass_voices[voice_idx].synth.lfo_delay = unlocked_f32(v, b, "lfo_delay", &bp, locked);
    // "lfo_waveform": "sine" | "triangle" | "saw" | "inv_saw" | "square"
    let bp = format!("{}.lfo_waveform", prefix);
    if !locked.contains(&bp)
        && let Some(w) = b.get("lfo_waveform").and_then(|v| v.as_str())
    {
        use crate::state::LfoWaveform;
        s.bass_voices[voice_idx].synth.lfo_waveform = match w.to_ascii_lowercase().as_str() {
            "triangle" | "tri" => LfoWaveform::Triangle,
            "saw" => LfoWaveform::Saw,
            "inv_saw" | "invsaw" | "ramp_down" => LfoWaveform::InvSaw,
            "square" | "pulse" => LfoWaveform::Square,
            _ => LfoWaveform::Sine,
        };
    }
    let bp = format!("{}.lfo_bpm_sync", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("lfo_bpm_sync").and_then(|v| v.as_bool())
    {
        s.bass_voices[voice_idx].synth.lfo_bpm_sync = v;
    }
    let bp = format!("{}.lfo_sync_beats", prefix);
    if !locked.contains(&bp)
        && let Some(v) = b.get("lfo_sync_beats").and_then(|v| v.as_f64())
    {
        s.bass_voices[voice_idx].synth.lfo_sync_beats = (v as f32).clamp(0.03125, 16.0);
    }
}

/// Apply hoover voice fields from an LLM JSON update object.
pub(super) fn apply_hoover_update(
    s: &mut AppState,
    h: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("hoover.enabled")
        && let Some(v) = h.get("enabled").and_then(|v| v.as_bool())
    {
        s.hoover.enabled = v;
    }
    s.hoover.filter_start = unlocked_f32(
        s.hoover.filter_start,
        h,
        "filter_start",
        "hoover.filter_start",
        locked,
    );
    s.hoover.sweep_time = unlocked_f32(
        s.hoover.sweep_time,
        h,
        "sweep_time",
        "hoover.sweep_time",
        locked,
    );
    s.hoover.resonance = unlocked_f32(
        s.hoover.resonance,
        h,
        "resonance",
        "hoover.resonance",
        locked,
    );
    s.hoover.detune = unlocked_f32(s.hoover.detune, h, "detune", "hoover.detune", locked);
    s.hoover.volume = unlocked_f32(s.hoover.volume, h, "volume", "hoover.volume", locked);
    if !locked.contains("hoover.pan")
        && let Some(v) = h.get("pan").and_then(|v| v.as_f64())
    {
        s.hoover.pan = (v as f32).clamp(-1.0, 1.0);
    }
    s.hoover.pitch_lfo_rate = unlocked_f32(
        s.hoover.pitch_lfo_rate,
        h,
        "pitch_lfo_rate",
        "hoover.pitch_lfo_rate",
        locked,
    );
    s.hoover.pitch_lfo_depth = unlocked_f32(
        s.hoover.pitch_lfo_depth,
        h,
        "pitch_lfo_depth",
        "hoover.pitch_lfo_depth",
        locked,
    );
    if !locked.contains("hoover.voices")
        && let Some(v) = h.get("voices").and_then(|v| v.as_u64())
    {
        s.hoover.voices = (v as u8).clamp(2, 7);
    }
    if !locked.contains("sequencer.hoover_steps")
        && let Some(arr) = h.get("hoover_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(a) = val.as_bool() {
                s.sequencer.hoover_pattern[i].active = a;
            }
        }
    }
    if !locked.contains("sequencer.hoover_notes")
        && let Some(arr) = h.get("hoover_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.hoover_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply Karplus-Strong pluck-voice fields from an LLM JSON update
/// object.  Mirrors `apply_hoover_update`: voice params plus the
/// sequencer's pluck_steps/pluck_notes arrays.
pub(super) fn apply_pluck_update(
    s: &mut AppState,
    pl: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("pluck.enabled")
        && let Some(v) = pl.get("enabled").and_then(|v| v.as_bool())
    {
        s.pluck.enabled = v;
    }
    s.pluck.damping = unlocked_f32(s.pluck.damping, pl, "damping", "pluck.damping", locked);
    s.pluck.brightness = unlocked_f32(
        s.pluck.brightness,
        pl,
        "brightness",
        "pluck.brightness",
        locked,
    );
    s.pluck.volume = unlocked_f32(s.pluck.volume, pl, "volume", "pluck.volume", locked);
    if !locked.contains("pluck.pan")
        && let Some(v) = pl.get("pan").and_then(|v| v.as_f64())
    {
        s.pluck.pan = (v as f32).clamp(-1.0, 1.0);
    }
    if !locked.contains("pluck.pitch_offset_semi")
        && let Some(v) = pl.get("pitch_offset_semi").and_then(|v| v.as_f64())
    {
        s.pluck.pitch_offset_semi = (v as f32).clamp(-24.0, 24.0);
    }
    if !locked.contains("sequencer.pluck_steps")
        && let Some(arr) = pl.get("pluck_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(a) = val.as_bool() {
                s.sequencer.pluck_pattern[i].active = a;
            }
        }
    }
    if !locked.contains("sequencer.pluck_notes")
        && let Some(arr) = pl.get("pluck_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.pluck_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply Wavetable voice fields from an LLM JSON update object.
/// Mirrors `apply_pluck_update`: voice params + sequencer pattern.
pub(super) fn apply_wavetable_update(
    s: &mut AppState,
    w: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("wavetable.enabled")
        && let Some(v) = w.get("enabled").and_then(|v| v.as_bool())
    {
        s.wavetable.enabled = v;
    }
    s.wavetable.position = unlocked_f32(
        s.wavetable.position,
        w,
        "position",
        "wavetable.position",
        locked,
    );
    s.wavetable.phase_offset = unlocked_f32(
        s.wavetable.phase_offset,
        w,
        "phase_offset",
        "wavetable.phase_offset",
        locked,
    );
    s.wavetable.volume = unlocked_f32(s.wavetable.volume, w, "volume", "wavetable.volume", locked);
    if !locked.contains("wavetable.pan")
        && let Some(v) = w.get("pan").and_then(|v| v.as_f64())
    {
        s.wavetable.pan = (v as f32).clamp(-1.0, 1.0);
    }
    if !locked.contains("wavetable.pitch_offset_semi")
        && let Some(v) = w.get("pitch_offset_semi").and_then(|v| v.as_f64())
    {
        s.wavetable.pitch_offset_semi = (v as f32).clamp(-24.0, 24.0);
    }
    if !locked.contains("sequencer.wavetable_steps")
        && let Some(arr) = w.get("wavetable_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(a) = val.as_bool() {
                s.sequencer.wavetable_pattern[i].active = a;
            }
        }
    }
    if !locked.contains("sequencer.wavetable_notes")
        && let Some(arr) = w.get("wavetable_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.wavetable_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply AN1X voice fields from an LLM JSON update object.
pub(super) fn apply_an1x_update(
    s: &mut AppState,
    a: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("an1x.enabled")
        && let Some(v) = a.get("enabled").and_then(|v| v.as_bool())
    {
        s.an1x.enabled = v;
    }
    s.an1x.volume = unlocked_f32(s.an1x.volume, a, "volume", "an1x.volume", locked);
    if !locked.contains("an1x.pan")
        && let Some(v) = a.get("pan").and_then(|v| v.as_f64())
    {
        s.an1x.pan = (v as f32).clamp(-1.0, 1.0);
    }
    s.an1x.osc1_level = unlocked_f32(
        s.an1x.osc1_level,
        a,
        "osc1_level",
        "an1x.osc1_level",
        locked,
    );
    s.an1x.osc2_level = unlocked_f32(
        s.an1x.osc2_level,
        a,
        "osc2_level",
        "an1x.osc2_level",
        locked,
    );
    s.an1x.osc2_detune = unlocked_f32(
        s.an1x.osc2_detune,
        a,
        "osc2_detune",
        "an1x.osc2_detune",
        locked,
    );
    s.an1x.sub_level = unlocked_f32(s.an1x.sub_level, a, "sub_level", "an1x.sub_level", locked);
    s.an1x.filter_cutoff = unlocked_f32(
        s.an1x.filter_cutoff,
        a,
        "filter_cutoff",
        "an1x.filter_cutoff",
        locked,
    );
    s.an1x.filter_resonance = unlocked_f32(
        s.an1x.filter_resonance,
        a,
        "filter_resonance",
        "an1x.filter_resonance",
        locked,
    );
    s.an1x.filter_env_amount = unlocked_f32(
        s.an1x.filter_env_amount,
        a,
        "filter_env_amount",
        "an1x.filter_env_amount",
        locked,
    );
    s.an1x.filter_attack = unlocked_f32(
        s.an1x.filter_attack,
        a,
        "filter_attack",
        "an1x.filter_attack",
        locked,
    );
    s.an1x.filter_decay = unlocked_f32(
        s.an1x.filter_decay,
        a,
        "filter_decay",
        "an1x.filter_decay",
        locked,
    );
    s.an1x.filter_sustain = unlocked_f32(
        s.an1x.filter_sustain,
        a,
        "filter_sustain",
        "an1x.filter_sustain",
        locked,
    );
    s.an1x.filter_release = unlocked_f32(
        s.an1x.filter_release,
        a,
        "filter_release",
        "an1x.filter_release",
        locked,
    );
    s.an1x.amp_attack = unlocked_f32(
        s.an1x.amp_attack,
        a,
        "amp_attack",
        "an1x.amp_attack",
        locked,
    );
    s.an1x.amp_decay = unlocked_f32(s.an1x.amp_decay, a, "amp_decay", "an1x.amp_decay", locked);
    s.an1x.amp_sustain = unlocked_f32(
        s.an1x.amp_sustain,
        a,
        "amp_sustain",
        "an1x.amp_sustain",
        locked,
    );
    s.an1x.amp_release = unlocked_f32(
        s.an1x.amp_release,
        a,
        "amp_release",
        "an1x.amp_release",
        locked,
    );
    s.an1x.lfo_rate = unlocked_f32(s.an1x.lfo_rate, a, "lfo_rate", "an1x.lfo_rate", locked);
    s.an1x.lfo_depth = unlocked_f32(s.an1x.lfo_depth, a, "lfo_depth", "an1x.lfo_depth", locked);
    s.an1x.lfo_delay = unlocked_f32(s.an1x.lfo_delay, a, "lfo_delay", "an1x.lfo_delay", locked);
    s.an1x.lfo_sync_beats = unlocked_f32(
        s.an1x.lfo_sync_beats,
        a,
        "lfo_sync_beats",
        "an1x.lfo_sync_beats",
        locked,
    );
    if !locked.contains("an1x.lfo_bpm_sync")
        && let Some(v) = a.get("lfo_bpm_sync").and_then(|v| v.as_bool())
    {
        s.an1x.lfo_bpm_sync = v;
    }
    if !locked.contains("an1x.hard_sync")
        && let Some(v) = a.get("hard_sync").and_then(|v| v.as_bool())
    {
        s.an1x.hard_sync = v;
    }
    s.an1x.pitch_env_attack = unlocked_f32(
        s.an1x.pitch_env_attack,
        a,
        "pitch_env_attack",
        "an1x.pitch_env_attack",
        locked,
    );
    s.an1x.pitch_env_decay = unlocked_f32(
        s.an1x.pitch_env_decay,
        a,
        "pitch_env_decay",
        "an1x.pitch_env_decay",
        locked,
    );
    s.an1x.pitch_env_amount = unlocked_f32(
        s.an1x.pitch_env_amount,
        a,
        "pitch_env_amount",
        "an1x.pitch_env_amount",
        locked,
    );
    s.an1x.drift = unlocked_f32(s.an1x.drift, a, "drift", "an1x.drift", locked);
    s.an1x.glide_time = unlocked_f32(
        s.an1x.glide_time,
        a,
        "glide_time",
        "an1x.glide_time",
        locked,
    );
    if !locked.contains("sequencer.an1x_steps")
        && let Some(arr) = a.get("an1x_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(on) = val.as_bool() {
                s.sequencer.an1x_pattern[i].active = on;
            }
        }
    }
    if !locked.contains("sequencer.an1x_notes")
        && let Some(arr) = a.get("an1x_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.an1x_pattern[i].note = n.clamp(0, 127) as u8;
            }
        }
    }
}

/// Apply FX fields from an LLM JSON update object.
pub(super) fn apply_fx_update(
    s: &mut AppState,
    fx: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    macro_rules! u {
        ($field:expr, $key:literal, $path:literal) => {
            $field = unlocked_f32($field, fx, $key, $path, locked);
        };
    }
    u!(s.fx.reverb_size, "reverb_size", "fx.reverb_size");
    u!(s.fx.reverb_damp, "reverb_damp", "fx.reverb_damp");
    u!(s.fx.reverb_mix, "reverb_mix", "fx.reverb_mix");
    u!(
        s.fx.reverb_gate_time,
        "reverb_gate_time",
        "fx.reverb_gate_time"
    );
    if !locked.contains("fx.reverb_freeze")
        && let Some(v) = fx.get("reverb_freeze").and_then(|v| v.as_bool())
    {
        s.fx.reverb_freeze = v;
    }
    // master_pitch_st: -12..+12 stored raw; unlocked_f32 clamp is applied via min/max
    if let Some(v) = fx.get("master_pitch_st").and_then(|v| v.as_f64()) {
        let path = "fx.master_pitch_st";
        if !locked.contains(path) {
            s.fx.master_pitch_st = (v as f32).clamp(-12.0, 12.0);
        }
    }
    u!(s.fx.delay_time, "delay_time", "fx.delay_time");
    u!(s.fx.delay_feedback, "delay_feedback", "fx.delay_feedback");
    u!(s.fx.delay_mix, "delay_mix", "fx.delay_mix");
    u!(
        s.fx.delay_wow_flutter,
        "delay_wow_flutter",
        "fx.delay_wow_flutter"
    );
    u!(
        s.fx.delay_saturation,
        "delay_saturation",
        "fx.delay_saturation"
    );
    if !locked.contains("fx.delay_freeze")
        && let Some(v) = fx.get("delay_freeze").and_then(|v| v.as_bool())
    {
        s.fx.delay_freeze = v;
    }
    u!(s.fx.delay_hpf, "delay_hpf", "fx.delay_hpf");
    u!(s.fx.delay_lpf, "delay_lpf", "fx.delay_lpf");
    u!(
        s.fx.distortion_drive,
        "distortion_drive",
        "fx.distortion_drive"
    );
    u!(s.fx.distortion_mix, "distortion_mix", "fx.distortion_mix");
    u!(s.fx.bitcrush_bits, "bitcrush_bits", "fx.bitcrush_bits");
    u!(s.fx.bitcrush_rate, "bitcrush_rate", "fx.bitcrush_rate");
    u!(s.fx.bitcrush_mix, "bitcrush_mix", "fx.bitcrush_mix");
    u!(s.fx.chorus_rate, "chorus_rate", "fx.chorus_rate");
    u!(s.fx.chorus_depth, "chorus_depth", "fx.chorus_depth");
    u!(s.fx.chorus_mix, "chorus_mix", "fx.chorus_mix");
    u!(s.fx.phaser_rate, "phaser_rate", "fx.phaser_rate");
    u!(s.fx.phaser_depth, "phaser_depth", "fx.phaser_depth");
    u!(s.fx.phaser_mix, "phaser_mix", "fx.phaser_mix");
    u!(
        s.fx.waveshaper_drive,
        "waveshaper_drive",
        "fx.waveshaper_drive"
    );
    u!(s.fx.waveshaper_mix, "waveshaper_mix", "fx.waveshaper_mix");
    u!(s.fx.ring_mod_freq, "ring_mod_freq", "fx.ring_mod_freq");
    u!(s.fx.ring_mod_mix, "ring_mod_mix", "fx.ring_mod_mix");
    u!(s.fx.eq_low_gain, "eq_low_gain", "fx.eq_low_gain");
    u!(s.fx.eq_mid_gain, "eq_mid_gain", "fx.eq_mid_gain");
    u!(s.fx.eq_hi_gain, "eq_hi_gain", "fx.eq_hi_gain");
    u!(
        s.fx.compressor_threshold,
        "compressor_threshold",
        "fx.compressor_threshold"
    );
    u!(
        s.fx.compressor_ratio,
        "compressor_ratio",
        "fx.compressor_ratio"
    );
    u!(s.fx.compressor_mix, "compressor_mix", "fx.compressor_mix");
    u!(
        s.fx.compressor_multiband,
        "compressor_multiband",
        "fx.compressor_multiband"
    );
    if !locked.contains("fx.compressor_reverse")
        && let Some(v) = fx.get("compressor_reverse").and_then(|v| v.as_bool())
    {
        s.fx.compressor_reverse = v;
    }
    u!(s.fx.stereo_width, "stereo_width", "fx.stereo_width");
    if !locked.contains("fx.tuning")
        && let Some(v) = fx.get("tuning").and_then(|v| v.as_u64())
    {
        s.fx.tuning = (v as u8).min(3);
    }
    u!(s.fx.tape_drive, "tape_drive", "fx.tape_drive");
    u!(s.fx.tape_mix, "tape_mix", "fx.tape_mix");
    u!(s.fx.tape_flutter, "tape_flutter", "fx.tape_flutter");
    u!(
        s.fx.autotune_amount,
        "autotune_amount",
        "fx.autotune_amount"
    );
    u!(s.fx.autotune_mix, "autotune_mix", "fx.autotune_mix");
    u!(s.fx.fx_pan_pos, "fx_pan_pos", "fx.fx_pan_pos");
    u!(s.fx.fx_pan_width, "fx_pan_width", "fx.fx_pan_width");
    u!(s.fx.fx_pan_rate, "fx_pan_rate", "fx.fx_pan_rate");
    u!(
        s.fx.conv_reverb_mix,
        "conv_reverb_mix",
        "fx.conv_reverb_mix"
    );
    u!(
        s.fx.conv_reverb_size,
        "conv_reverb_size",
        "fx.conv_reverb_size"
    );
    u!(
        s.fx.conv_reverb_predelay,
        "conv_reverb_predelay",
        "fx.conv_reverb_predelay"
    );
    u!(
        s.fx.conv_reverb_damp,
        "conv_reverb_damp",
        "fx.conv_reverb_damp"
    );
    u!(
        s.fx.conv_reverb_lowcut,
        "conv_reverb_lowcut",
        "fx.conv_reverb_lowcut"
    );
    u!(
        s.fx.conv_reverb_width,
        "conv_reverb_width",
        "fx.conv_reverb_width"
    );
    if !locked.contains("fx.conv_reverb_reverse")
        && let Some(v) = fx.get("conv_reverb_reverse").and_then(|v| v.as_bool())
    {
        s.fx.conv_reverb_reverse = v;
    }

    // ── Parametric EQ bands ──────────────────────────────────────────────
    // `fx.param_eq_bands` is a positional sparse array — entries may be
    // null to skip that band, so the LLM can edit a single band without
    // re-emitting the whole 8-band set.  Each per-band field respects an
    // `fx.param_eq_bands.N.<field>` lock path, mirroring how
    // `bass_voices.N` edits gate per-field locks.
    if !locked.contains("fx.param_eq_bands")
        && let Some(arr) = fx.get("param_eq_bands").and_then(|v| v.as_array())
    {
        for (i, entry) in arr.iter().enumerate().take(s.fx.param_eq_bands.len()) {
            let Some(obj) = entry.as_object() else {
                continue;
            };
            let band = &mut s.fx.param_eq_bands[i];
            let lock_kind = format!("fx.param_eq_bands.{}.kind", i);
            let lock_freq = format!("fx.param_eq_bands.{}.freq", i);
            let lock_gain = format!("fx.param_eq_bands.{}.gain", i);
            let lock_q = format!("fx.param_eq_bands.{}.q", i);
            let lock_enabled = format!("fx.param_eq_bands.{}.enabled", i);
            if !locked.contains(&lock_kind)
                && let Some(k) = obj.get("kind").and_then(|v| v.as_u64())
            {
                band.kind = super::fx::ParamEqBandKind::from_u8(k as u8);
            }
            if !locked.contains(&lock_freq)
                && let Some(f) = obj.get("freq").and_then(|v| v.as_f64())
            {
                band.freq_hz = (f as f32).clamp(20.0, 20_000.0);
            }
            if !locked.contains(&lock_gain)
                && let Some(g) = obj.get("gain").and_then(|v| v.as_f64())
            {
                band.gain_db = (g as f32).clamp(-18.0, 18.0);
            }
            if !locked.contains(&lock_q)
                && let Some(q) = obj.get("q").and_then(|v| v.as_f64())
            {
                band.q = (q as f32).clamp(0.1, 10.0);
            }
            if !locked.contains(&lock_enabled)
                && let Some(e) = obj.get("enabled").and_then(|v| v.as_bool())
            {
                band.enabled = e;
            }
        }
    }

    // ── Pitch shifter (standalone bidirectional shifter, distinct
    //     from Autotune which is upward-only) ─────────────────────────
    if !locked.contains("fx.pitch_shift_semi")
        && let Some(v) = fx.get("pitch_shift_semi").and_then(|v| v.as_f64())
    {
        s.fx.pitch_shift_semi = (v as f32).clamp(-24.0, 24.0);
    }
    if !locked.contains("fx.pitch_shift_fine")
        && let Some(v) = fx.get("pitch_shift_fine").and_then(|v| v.as_f64())
    {
        s.fx.pitch_shift_fine = (v as f32).clamp(-100.0, 100.0);
    }
    u!(
        s.fx.pitch_shift_mix,
        "pitch_shift_mix",
        "fx.pitch_shift_mix"
    );
    u!(
        s.fx.pitch_shift_fbk,
        "pitch_shift_fbk",
        "fx.pitch_shift_fbk"
    );

    // ── Mid/side master knobs ───────────────────────────────────────────
    u!(s.fx.ms_mid_gain, "ms_mid_gain", "fx.ms_mid_gain");
    u!(s.fx.ms_mid_tilt, "ms_mid_tilt", "fx.ms_mid_tilt");
    u!(s.fx.ms_mid_sat, "ms_mid_sat", "fx.ms_mid_sat");
    u!(s.fx.ms_side_gain, "ms_side_gain", "fx.ms_side_gain");
    u!(s.fx.ms_side_tilt, "ms_side_tilt", "fx.ms_side_tilt");
    u!(s.fx.ms_side_sat, "ms_side_sat", "fx.ms_side_sat");

    u!(s.fx.master_volume, "master_volume", "fx.master_volume");
    u!(
        s.fx.xmod_bass_to_an1x_pitch,
        "xmod_bass_to_an1x_pitch",
        "fx.xmod_bass_to_an1x_pitch"
    );
    u!(
        s.fx.xmod_noise_to_filter,
        "xmod_noise_to_filter",
        "fx.xmod_noise_to_filter"
    );
    u!(
        s.fx.sidechain_amount,
        "sidechain_amount",
        "fx.sidechain_amount"
    );
    u!(
        s.fx.sidechain_attack,
        "sidechain_attack",
        "fx.sidechain_attack"
    );
    u!(
        s.fx.sidechain_release,
        "sidechain_release",
        "fx.sidechain_release"
    );

    // ── XY pad first-class paths ─────────────────────────────────────────
    // Each entry is `(xy_key, field_a, field_b, min, max)` — writing
    // `fx.<xy_key>: [x, y]` sets `field_a` to x and `field_b` to y,
    // respecting per-field locks *and* the `fx.<xy_key>` lock path.
    // Maps the canonical Pair-0 of each FX pad; Pair 1 / Pair 2 stay
    // reachable via the individual knob paths.
    type XyMap = (&'static str, &'static str, &'static str, f32, f32);
    const XY_PAIRS: &[XyMap] = &[
        ("reverb_xy", "reverb_size", "reverb_damp", 0.0, 1.0),
        ("delay_xy", "delay_time", "delay_feedback", 0.0, 1.0),
        ("chorus_xy", "chorus_rate", "chorus_depth", 0.0, 1.0),
        ("phaser_xy", "phaser_rate", "phaser_depth", 0.0, 1.0),
        ("ring_mod_xy", "ring_mod_freq", "ring_mod_mix", 0.0, 1.0),
        (
            "waveshaper_xy",
            "waveshaper_drive",
            "waveshaper_mix",
            0.0,
            1.0,
        ),
        ("bitcrush_xy", "bitcrush_bits", "bitcrush_rate", 0.0, 1.0),
        ("eq_xy", "eq_low_gain", "eq_mid_gain", -1.0, 1.0),
        (
            "compressor_xy",
            "compressor_threshold",
            "compressor_ratio",
            0.0,
            1.0,
        ),
        ("tape_xy", "tape_drive", "tape_flutter", 0.0, 1.0),
        (
            "distortion_xy",
            "distortion_drive",
            "distortion_mix",
            0.0,
            1.0,
        ),
        ("autotune_xy", "autotune_amount", "autotune_mix", 0.0, 1.0),
        ("fx_pan_xy", "fx_pan_pos", "fx_pan_width", 0.0, 1.0),
    ];
    for (xy_key, field_a, field_b, min, max) in XY_PAIRS {
        let Some(arr) = fx.get(*xy_key).and_then(|v| v.as_array()) else {
            continue;
        };
        if arr.len() != 2 {
            continue;
        }
        let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) else {
            continue;
        };
        let xy_path = format!("fx.{}", xy_key);
        if locked.contains(&xy_path) {
            continue;
        }
        let path_a = format!("fx.{}", field_a);
        let path_b = format!("fx.{}", field_b);
        let x = (x as f32).clamp(*min, *max);
        let y = (y as f32).clamp(*min, *max);
        if !locked.contains(&path_a)
            && let Some(dst) = fx_field_mut(&mut s.fx, field_a)
        {
            *dst = x;
        }
        if !locked.contains(&path_b)
            && let Some(dst) = fx_field_mut(&mut s.fx, field_b)
        {
            *dst = y;
        }
    }
}

/// Resolve an `FxState` field name to a mutable reference to that field.
/// Returns `None` for fields that aren't scalar `f32` knobs (booleans,
/// enum-ish `u8` selectors).  Kept in one place so the XY-pad apply loop
/// doesn't need to duplicate the big match.
fn fx_field_mut<'a>(fx: &'a mut super::FxState, key: &str) -> Option<&'a mut f32> {
    Some(match key {
        "reverb_size" => &mut fx.reverb_size,
        "reverb_damp" => &mut fx.reverb_damp,
        "reverb_mix" => &mut fx.reverb_mix,
        "delay_time" => &mut fx.delay_time,
        "delay_feedback" => &mut fx.delay_feedback,
        "delay_mix" => &mut fx.delay_mix,
        "chorus_rate" => &mut fx.chorus_rate,
        "chorus_depth" => &mut fx.chorus_depth,
        "chorus_mix" => &mut fx.chorus_mix,
        "phaser_rate" => &mut fx.phaser_rate,
        "phaser_depth" => &mut fx.phaser_depth,
        "phaser_mix" => &mut fx.phaser_mix,
        "ring_mod_freq" => &mut fx.ring_mod_freq,
        "ring_mod_mix" => &mut fx.ring_mod_mix,
        "waveshaper_drive" => &mut fx.waveshaper_drive,
        "waveshaper_mix" => &mut fx.waveshaper_mix,
        "bitcrush_bits" => &mut fx.bitcrush_bits,
        "bitcrush_rate" => &mut fx.bitcrush_rate,
        "bitcrush_mix" => &mut fx.bitcrush_mix,
        "eq_low_gain" => &mut fx.eq_low_gain,
        "eq_mid_gain" => &mut fx.eq_mid_gain,
        "eq_hi_gain" => &mut fx.eq_hi_gain,
        "compressor_threshold" => &mut fx.compressor_threshold,
        "compressor_ratio" => &mut fx.compressor_ratio,
        "compressor_mix" => &mut fx.compressor_mix,
        "tape_drive" => &mut fx.tape_drive,
        "tape_mix" => &mut fx.tape_mix,
        "tape_flutter" => &mut fx.tape_flutter,
        "distortion_drive" => &mut fx.distortion_drive,
        "distortion_mix" => &mut fx.distortion_mix,
        "autotune_amount" => &mut fx.autotune_amount,
        "autotune_mix" => &mut fx.autotune_mix,
        "fx_pan_pos" => &mut fx.fx_pan_pos,
        "fx_pan_width" => &mut fx.fx_pan_width,
        "fx_pan_rate" => &mut fx.fx_pan_rate,
        "conv_reverb_mix" => &mut fx.conv_reverb_mix,
        "conv_reverb_size" => &mut fx.conv_reverb_size,
        "conv_reverb_predelay" => &mut fx.conv_reverb_predelay,
        "conv_reverb_damp" => &mut fx.conv_reverb_damp,
        "conv_reverb_lowcut" => &mut fx.conv_reverb_lowcut,
        "conv_reverb_width" => &mut fx.conv_reverb_width,
        "pitch_shift_semi" => &mut fx.pitch_shift_semi,
        "pitch_shift_fine" => &mut fx.pitch_shift_fine,
        "pitch_shift_mix" => &mut fx.pitch_shift_mix,
        "pitch_shift_fbk" => &mut fx.pitch_shift_fbk,
        "ms_mid_gain" => &mut fx.ms_mid_gain,
        "ms_mid_tilt" => &mut fx.ms_mid_tilt,
        "ms_mid_sat" => &mut fx.ms_mid_sat,
        "ms_side_gain" => &mut fx.ms_side_gain,
        "ms_side_tilt" => &mut fx.ms_side_tilt,
        "ms_side_sat" => &mut fx.ms_side_sat,
        _ => return None,
    })
}
