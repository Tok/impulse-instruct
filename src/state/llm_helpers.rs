// ─── state/llm_helpers.rs ────────────────────────────────────────────────────
// Per-voice LLM update helpers extracted from apply_llm_update to keep
// transitions.rs under the 1000-line limit.

use std::collections::HashSet;

use super::{AppState, FilterMode, MAX_STEPS, Waveform};

/// Returns the updated value if not locked, otherwise returns the original.
pub(super) fn unlocked_f32(
    current: f32,
    obj: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    path: &str,
    locked: &HashSet<String>,
) -> f32 {
    if locked.contains(path) {
        return current;
    }
    obj.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| (v as f32).clamp(0.0, 1.0))
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
    u!(s.fx.stereo_width, "stereo_width", "fx.stereo_width");
    u!(s.fx.tape_drive, "tape_drive", "fx.tape_drive");
    u!(s.fx.tape_mix, "tape_mix", "fx.tape_mix");
    u!(s.fx.tape_flutter, "tape_flutter", "fx.tape_flutter");
    u!(
        s.fx.autotune_amount,
        "autotune_amount",
        "fx.autotune_amount"
    );
    u!(s.fx.autotune_mix, "autotune_mix", "fx.autotune_mix");
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
}
