// ─── state/transitions.rs ────────────────────────────────────────────────────
// Pure state transition helpers extracted to keep state/mod.rs under 1000 lines.

use super::llm_helpers::{
    apply_an1x_update, apply_bass_update, apply_fx_update, apply_hoover_update, unlocked_f32,
};
use super::rack::{ModuleKind, PortDir, PortKind, PortRef};
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

pub fn set_drum_step_ratchet(
    state: AppState,
    voice: DrumVoice,
    step: usize,
    ratchet: u8,
) -> AppState {
    let mut s = state;
    if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice)
        && step < pattern.len()
    {
        pattern[step].ratchet = ratchet.clamp(1, 4);
    }
    s
}

/// Set the step length for an individual drum voice (polyrhythm).
pub fn set_drum_voice_steps(state: AppState, voice: DrumVoice, n: usize) -> AppState {
    let mut s = state;
    s.sequencer
        .drum_steps
        .insert(voice, n.clamp(1, crate::state::MAX_STEPS));
    s
}

/// Set the step length for bass, hoover, or an1x lanes.
pub fn set_lane_steps(state: AppState, lane: &str, n: usize) -> AppState {
    let mut s = state;
    let n = n.clamp(1, crate::state::MAX_STEPS);
    match lane {
        "bass" => s.sequencer.bass_steps = n,
        "hoover" => s.sequencer.hoover_steps = n,
        "an1x" => s.sequencer.an1x_steps = n,
        _ => {}
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
        apply_bass_update(&mut s, b, locked);
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
        // Polyrhythm: per-voice step-count overrides.
        if !locked.contains("sequencer.drum_lengths")
            && let Some(obj) = seq.get("drum_lengths").and_then(|v| v.as_object())
        {
            use DrumVoice::*;
            for (key, voice) in &[
                ("kick_a", Kick808),
                ("snare_a", Snare808),
                ("hihat_a", HihatClosed808),
                ("hihat_a_open", HihatOpen808),
                ("kick_b", Kick909),
                ("snare_b", Snare909),
                ("hihat_b", HihatClosed909),
                ("hihat_b_open", HihatOpen909),
                ("clap_b", Clap909),
            ] {
                if let Some(n) = obj.get(*key).and_then(|v| v.as_u64()) {
                    s = set_drum_voice_steps(s, *voice, n as usize);
                }
            }
        }
        // Ratchet: per-voice per-step sub-hit counts.
        if !locked.contains("sequencer.drum_ratchets")
            && let Some(obj) = seq.get("drum_ratchets").and_then(|v| v.as_object())
        {
            use DrumVoice::*;
            for (key, voice) in &[
                ("kick_a", Kick808),
                ("snare_a", Snare808),
                ("hihat_a", HihatClosed808),
                ("hihat_a_open", HihatOpen808),
                ("kick_b", Kick909),
                ("snare_b", Snare909),
                ("hihat_b", HihatClosed909),
                ("hihat_b_open", HihatOpen909),
                ("clap_b", Clap909),
            ] {
                if let Some(arr) = obj.get(*key).and_then(|v| v.as_array()) {
                    for (step, val) in arr.iter().enumerate().take(MAX_STEPS) {
                        if let Some(r) = val.as_u64() {
                            s = set_drum_step_ratchet(s, *voice, step, r as u8);
                        }
                    }
                }
            }
        }
        if !locked.contains("sequencer.bass_len")
            && let Some(n) = seq.get("bass_len").and_then(|v| v.as_u64())
        {
            s = set_lane_steps(s, "bass", n as usize);
        }
        if !locked.contains("sequencer.hoover_len")
            && let Some(n) = seq.get("hoover_len").and_then(|v| v.as_u64())
        {
            s = set_lane_steps(s, "hoover", n as usize);
        }
        if !locked.contains("sequencer.an1x_len")
            && let Some(n) = seq.get("an1x_len").and_then(|v| v.as_u64())
        {
            s = set_lane_steps(s, "an1x", n as usize);
        }
        if !locked.contains("sequencer.bass_steps")
            && let Some(arr) = seq.get("bass_steps").and_then(|v| v.as_array())
        {
            apply_llm_step_array(arr, &mut s.sequencer.bass_pattern, MAX_STEPS, |step, a| {
                step.active = a;
            });
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
        let drum_step_fields: &[(&str, DrumVoice, f32)] = &[
            ("kick_a_steps", DrumVoice::Kick808, 1.0),
            ("hihat_a_steps", DrumVoice::HihatClosed808, 0.7),
            ("snare_a_steps", DrumVoice::Snare808, 1.0),
            ("kick_b_steps", DrumVoice::Kick909, 1.0),
            ("snare_b_steps", DrumVoice::Snare909, 1.0),
            ("clap_b_steps", DrumVoice::Clap909, 1.0),
            ("hihat_b_steps", DrumVoice::HihatClosed909, 0.7),
        ];
        for &(field, voice, default_vel) in drum_step_fields {
            let lock_key = format!("sequencer.{}", field);
            if !locked.contains(&lock_key)
                && let Some(arr) = seq.get(field).and_then(|v| v.as_array())
            {
                // Collect indices first to avoid split-borrow issues with drum_patterns
                let arr: Vec<_> = arr.clone();
                if let Some(pattern) = s.sequencer.drum_patterns.get_mut(&voice) {
                    apply_llm_step_array(&arr, pattern, MAX_STEPS, |step, active| {
                        step.active = active;
                        if active && step.velocity == 0.0 {
                            step.velocity = default_vel;
                        }
                    });
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
        apply_hoover_update(&mut s, h, locked);
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

    // ── Rack routing (enable/disable modules, add/remove cables) ─────────────
    // JSON: { "rack": { "enable": ["bitcrush"], "disable": ["reverb"],
    //                   "connect": [{"from": "bitcrush", "to": "master"}],
    //                   "disconnect": [{"from": "bitcrush", "to": "master"}] } }
    if let Some(rack_upd) = update.get("rack").and_then(|v| v.as_object()) {
        if let Some(arr) = rack_upd.get("enable").and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(name) = v.as_str() {
                    for m in &mut s.rack.modules {
                        if rack_kind_name_matches(m.kind, name) {
                            m.enabled = true;
                        }
                    }
                }
            }
        }
        if let Some(arr) = rack_upd.get("disable").and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(name) = v.as_str() {
                    for m in &mut s.rack.modules {
                        if rack_kind_name_matches(m.kind, name) {
                            m.enabled = false;
                        }
                    }
                }
            }
        }
        if let Some(arr) = rack_upd.get("connect").and_then(|v| v.as_array()) {
            for v in arr {
                let from_name = v.get("from").and_then(|v| v.as_str());
                let to_name = v.get("to").and_then(|v| v.as_str());
                if let (Some(fn_), Some(tn)) = (from_name, to_name) {
                    let from_id = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, fn_))
                        .map(|m| m.id);
                    let to_id = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, tn))
                        .map(|m| m.id);
                    if let (Some(fid), Some(tid)) = (from_id, to_id) {
                        // Don't duplicate an existing cable
                        let exists = s
                            .rack
                            .cables
                            .iter()
                            .any(|c| c.from.module_id == fid && c.to.module_id == tid);
                        if !exists {
                            s.rack.connect(
                                PortRef {
                                    module_id: fid,
                                    dir: PortDir::Out,
                                    kind: PortKind::Audio,
                                    index: 0,
                                },
                                PortRef {
                                    module_id: tid,
                                    dir: PortDir::In,
                                    kind: PortKind::Audio,
                                    index: 0,
                                },
                            );
                        }
                    }
                }
            }
        }
        if let Some(arr) = rack_upd.get("disconnect").and_then(|v| v.as_array()) {
            for v in arr {
                let from_name = v.get("from").and_then(|v| v.as_str());
                let to_name = v.get("to").and_then(|v| v.as_str());
                if let (Some(fn_), Some(tn)) = (from_name, to_name) {
                    let from_id = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, fn_))
                        .map(|m| m.id);
                    let to_id = s
                        .rack
                        .modules
                        .iter()
                        .find(|m| rack_kind_name_matches(m.kind, tn))
                        .map(|m| m.id);
                    if let (Some(fid), Some(tid)) = (from_id, to_id) {
                        let from_ref = PortRef {
                            module_id: fid,
                            dir: PortDir::Out,
                            kind: PortKind::Audio,
                            index: 0,
                        };
                        let to_ref = PortRef {
                            module_id: tid,
                            dir: PortDir::In,
                            kind: PortKind::Audio,
                            index: 0,
                        };
                        s.rack.disconnect(&from_ref, &to_ref);
                    }
                }
            }
        }
    }

    s
}

/// Match a module kind against a flexible name string from the LLM.
fn rack_kind_name_matches(kind: ModuleKind, name: &str) -> bool {
    let n = name.to_lowercase();
    match kind {
        ModuleKind::FxBitcrush => matches!(
            n.as_str(),
            "bitcrush" | "bit_crush" | "bit crush" | "lofi" | "lo-fi"
        ),
        ModuleKind::FxReverb => matches!(n.as_str(), "reverb" | "verb"),
        ModuleKind::FxDelay => matches!(n.as_str(), "delay" | "echo"),
        ModuleKind::FxChorus => matches!(n.as_str(), "chorus" | "ensemble"),
        ModuleKind::FxPhaser => matches!(n.as_str(), "phaser" | "phase"),
        ModuleKind::FxRingMod => matches!(n.as_str(), "ringmod" | "ring_mod" | "ring mod" | "ring"),
        ModuleKind::FxWaveshaper => matches!(n.as_str(), "waveshaper" | "wave_shaper" | "shaper"),
        ModuleKind::FxEq => matches!(n.as_str(), "eq" | "equalizer" | "equaliser"),
        ModuleKind::FxCompressor => matches!(n.as_str(), "compressor" | "comp"),
        ModuleKind::FxTapeSat => matches!(
            n.as_str(),
            "tapesat" | "tape_sat" | "tape sat" | "tape" | "saturation"
        ),
        ModuleKind::FxDrive => matches!(n.as_str(), "drive" | "overdrive" | "distortion"),
        ModuleKind::LfoModule => matches!(n.as_str(), "lfo"),
        ModuleKind::AcidBass => matches!(n.as_str(), "bass" | "acid" | "303"),
        ModuleKind::DrumKit808 => matches!(n.as_str(), "808" | "kit_a" | "drum_a" | "drums_a"),
        ModuleKind::DrumKit909 => matches!(n.as_str(), "909" | "kit_b" | "drum_b" | "drums_b"),
        ModuleKind::HooverLead => matches!(n.as_str(), "hoover" | "lead"),
        ModuleKind::An1xVoice => matches!(n.as_str(), "an1x" | "an-1x" | "pad" | "synth"),
        ModuleKind::AmenSampler => matches!(n.as_str(), "amen" | "sampler" | "break"),
        ModuleKind::NoiseVoice => matches!(n.as_str(), "noise"),
        ModuleKind::MasterOutput => {
            matches!(n.as_str(), "master" | "master_out" | "out" | "output")
        }
        ModuleKind::StepSequencer => matches!(n.as_str(), "sequencer" | "seq"),
    }
}

// ─── Step-array parser ────────────────────────────────────────────────────────

/// Apply a JSON step array to a mutable pattern slice.
///
/// Accepts three compact formats from the LLM:
///   `[]`           — clear: set every step to `false`
///   `[0, 4, 8]`    — index list (< 16 elements): clear all, then activate listed indices
///   `[1,0,1,0,…]`  — inline 0/1 or true/false (≥ 16 elements): write element-by-element
///
/// `set_active` is called once per step and must update `active` plus any default fields
/// (e.g. initialise `velocity` on first activation).
pub fn apply_llm_step_array<T, F>(
    arr: &[serde_json::Value],
    items: &mut [T],
    max_write: usize,
    mut set_active: F,
) where
    F: FnMut(&mut T, bool),
{
    let n = items.len().min(max_write);
    if arr.is_empty() {
        // [] = clear all
        for item in items[..n].iter_mut() {
            set_active(item, false);
        }
        return;
    }
    if arr.len() < 16 {
        // Index list: clear everything, then activate listed positions
        for item in items[..n].iter_mut() {
            set_active(item, false);
        }
        for val in arr {
            if let Some(idx) = val.as_u64().map(|i| i as usize)
                && idx < n
            {
                set_active(&mut items[idx], true);
            }
        }
        return;
    }
    // Inline: element-by-element (0/1 integers accepted alongside true/false)
    for (i, val) in arr.iter().enumerate().take(n) {
        let active = match val {
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::Number(num) => num.as_u64().unwrap_or(0) != 0,
            _ => continue,
        };
        set_active(&mut items[i], active);
    }
}

// ─── Pattern bank & chain ─────────────────────────────────────────────────────

/// Save the live `sequencer` state into `pattern_bank[slot]` (0–7).
pub fn bank_write(state: AppState, slot: usize) -> AppState {
    let mut s = state;
    let slot = slot.min(7);
    if s.pattern_bank.len() < 8 {
        s.pattern_bank
            .resize_with(8, super::SequencerState::default);
    }
    s.pattern_bank[slot] = s.sequencer.clone();
    s
}

/// Load `pattern_bank[slot]` into the live `sequencer`.
/// `keep_transport` = true preserves BPM/swing/running (used during live performance).
pub fn bank_load(state: AppState, slot: usize, keep_transport: bool) -> AppState {
    let mut s = state;
    let slot = slot.min(7);
    if let Some(pattern) = s.pattern_bank.get(slot).cloned() {
        let bpm = s.sequencer.bpm;
        let swing = s.sequencer.swing;
        let running = s.sequencer.running;
        let step = s.sequencer.current_step;
        s.sequencer = pattern;
        if keep_transport {
            s.sequencer.bpm = bpm;
            s.sequencer.swing = swing;
            s.sequencer.running = running;
            s.sequencer.current_step = step;
        }
    }
    s
}

/// Set which bank slot the UI is editing (0–7). Does not affect playback.
pub fn set_pattern_edit(state: AppState, slot: usize) -> AppState {
    let mut s = state;
    s.pattern_edit = slot.min(7);
    s
}

/// Replace the entire chain sequence (entries clamped to 0–7, max 8 slots).
pub fn set_chain(state: AppState, chain: Vec<usize>) -> AppState {
    let mut s = state;
    s.chain = chain.into_iter().map(|v| v.min(7)).take(8).collect();
    s
}

/// Append one slot index to the chain (noop if already 8 entries).
pub fn chain_push(state: AppState, slot: usize) -> AppState {
    let mut s = state;
    if s.chain.len() < 8 {
        s.chain.push(slot.min(7));
    }
    s
}

/// Remove the last entry from the chain.
pub fn chain_pop(state: AppState) -> AppState {
    let mut s = state;
    s.chain.pop();
    s
}

/// Enable or disable chain playback mode.
pub fn set_chain_enabled(state: AppState, enabled: bool) -> AppState {
    let mut s = state;
    s.chain_enabled = enabled;
    if !enabled {
        s.chain_pos = 0;
    }
    s
}

/// Save current sequencer to `pattern_edit` slot, then load `new_slot` (keep transport).
/// This is the primary bank-switch operation: always saves before loading so no edits are lost.
pub fn bank_swap(state: AppState, new_slot: usize) -> AppState {
    let mut s = state;
    let new_slot = new_slot.min(7);
    if s.pattern_edit == new_slot {
        return s; // already on this slot — save in place
    }
    if s.pattern_bank.len() < 8 {
        s.pattern_bank
            .resize_with(8, super::SequencerState::default);
    }
    // Save current edits to the previously active slot
    s.pattern_bank[s.pattern_edit] = s.sequencer.clone();
    // Load new slot, preserving transport
    let bpm = s.sequencer.bpm;
    let swing = s.sequencer.swing;
    let running = s.sequencer.running;
    let step = s.sequencer.current_step;
    s.sequencer = s.pattern_bank.get(new_slot).cloned().unwrap_or_default();
    s.sequencer.bpm = bpm;
    s.sequencer.swing = swing;
    s.sequencer.running = running;
    s.sequencer.current_step = step;
    s.pattern_edit = new_slot;
    s
}

/// Toggle live-record mode. Disables itself automatically when sequencer stops.
pub fn toggle_live_record(state: AppState) -> AppState {
    let mut s = state;
    s.live_record = !s.live_record;
    s
}

/// Write a note into the bass pattern at the given step (used by live record).
pub fn record_bass_note(state: AppState, step: usize, note: u8) -> AppState {
    let mut s = state;
    let step = step % s.sequencer.bass_steps.max(1);
    if step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].active = true;
        s.sequencer.bass_pattern[step].note = note;
    }
    s
}
