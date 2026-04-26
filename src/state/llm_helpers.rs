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
    // "lfo_target": "off" | "pitch" | "pwm" | "cutoff" | "amp" | "pan"
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
            "pan" | "stereo" => BassLfoTarget::Pan,
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
    let bp = format!("{}.lfo_phase", prefix);
    let v = s.bass_voices[voice_idx].synth.lfo_phase;
    s.bass_voices[voice_idx].synth.lfo_phase = unlocked_f32(v, b, "lfo_phase", &bp, locked);
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

/// Apply SampleInstrument fields from an LLM JSON update object.
/// Mirrors `apply_wavetable_update` — voice params + sequencer pattern.
pub(super) fn apply_sample_instrument_update(
    s: &mut AppState,
    w: &serde_json::Map<String, serde_json::Value>,
    locked: &HashSet<String>,
) {
    if !locked.contains("sample.enabled")
        && let Some(v) = w.get("enabled").and_then(|v| v.as_bool())
    {
        s.sample_instrument.enabled = v;
    }
    if !locked.contains("sample.root_note")
        && let Some(v) = w.get("root_note").and_then(|v| v.as_u64())
    {
        s.sample_instrument.root_note = v.clamp(0, 127) as u8;
    }
    s.sample_instrument.volume = unlocked_f32(
        s.sample_instrument.volume,
        w,
        "volume",
        "sample.volume",
        locked,
    );
    if !locked.contains("sample.pan")
        && let Some(v) = w.get("pan").and_then(|v| v.as_f64())
    {
        s.sample_instrument.pan = (v as f32).clamp(-1.0, 1.0);
    }
    if !locked.contains("sample.pitch_offset_cents")
        && let Some(v) = w.get("pitch_offset_cents").and_then(|v| v.as_f64())
    {
        s.sample_instrument.pitch_offset_cents = (v as f32).clamp(-100.0, 100.0);
    }
    s.sample_instrument.attack = unlocked_f32(
        s.sample_instrument.attack,
        w,
        "attack",
        "sample.attack",
        locked,
    );
    s.sample_instrument.decay = unlocked_f32(
        s.sample_instrument.decay,
        w,
        "decay",
        "sample.decay",
        locked,
    );
    s.sample_instrument.sustain = unlocked_f32(
        s.sample_instrument.sustain,
        w,
        "sustain",
        "sample.sustain",
        locked,
    );
    s.sample_instrument.release = unlocked_f32(
        s.sample_instrument.release,
        w,
        "release",
        "sample.release",
        locked,
    );
    s.sample_instrument.loop_start = unlocked_f32(
        s.sample_instrument.loop_start,
        w,
        "loop_start",
        "sample.loop_start",
        locked,
    );
    s.sample_instrument.loop_end = unlocked_f32(
        s.sample_instrument.loop_end,
        w,
        "loop_end",
        "sample.loop_end",
        locked,
    );
    if !locked.contains("sample.loop_enabled")
        && let Some(v) = w.get("loop_enabled").and_then(|v| v.as_bool())
    {
        s.sample_instrument.loop_enabled = v;
    }
    s.sample_instrument.filter_cutoff = unlocked_f32(
        s.sample_instrument.filter_cutoff,
        w,
        "filter_cutoff",
        "sample.filter_cutoff",
        locked,
    );
    s.sample_instrument.filter_resonance = unlocked_f32(
        s.sample_instrument.filter_resonance,
        w,
        "filter_resonance",
        "sample.filter_resonance",
        locked,
    );
    s.sample_instrument.filter_mix = unlocked_f32(
        s.sample_instrument.filter_mix,
        w,
        "filter_mix",
        "sample.filter_mix",
        locked,
    );
    if !locked.contains("sample.filter_mode")
        && let Some(v) = w.get("filter_mode").and_then(|v| v.as_u64())
    {
        s.sample_instrument.filter_mode = (v as u8).min(2);
    }
    if !locked.contains("sample.formant_preserve")
        && let Some(v) = w.get("formant_preserve").and_then(|v| v.as_bool())
    {
        s.sample_instrument.formant_preserve = v;
    }
    if !locked.contains("sample.time_stretch")
        && let Some(v) = w.get("time_stretch").and_then(|v| v.as_f64())
    {
        s.sample_instrument.time_stretch = (v as f32).clamp(0.25, 4.0);
    }
    if !locked.contains("sequencer.sample_steps")
        && let Some(arr) = w.get("sample_steps").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(a) = val.as_bool() {
                s.sequencer.sample_pattern[i].active = a;
            }
        }
    }
    if !locked.contains("sequencer.sample_notes")
        && let Some(arr) = w.get("sample_notes").and_then(|v| v.as_array())
    {
        for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
            if let Some(n) = val.as_u64() {
                s.sequencer.sample_pattern[i].note = n.clamp(0, 127) as u8;
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
