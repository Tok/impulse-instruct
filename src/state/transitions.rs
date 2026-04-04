// ─── state/transitions.rs ────────────────────────────────────────────────────
// Pure state transition helpers extracted to keep state/mod.rs under 1000 lines.

use std::collections::HashSet;

use super::{
    AppState, DrumVoice, FilterMode, LfoTarget, LfoWaveform, MAX_STEPS, Scale, Waveform,
    snap_to_scale,
};
use crate::sequencer::euclidean_rhythm;

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
        // Tile AN1X pattern
        for i in old_steps..new_steps {
            s.sequencer.an1x_pattern[i] = s.sequencer.an1x_pattern[i % old_steps];
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

/// Set velocity for a single drum step.
pub fn set_drum_step_velocity(
    state: AppState,
    voice: DrumVoice,
    step: usize,
    vel: f32,
) -> AppState {
    let mut s = state;
    if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice)
        && step < pattern.len()
    {
        pattern[step].velocity = vel.clamp(0.05, 1.0);
    }
    s
}

pub fn set_drum_step_probability(
    state: AppState,
    voice: DrumVoice,
    step: usize,
    prob: f32,
) -> AppState {
    let mut s = state;
    if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice)
        && step < pattern.len()
    {
        pattern[step].probability = prob.clamp(0.0, 1.0);
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

/// Set an AN1X sequencer step.
pub fn set_an1x_step(state: AppState, step: usize, note: u8, active: bool) -> AppState {
    let mut s = state;
    if step < s.sequencer.an1x_pattern.len() {
        s.sequencer.an1x_pattern[step].active = active;
        s.sequencer.an1x_pattern[step].note = note;
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

/// Apply the BoC-style AN1X preset — warm detuned pad with slow attack and LFO drift.
/// LLM trigger: "add a pad", "warm lead", "BoC", "ambient".
pub fn apply_boc_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Saw;
    s.an1x.osc2_wave = crate::state::An1xWave::Saw;
    s.an1x.osc1_level = 0.8;
    s.an1x.osc2_level = 0.65;
    s.an1x.osc2_detune = 0.53; // ~+2.9 semitones — classic detuned beating
    s.an1x.osc2_octave = 0;
    s.an1x.sub_level = 0.15;
    s.an1x.filter_cutoff = 0.42;
    s.an1x.filter_resonance = 0.28;
    s.an1x.filter_env_amount = 0.58; // gentle positive mod
    s.an1x.filter_attack = 0.15;
    s.an1x.filter_decay = 0.5;
    s.an1x.filter_sustain = 0.35;
    s.an1x.filter_release = 0.4;
    s.an1x.amp_attack = 0.32; // slow pad attack
    s.an1x.amp_decay = 0.55;
    s.an1x.amp_sustain = 0.65;
    s.an1x.amp_release = 0.5;
    s.an1x.lfo_rate = 0.09; // ~0.12 Hz — barely perceptible breathing
    s.an1x.lfo_depth = 0.12;
    s.an1x.lfo_target = crate::state::An1xLfoTarget::Pitch;
    s.an1x.lfo_delay = 0.4; // LFO fades in over ~1.6s after note is struck
    s.an1x.pitch_env_attack = 0.0;
    s.an1x.pitch_env_decay = 0.1;
    s.an1x.pitch_env_amount = 0.5; // neutral — no pitch transient on pads
    s.an1x.hard_sync = false;
    s.an1x.lfo_bpm_sync = false;
    s.an1x.lfo_sync_beats = 4.0;
    s.an1x.drift = 0.14;
    s.an1x.glide_time = 0.18;
    s.an1x.glide_legato = true;
    s.an1x.volume = 0.75;
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
        apply_fx_update(&mut s, fx, locked);
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

    if let Some(eg) = update.get("free_eg").and_then(|v| v.as_object())
        && !locked.contains("free_eg")
    {
        if let Some(v) = eg.get("enabled").and_then(|v| v.as_bool()) {
            s.free_eg.enabled = v;
        }
        if let Some(v) = eg.get("period").and_then(|v| v.as_f64()) {
            s.free_eg.period = (v as f32).clamp(0.0, 1.0);
        }
        if let Some(v) = eg.get("depth").and_then(|v| v.as_f64()) {
            s.free_eg.depth = (v as f32).clamp(0.0, 1.0);
        }
        if let Some(v) = eg.get("loop_mode").and_then(|v| v.as_bool()) {
            s.free_eg.loop_mode = v;
        }
        if let Some(v) = eg.get("target").and_then(|v| v.as_str()) {
            s.free_eg.target = match v {
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
        if let Some(arr) = eg.get("values").and_then(|v| v.as_array()) {
            for (i, val) in arr.iter().enumerate().take(8) {
                if let Some(v) = val.as_f64() {
                    s.free_eg.values[i] = (v as f32).clamp(0.0, 1.0);
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

    if let Some(a) = update.get("an1x").and_then(|v| v.as_object()) {
        apply_an1x_update(&mut s, a, locked);
    }

    // ── Euclidean rhythm ──────────────────────────────────────────────────────
    // JSON: { "euclidean": { "voice": "kick_a", "pulses": 5, "steps": 16 } }
    if let Some(e) = update.get("euclidean").and_then(|v| v.as_object()) {
        let voice_str = e.get("voice").and_then(|v| v.as_str()).unwrap_or("");
        let pulses = e.get("pulses").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
        let n_steps = e
            .get("steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(s.sequencer.steps as u64) as usize;
        let drum_voice = match voice_str {
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
        };
        if let Some(voice) = drum_voice {
            let lock_path = format!("sequencer.{}_steps", voice_str);
            if !locked.contains(&lock_path) {
                let pattern = euclidean_rhythm(pulses, n_steps);
                if let Some(row) = s.sequencer.drum_patterns.get_mut(&voice) {
                    for (i, &active) in pattern.iter().enumerate().take(row.len()) {
                        row[i].active = active;
                    }
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

/// Apply AN1X voice fields from an LLM JSON update object.
/// Extracted to keep `apply_llm_update` under the 1000-line limit.
fn apply_an1x_update(
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
/// Extracted to keep `apply_llm_update` under the 1000-line limit.
fn apply_fx_update(
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
    u!(s.fx.delay_time, "delay_time", "fx.delay_time");
    u!(s.fx.delay_feedback, "delay_feedback", "fx.delay_feedback");
    u!(s.fx.delay_mix, "delay_mix", "fx.delay_mix");
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
    u!(s.fx.tape_drive, "tape_drive", "fx.tape_drive");
    u!(s.fx.tape_mix, "tape_mix", "fx.tape_mix");
    u!(s.fx.tape_flutter, "tape_flutter", "fx.tape_flutter");
    u!(s.fx.master_volume, "master_volume", "fx.master_volume");
}
