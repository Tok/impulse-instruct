// ─── state/transitions.rs ────────────────────────────────────────────────────
// Pure state transition helpers extracted to keep state/mod.rs under 1000 lines.

use std::collections::HashSet;

use super::{
    AppState, DrumVoice, FilterMode, LfoTarget, LfoWaveform, MAX_STEPS, Scale, Waveform,
    snap_to_scale,
};

/// Set the active step count, tiling existing patterns into the new slots when expanding.
///
/// When going from 16 → 32 steps, steps 16–31 are filled by repeating the pattern from 0–15.
/// When going from 16 → 64, the 16-step pattern is repeated into all four banks.
/// Shrinking never erases data — the slots above the new count remain in memory (hidden).
/// Any LLM-provided pattern arrays applied *after* this call will overwrite the tiled values.
pub fn expand_sequencer_steps(state: AppState, new_steps: usize) -> AppState {
    let mut s = state;
    let old_steps = s.sequencer.steps;
    let new_steps = new_steps.clamp(1, MAX_STEPS);
    s.sequencer.steps = new_steps;

    if new_steps > old_steps && old_steps > 0 {
        // Tile bass pattern
        for i in old_steps..new_steps {
            s.sequencer.bass_pattern[i] = s.sequencer.bass_pattern[i % old_steps];
        }
        // Tile hoover pattern
        for i in old_steps..new_steps {
            s.sequencer.hoover_pattern[i] = s.sequencer.hoover_pattern[i % old_steps];
        }
        // Tile every drum voice
        let voices: Vec<DrumVoice> = s.sequencer.drum_patterns.keys().cloned().collect();
        for voice in voices {
            if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice) {
                for i in old_steps..new_steps {
                    pattern[i] = pattern[i % old_steps];
                }
            }
        }
    }

    s
}

/// Set the key root note (0=C … 11=B).
pub fn set_root_note(state: AppState, root: u8) -> AppState {
    let mut s = state;
    s.sequencer.root_note = root.clamp(0, 11);
    s
}

/// Set the active scale / mode.
pub fn set_scale(state: AppState, scale: Scale) -> AppState {
    let mut s = state;
    s.sequencer.scale = scale;
    s
}

/// Enable or disable automatic scale-snapping of LLM-provided bass notes.
pub fn set_scale_snap(state: AppState, enabled: bool) -> AppState {
    let mut s = state;
    s.sequencer.scale_snap = enabled;
    s
}

/// Toggle sequencer running state.
pub fn toggle_sequencer_running(state: AppState) -> AppState {
    let mut s = state;
    s.sequencer.running = !s.sequencer.running;
    s
}

/// Lock a single parameter so the LLM cannot change it.
pub fn lock_param(state: AppState, path: &str) -> AppState {
    let mut s = state;
    s.llm.locked_params.insert(path.to_string());
    s
}

/// Lock multiple parameters at once.
pub fn lock_params(state: AppState, paths: &[&str]) -> AppState {
    let mut s = state;
    for path in paths {
        s.llm.locked_params.insert(path.to_string());
    }
    s
}

/// Unlock a single parameter.
pub fn unlock_param(state: AppState, path: &str) -> AppState {
    let mut s = state;
    s.llm.locked_params.remove(path);
    s
}

/// Unlock multiple parameters at once.
pub fn unlock_params(state: AppState, paths: &[&str]) -> AppState {
    let mut s = state;
    for path in paths {
        s.llm.locked_params.remove(*path);
    }
    s
}

/// Toggle a drum step (pure function).
pub fn toggle_drum_step(state: AppState, voice: DrumVoice, step: usize) -> AppState {
    let mut s = state;
    if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice)
        && step < pattern.len()
    {
        pattern[step].active = !pattern[step].active;
        if pattern[step].active && pattern[step].velocity == 0.0 {
            pattern[step].velocity = 1.0;
        }
    }
    s
}

/// Set a 303 step note.
pub fn set_bass_step(state: AppState, step: usize, note: u8, active: bool) -> AppState {
    let mut s = state;
    if step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].active = active;
        s.sequencer.bass_pattern[step].note = note;
    }
    s
}

/// Toggle accent on a 303 step.
pub fn toggle_bass_accent(state: AppState, step: usize) -> AppState {
    let mut s = state;
    if step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].accent = !s.sequencer.bass_pattern[step].accent;
    }
    s
}

/// Toggle slide on a 303 step.
pub fn toggle_bass_slide(state: AppState, step: usize) -> AppState {
    let mut s = state;
    if step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].slide = !s.sequencer.bass_pattern[step].slide;
    }
    s
}

/// Apply the Reese bass preset.
/// Detuned dual saws + sub oscillator + highpass to cut sub mud + light chorus.
/// LLM trigger: "Reese bass", "detuned bass", "jungle bass".
pub fn apply_reese_preset(state: AppState) -> AppState {
    let mut s = state;
    s.bass.waveform = Waveform::Supersaw;
    s.bass.supersaw_voices = 2;
    s.bass.supersaw_detune = 0.3; // tight detuning — beating without flange
    s.bass.sub_osc_level = 0.5;
    s.bass.filter_mode = FilterMode::Highpass;
    s.bass.cutoff = 0.25; // HP removes low mud, keeps mid growl
    s.bass.resonance = 0.35;
    s.bass.env_mod = 0.0;
    s.bass.distortion = 0.15;
    s.bass.fm_depth = 0.0;
    s
}

/// Set a hoover sequencer step.
pub fn set_hoover_step(state: AppState, step: usize, note: u8, active: bool) -> AppState {
    let mut s = state;
    if step < s.sequencer.hoover_pattern.len() {
        s.sequencer.hoover_pattern[step].active = active;
        s.sequencer.hoover_pattern[step].note = note;
    }
    s
}

/// Apply the Hoover lead preset — sets canonical Hoover parameters.
/// LLM trigger: "add a hoover", "rave lead", "dominator".
pub fn apply_hoover_preset(state: AppState) -> AppState {
    let mut s = state;
    s.hoover.enabled = true;
    s.hoover.filter_start = 0.82;
    s.hoover.sweep_time = 0.55;
    s.hoover.resonance = 0.76;
    s.hoover.detune = 0.42;
    s.hoover.voices = 5;
    s.hoover.pitch_lfo_rate = 1.3;
    s.hoover.pitch_lfo_depth = 0.18;
    s.hoover.volume = 0.72;
    s
}

// ─── LLM state update ─────────────────────────────────────────────────────────

/// Apply an LLM-generated partial update, respecting locked params.
/// Returns the new state (caller replaces old state with this).
pub fn apply_llm_update(state: AppState, update: &serde_json::Value) -> AppState {
    let mut s = state;
    let locked = &s.llm.locked_params.clone();

    if let Some(b) = update.get("bass").and_then(|v| v.as_object()) {
        s.bass.cutoff = unlocked_f32(s.bass.cutoff, b, "cutoff", "bass.cutoff", locked);
        s.bass.resonance = unlocked_f32(s.bass.resonance, b, "resonance", "bass.resonance", locked);
        s.bass.env_mod = unlocked_f32(s.bass.env_mod, b, "env_mod", "bass.env_mod", locked);
        s.bass.decay = unlocked_f32(s.bass.decay, b, "decay", "bass.decay", locked);
        s.bass.accent_level = unlocked_f32(
            s.bass.accent_level,
            b,
            "accent_level",
            "bass.accent_level",
            locked,
        );
        s.bass.distortion = unlocked_f32(
            s.bass.distortion,
            b,
            "distortion",
            "bass.distortion",
            locked,
        );
        s.bass.volume = unlocked_f32(s.bass.volume, b, "volume", "bass.volume", locked);
        s.bass.supersaw_detune = unlocked_f32(
            s.bass.supersaw_detune,
            b,
            "supersaw_detune",
            "bass.supersaw_detune",
            locked,
        );
        if let Some(v) = b.get("supersaw_voices").and_then(|v| v.as_u64())
            && !locked.contains("bass.supersaw_voices")
        {
            s.bass.supersaw_voices = (v as u8).clamp(2, 7);
        }
        if !locked.contains("bass.sub_osc_level")
            && let Some(v) = b.get("sub_osc_level").and_then(|v| v.as_f64())
        {
            s.bass.sub_osc_level = (v as f32).clamp(0.0, 1.0);
        }
        if !locked.contains("bass.portamento_time")
            && let Some(v) = b.get("portamento_time").and_then(|v| v.as_f64())
        {
            s.bass.portamento_time = (v as f32).clamp(0.0, 1.0);
        }
        if !locked.contains("bass.noise_mix")
            && let Some(v) = b.get("noise_mix").and_then(|v| v.as_f64())
        {
            s.bass.noise_mix = (v as f32).clamp(0.0, 1.0);
        }
        if !locked.contains("bass.osc_detune")
            && let Some(v) = b.get("osc_detune").and_then(|v| v.as_f64())
        {
            s.bass.osc_detune = (v as f32).clamp(-1.0, 1.0);
        }
        s.bass.fm_ratio = unlocked_f32(s.bass.fm_ratio, b, "fm_ratio", "bass.fm_ratio", locked);
        s.bass.fm_depth = unlocked_f32(s.bass.fm_depth, b, "fm_depth", "bass.fm_depth", locked);
        if !locked.contains("bass.waveform")
            && let Some(w) = b.get("waveform").and_then(|v| v.as_str())
        {
            s.bass.waveform = match w {
                "Square" => Waveform::Square,
                "Supersaw" => Waveform::Supersaw,
                _ => Waveform::Saw,
            };
        }
        if !locked.contains("bass.filter_mode")
            && let Some(m) = b.get("filter_mode").and_then(|v| v.as_str())
        {
            s.bass.filter_mode = match m {
                "Highpass" | "HP" => FilterMode::Highpass,
                "Bandpass" | "BP" => FilterMode::Bandpass,
                _ => FilterMode::Lowpass,
            };
        }
    }

    if let Some(seq) = update.get("sequencer").and_then(|v| v.as_object()) {
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
            s = expand_sequencer_steps(s, steps as usize);
        }
        if !locked.contains("sequencer.time_sig_num")
            && let Some(v) = seq.get("time_sig_num").and_then(|v| v.as_u64())
        {
            s.sequencer.time_sig_num = (v as u8).clamp(2, 9);
        }
        if !locked.contains("sequencer.bass_steps")
            && let Some(arr) = seq.get("bass_steps").and_then(|v| v.as_array())
        {
            for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                if let Some(active) = val.as_bool() {
                    s.sequencer.bass_pattern[i].active = active;
                }
            }
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
        if !locked.contains("sequencer.bass_notes")
            && let Some(arr) = seq.get("bass_notes").and_then(|v| v.as_array())
        {
            let snap = s.sequencer.scale_snap;
            let root = s.sequencer.root_note;
            let scale = s.sequencer.scale;
            for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                if let Some(note) = val.as_u64() {
                    let note = note.clamp(0, 127) as u8;
                    s.sequencer.bass_pattern[i].note = if snap {
                        snap_to_scale(note, root, scale)
                    } else {
                        note
                    };
                }
            }
        }
        if !locked.contains("sequencer.kick_a_steps")
            && let Some(arr) = seq.get("kick_a_steps").and_then(|v| v.as_array())
            && let Some(pattern) = s.sequencer.drum_patterns.get_mut(&DrumVoice::Kick808)
        {
            for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                if let Some(active) = val.as_bool() {
                    pattern[i].active = active;
                    if active && pattern[i].velocity == 0.0 {
                        pattern[i].velocity = 1.0;
                    }
                }
            }
        }
        let drum_pattern_fields: &[(&str, DrumVoice, f32)] = &[
            ("hihat_a_steps", DrumVoice::HihatClosed808, 0.7),
            ("snare_a_steps", DrumVoice::Snare808, 1.0),
            ("kick_b_steps", DrumVoice::Kick909, 1.0),
            ("snare_b_steps", DrumVoice::Snare909, 1.0),
            ("clap_b_steps", DrumVoice::Clap909, 1.0),
            ("hihat_b_steps", DrumVoice::HihatClosed909, 0.7),
        ];
        for &(field, voice, default_vel) in drum_pattern_fields {
            let lock_key = format!("sequencer.{}", field);
            if !locked.contains(&lock_key)
                && let Some(arr) = seq.get(field).and_then(|v| v.as_array())
                && let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice)
            {
                for (i, val) in arr.iter().enumerate().take(MAX_STEPS) {
                    if let Some(active) = val.as_bool() {
                        pattern[i].active = active;
                        if active && pattern[i].velocity == 0.0 {
                            pattern[i].velocity = default_vel;
                        }
                    }
                }
            }
        }
    }

    if let Some(kit_a) = update.get("kit_a").and_then(|v| v.as_object())
        && let Some(kick) = kit_a.get("kick").and_then(|v| v.as_object())
    {
        s.kit_a.kick.pitch_env_depth = unlocked_f32(
            s.kit_a.kick.pitch_env_depth,
            kick,
            "pitch_env_depth",
            "kit_a.kick.pitch_env_depth",
            locked,
        );
        s.kit_a.kick.pitch_env_time = unlocked_f32(
            s.kit_a.kick.pitch_env_time,
            kick,
            "pitch_env_time",
            "kit_a.kick.pitch_env_time",
            locked,
        );
    }

    if let Some(kit_b) = update.get("kit_b").and_then(|v| v.as_object())
        && let Some(kick) = kit_b.get("kick").and_then(|v| v.as_object())
    {
        s.kit_b.kick.pitch_env_depth = unlocked_f32(
            s.kit_b.kick.pitch_env_depth,
            kick,
            "pitch_env_depth",
            "kit_b.kick.pitch_env_depth",
            locked,
        );
        s.kit_b.kick.pitch_env_time = unlocked_f32(
            s.kit_b.kick.pitch_env_time,
            kick,
            "pitch_env_time",
            "kit_b.kick.pitch_env_time",
            locked,
        );
    }

    if let Some(fx) = update.get("fx").and_then(|v| v.as_object()) {
        s.fx.reverb_size = unlocked_f32(
            s.fx.reverb_size,
            fx,
            "reverb_size",
            "fx.reverb_size",
            locked,
        );
        s.fx.reverb_mix = unlocked_f32(s.fx.reverb_mix, fx, "reverb_mix", "fx.reverb_mix", locked);
        s.fx.delay_time = unlocked_f32(s.fx.delay_time, fx, "delay_time", "fx.delay_time", locked);
        s.fx.delay_feedback = unlocked_f32(
            s.fx.delay_feedback,
            fx,
            "delay_feedback",
            "fx.delay_feedback",
            locked,
        );
        s.fx.delay_mix = unlocked_f32(s.fx.delay_mix, fx, "delay_mix", "fx.delay_mix", locked);
        s.fx.distortion_drive = unlocked_f32(
            s.fx.distortion_drive,
            fx,
            "distortion_drive",
            "fx.distortion_drive",
            locked,
        );
        s.fx.distortion_mix = unlocked_f32(
            s.fx.distortion_mix,
            fx,
            "distortion_mix",
            "fx.distortion_mix",
            locked,
        );
        s.fx.bitcrush_bits = unlocked_f32(
            s.fx.bitcrush_bits,
            fx,
            "bitcrush_bits",
            "fx.bitcrush_bits",
            locked,
        );
        s.fx.bitcrush_rate = unlocked_f32(
            s.fx.bitcrush_rate,
            fx,
            "bitcrush_rate",
            "fx.bitcrush_rate",
            locked,
        );
        s.fx.bitcrush_mix = unlocked_f32(
            s.fx.bitcrush_mix,
            fx,
            "bitcrush_mix",
            "fx.bitcrush_mix",
            locked,
        );
        s.fx.chorus_rate = unlocked_f32(
            s.fx.chorus_rate,
            fx,
            "chorus_rate",
            "fx.chorus_rate",
            locked,
        );
        s.fx.chorus_depth = unlocked_f32(
            s.fx.chorus_depth,
            fx,
            "chorus_depth",
            "fx.chorus_depth",
            locked,
        );
        s.fx.chorus_mix = unlocked_f32(s.fx.chorus_mix, fx, "chorus_mix", "fx.chorus_mix", locked);
        s.fx.phaser_rate = unlocked_f32(
            s.fx.phaser_rate,
            fx,
            "phaser_rate",
            "fx.phaser_rate",
            locked,
        );
        s.fx.phaser_depth = unlocked_f32(
            s.fx.phaser_depth,
            fx,
            "phaser_depth",
            "fx.phaser_depth",
            locked,
        );
        s.fx.phaser_mix = unlocked_f32(s.fx.phaser_mix, fx, "phaser_mix", "fx.phaser_mix", locked);
        s.fx.waveshaper_drive = unlocked_f32(
            s.fx.waveshaper_drive,
            fx,
            "waveshaper_drive",
            "fx.waveshaper_drive",
            locked,
        );
        s.fx.waveshaper_mix = unlocked_f32(
            s.fx.waveshaper_mix,
            fx,
            "waveshaper_mix",
            "fx.waveshaper_mix",
            locked,
        );
        s.fx.ring_mod_freq = unlocked_f32(
            s.fx.ring_mod_freq,
            fx,
            "ring_mod_freq",
            "fx.ring_mod_freq",
            locked,
        );
        s.fx.ring_mod_mix = unlocked_f32(
            s.fx.ring_mod_mix,
            fx,
            "ring_mod_mix",
            "fx.ring_mod_mix",
            locked,
        );
        s.fx.eq_low_gain = unlocked_f32(
            s.fx.eq_low_gain,
            fx,
            "eq_low_gain",
            "fx.eq_low_gain",
            locked,
        );
        s.fx.eq_mid_gain = unlocked_f32(
            s.fx.eq_mid_gain,
            fx,
            "eq_mid_gain",
            "fx.eq_mid_gain",
            locked,
        );
        s.fx.eq_hi_gain = unlocked_f32(s.fx.eq_hi_gain, fx, "eq_hi_gain", "fx.eq_hi_gain", locked);
        s.fx.compressor_threshold = unlocked_f32(
            s.fx.compressor_threshold,
            fx,
            "compressor_threshold",
            "fx.compressor_threshold",
            locked,
        );
        s.fx.compressor_ratio = unlocked_f32(
            s.fx.compressor_ratio,
            fx,
            "compressor_ratio",
            "fx.compressor_ratio",
            locked,
        );
        s.fx.compressor_mix = unlocked_f32(
            s.fx.compressor_mix,
            fx,
            "compressor_mix",
            "fx.compressor_mix",
            locked,
        );
        s.fx.tape_drive = unlocked_f32(s.fx.tape_drive, fx, "tape_drive", "fx.tape_drive", locked);
        s.fx.tape_mix = unlocked_f32(s.fx.tape_mix, fx, "tape_mix", "fx.tape_mix", locked);
        s.fx.tape_flutter = unlocked_f32(
            s.fx.tape_flutter,
            fx,
            "tape_flutter",
            "fx.tape_flutter",
            locked,
        );
    }

    if let Some(lfo_arr) = update.get("lfo").and_then(|v| v.as_array()) {
        for (i, slot_val) in lfo_arr.iter().enumerate().take(4) {
            let path_prefix = format!("lfo[{}]", i);
            if locked.contains(&path_prefix) {
                continue;
            }
            if let Some(obj) = slot_val.as_object() {
                if let Some(v) = obj.get("enabled").and_then(|v| v.as_bool()) {
                    s.lfo[i].enabled = v;
                }
                if let Some(v) = obj.get("rate").and_then(|v| v.as_f64()) {
                    s.lfo[i].rate = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = obj.get("depth").and_then(|v| v.as_f64()) {
                    s.lfo[i].depth = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = obj.get("phase_offset").and_then(|v| v.as_f64()) {
                    s.lfo[i].phase_offset = (v as f32).clamp(0.0, 1.0);
                }
                if let Some(v) = obj.get("waveform").and_then(|v| v.as_str()) {
                    s.lfo[i].waveform = match v {
                        "Triangle" => LfoWaveform::Triangle,
                        "Saw" => LfoWaveform::Saw,
                        "InvSaw" => LfoWaveform::InvSaw,
                        "Square" => LfoWaveform::Square,
                        "SampleAndHold" | "S&H" => LfoWaveform::SampleAndHold,
                        _ => LfoWaveform::Sine,
                    };
                }
                if let Some(v) = obj.get("target").and_then(|v| v.as_str()) {
                    s.lfo[i].target = match v {
                        "BassCutoff" => LfoTarget::BassCutoff,
                        "BassResonance" => LfoTarget::BassResonance,
                        "BassPitch" => LfoTarget::BassPitch,
                        "BassVolume" => LfoTarget::BassVolume,
                        "ReverbMix" => LfoTarget::ReverbMix,
                        "DelayTime" => LfoTarget::DelayTime,
                        "DelayFeedback" => LfoTarget::DelayFeedback,
                        "ChorusMix" => LfoTarget::ChorusMix,
                        "ChorusRate" => LfoTarget::ChorusRate,
                        "Kick808Pitch" => LfoTarget::Kick808Pitch,
                        _ => LfoTarget::None,
                    };
                }
            }
        }
    }

    if let Some(n) = update.get("noise").and_then(|v| v.as_object()) {
        if !locked.contains("noise.enabled")
            && let Some(v) = n.get("enabled").and_then(|v| v.as_bool())
        {
            s.noise_voice.enabled = v;
        }
        s.noise_voice.volume =
            unlocked_f32(s.noise_voice.volume, n, "volume", "noise.volume", locked);
        s.noise_voice.color = unlocked_f32(s.noise_voice.color, n, "color", "noise.color", locked);
        s.noise_voice.cutoff =
            unlocked_f32(s.noise_voice.cutoff, n, "cutoff", "noise.cutoff", locked);
    }

    if let Some(h) = update.get("hoover").and_then(|v| v.as_object()) {
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

    s
}

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
