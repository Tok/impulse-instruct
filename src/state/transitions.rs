// ─── state/transitions.rs ────────────────────────────────────────────────────
// Pure state transition helpers extracted to keep state/mod.rs under 1000 lines.

pub use super::llm_apply::{apply_llm_step_array, apply_llm_update};

use super::{
    AgentRole, AppState, ConversationMode, DrumVoice, FilterMode, LlmAgentState, MAX_STEPS,
    ModuleKind, Scale, Waveform, rack_kind_name_matches,
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
        // Tile bass pattern (legacy single pattern + per-voice patterns)
        for i in old_steps..new_steps {
            s.sequencer.bass_pattern[i] = s.sequencer.bass_pattern[i % old_steps];
        }
        for vi in 0..crate::state::MAX_BASS_VOICES {
            if let Some(pat) = s.sequencer.bass_patterns.get_mut(vi) {
                for i in old_steps..new_steps {
                    pat[i] = pat[i % old_steps];
                }
            }
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
        "bass" => {
            s.sequencer.bass_steps = n;
            // Keep voice 0 step count in sync with the legacy field
            if let Some(vs) = s.sequencer.bass_voice_steps.get_mut(0) {
                *vs = n;
            }
        }
        "hoover" => s.sequencer.hoover_steps = n,
        "an1x" => s.sequencer.an1x_steps = n,
        _ => {}
    }
    s
}

/// Set a 303 step note on a specific voice's pattern.  Voice 0 mirrors
/// `bass_pattern` (the legacy field) so existing callers stay in sync.
pub fn set_bass_step_voice(
    state: AppState,
    voice_idx: usize,
    step: usize,
    note: u8,
    active: bool,
) -> AppState {
    let mut s = state;
    let vi = voice_idx.min(crate::state::MAX_BASS_VOICES - 1);
    if let Some(pat) = s.sequencer.bass_patterns.get_mut(vi)
        && step < pat.len()
    {
        pat[step].active = active;
        pat[step].note = note;
    }
    if vi == 0 && step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].active = active;
        s.sequencer.bass_pattern[step].note = note;
    }
    s
}

/// Set a 303 step note on the active voice's pattern.
pub fn set_bass_step(state: AppState, step: usize, note: u8, active: bool) -> AppState {
    let vi = state.active_voice;
    set_bass_step_voice(state, vi, step, note, active)
}

/// Toggle accent on a 303 step on a specific voice.  Intensity flips
/// between 0.0 (off) and 1.0 (full accent); non-zero values round to
/// "on" semantics so a partially-set accent toggles cleanly back to 0.
pub fn toggle_bass_accent_voice(state: AppState, voice_idx: usize, step: usize) -> AppState {
    let mut s = state;
    let vi = voice_idx.min(crate::state::MAX_BASS_VOICES - 1);
    let new_val = if let Some(pat) = s.sequencer.bass_patterns.get(vi)
        && step < pat.len()
    {
        if pat[step].accent > 0.0 { 0.0 } else { 1.0 }
    } else {
        1.0
    };
    if let Some(pat) = s.sequencer.bass_patterns.get_mut(vi)
        && step < pat.len()
    {
        pat[step].accent = new_val;
    }
    if vi == 0 && step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].accent = new_val;
    }
    s
}

/// Toggle accent on a 303 step (active voice).
pub fn toggle_bass_accent(state: AppState, step: usize) -> AppState {
    let vi = state.active_voice;
    toggle_bass_accent_voice(state, vi, step)
}

/// Toggle slide on a 303 step on a specific voice.
pub fn toggle_bass_slide_voice(state: AppState, voice_idx: usize, step: usize) -> AppState {
    let mut s = state;
    let vi = voice_idx.min(crate::state::MAX_BASS_VOICES - 1);
    let new_val = if let Some(pat) = s.sequencer.bass_patterns.get(vi)
        && step < pat.len()
    {
        if pat[step].slide > 0.0 { 0.0 } else { 1.0 }
    } else {
        1.0
    };
    if let Some(pat) = s.sequencer.bass_patterns.get_mut(vi)
        && step < pat.len()
    {
        pat[step].slide = new_val;
    }
    if vi == 0 && step < s.sequencer.bass_pattern.len() {
        s.sequencer.bass_pattern[step].slide = new_val;
    }
    s
}

/// Toggle slide on a 303 step (active voice).
pub fn toggle_bass_slide(state: AppState, step: usize) -> AppState {
    let vi = state.active_voice;
    toggle_bass_slide_voice(state, vi, step)
}

/// Apply the Reese bass preset.
/// Detuned dual saws + sub oscillator + highpass to cut sub mud + light chorus.
/// LLM trigger: "Reese bass", "detuned bass", "jungle bass".
pub fn apply_reese_preset(state: AppState) -> AppState {
    let mut s = state;
    s.bass_voices[0].synth.waveform = Waveform::Supersaw;
    s.bass_voices[0].synth.supersaw_voices = 2;
    s.bass_voices[0].synth.supersaw_detune = 0.3; // tight detuning — beating without flange
    s.bass_voices[0].synth.sub_osc_level = 0.5;
    s.bass_voices[0].synth.filter_mode = FilterMode::Highpass;
    s.bass_voices[0].synth.cutoff = 0.25; // HP removes low mud, keeps mid growl
    s.bass_voices[0].synth.resonance = 0.35;
    s.bass_voices[0].synth.env_mod = 0.0;
    s.bass_voices[0].synth.distortion = 0.15;
    s.bass_voices[0].synth.fm_depth = 0.0;
    s
}

/// Gabber kick preset: extreme pitch envelope + hard clip for distorted swooping kick.
pub fn apply_gabber_kick_preset(state: AppState) -> AppState {
    let mut s = state;
    s.kit_a.kick.pitch = 0.35; // lower base pitch
    s.kit_a.kick.decay = 0.7; // long tail
    s.kit_a.kick.punch = 0.9; // maximum transient
    s.kit_a.kick.pitch_env_depth = 0.9; // extreme pitch sweep
    s.kit_a.kick.pitch_env_time = 0.6; // long sweep time — the gabber "swooop"
    s.kit_a.kick.clip = 0.8; // heavy hard clipping
    s.kit_a.kick.volume = 0.85;
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

/// Warm Pad preset — lush, slow-moving, classic analog pad sound.
pub fn apply_warm_pad_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Saw;
    s.an1x.osc2_wave = crate::state::An1xWave::Saw;
    s.an1x.osc1_level = 0.75;
    s.an1x.osc2_level = 0.7;
    s.an1x.osc2_detune = 0.52;
    s.an1x.sub_level = 0.3;
    s.an1x.filter_cutoff = 0.35;
    s.an1x.filter_resonance = 0.15;
    s.an1x.filter_env_amount = 0.55;
    s.an1x.filter_attack = 0.4;
    s.an1x.filter_decay = 0.6;
    s.an1x.filter_sustain = 0.4;
    s.an1x.filter_release = 0.55;
    s.an1x.amp_attack = 0.45;
    s.an1x.amp_decay = 0.5;
    s.an1x.amp_sustain = 0.7;
    s.an1x.amp_release = 0.6;
    s.an1x.drift = 0.08;
    s.an1x.volume = 0.7;
    s
}

/// Evolving Texture preset — slowly morphing sound with LFO on filter.
pub fn apply_evolving_texture_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Saw;
    s.an1x.osc2_wave = crate::state::An1xWave::Triangle;
    s.an1x.osc1_level = 0.6;
    s.an1x.osc2_level = 0.8;
    s.an1x.osc2_detune = 0.54;
    s.an1x.filter_cutoff = 0.5;
    s.an1x.filter_resonance = 0.35;
    s.an1x.filter_env_amount = 0.6;
    s.an1x.filter_attack = 0.3;
    s.an1x.filter_decay = 0.7;
    s.an1x.filter_sustain = 0.3;
    s.an1x.filter_release = 0.7;
    s.an1x.amp_attack = 0.5;
    s.an1x.amp_decay = 0.6;
    s.an1x.amp_sustain = 0.55;
    s.an1x.amp_release = 0.75;
    s.an1x.lfo_rate = 0.06;
    s.an1x.lfo_depth = 0.25;
    s.an1x.lfo_target = crate::state::An1xLfoTarget::FilterCutoff;
    s.an1x.lfo_delay = 0.5;
    s.an1x.drift = 0.2;
    s.an1x.volume = 0.65;
    s
}

/// Glass Pad preset — bright, crystalline, shimmering pad.
pub fn apply_glass_pad_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Triangle;
    s.an1x.osc2_wave = crate::state::An1xWave::Sine;
    s.an1x.osc1_level = 0.9;
    s.an1x.osc2_level = 0.5;
    s.an1x.osc2_detune = 0.53;
    s.an1x.osc2_octave = 1;
    s.an1x.filter_cutoff = 0.7;
    s.an1x.filter_resonance = 0.4;
    s.an1x.filter_env_amount = 0.62;
    s.an1x.filter_attack = 0.2;
    s.an1x.filter_decay = 0.5;
    s.an1x.filter_sustain = 0.5;
    s.an1x.filter_release = 0.65;
    s.an1x.amp_attack = 0.35;
    s.an1x.amp_decay = 0.45;
    s.an1x.amp_sustain = 0.6;
    s.an1x.amp_release = 0.8;
    s.an1x.hard_sync = true;
    s.an1x.drift = 0.05;
    s.an1x.volume = 0.6;
    s
}

/// Sub Drone preset — deep, sustained, barely audible movement.
pub fn apply_sub_drone_preset(state: AppState) -> AppState {
    let mut s = state;
    s.an1x.enabled = true;
    s.an1x.osc1_wave = crate::state::An1xWave::Sine;
    s.an1x.osc2_wave = crate::state::An1xWave::Triangle;
    s.an1x.osc1_level = 0.9;
    s.an1x.osc2_level = 0.4;
    s.an1x.osc2_detune = 0.505;
    s.an1x.osc2_octave = -1;
    s.an1x.sub_level = 0.6;
    s.an1x.filter_cutoff = 0.2;
    s.an1x.filter_resonance = 0.1;
    s.an1x.filter_env_amount = 0.5;
    s.an1x.filter_attack = 0.6;
    s.an1x.filter_decay = 0.8;
    s.an1x.filter_sustain = 0.6;
    s.an1x.filter_release = 0.9;
    s.an1x.amp_attack = 0.7;
    s.an1x.amp_decay = 0.7;
    s.an1x.amp_sustain = 0.8;
    s.an1x.amp_release = 0.95;
    s.an1x.lfo_rate = 0.03;
    s.an1x.lfo_depth = 0.08;
    s.an1x.lfo_target = crate::state::An1xLfoTarget::Pitch;
    s.an1x.drift = 0.25;
    s.an1x.volume = 0.7;
    s
}

// ─── LLM state update — see llm_apply.rs ─────────────────────────────────────

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

/// Spawn an LLM agent: add rack module, create agent state, wire control cables.
///
/// Empty scope wires to all controllable modules; non-empty scope wires only
/// to modules whose kind matches one of the scope strings.
/// Returns `(new_state, agent_module_id)`.
pub fn spawn_agent(
    state: AppState,
    persona: &str,
    scope: &[String],
    role: AgentRole,
    model: Option<String>,
) -> (AppState, u32) {
    let mut s = state;
    let id = s.rack.add_module(ModuleKind::LlmAgent);

    let mut agent = LlmAgentState::from_singleton(id, &s.llm);
    agent.persona_name = persona.to_string();
    agent.scope = scope.to_vec();
    agent.role = role;
    if let Some(m) = model {
        agent.model_path = Some(m);
    }
    s.llm_agents.push(agent);

    // Wire control cables
    let targets: Vec<u32> = if scope.is_empty() {
        s.rack
            .modules
            .iter()
            .filter(|m| {
                !matches!(
                    m.kind,
                    ModuleKind::MasterOutput | ModuleKind::LlmAgent | ModuleKind::LlmConsole
                )
            })
            .map(|m| m.id)
            .collect()
    } else {
        s.rack
            .modules
            .iter()
            .filter(|m| scope.iter().any(|sc| rack_kind_name_matches(m.kind, sc)))
            .map(|m| m.id)
            .collect()
    };
    for tid in &targets {
        s.rack.connect_control(id, *tid);
    }

    (s, id)
}

/// After an agent is spawned, optionally apply a conversation-mode override
/// and add + wire a NeuTts module if TTS is requested.  Shared by the
/// HTTP `/api/rack/agent` handler and the LLM `spawn_agent` action so both
/// paths produce identical results.  No-op if `agent_id` doesn't exist.
pub fn apply_agent_mode_and_tts(
    state: AppState,
    agent_id: u32,
    mode: Option<&str>,
    tts: bool,
) -> AppState {
    let mut s = state;
    if let Some(mode_str) = mode {
        use crate::state::ConversationMode;
        let cm = match mode_str.to_ascii_lowercase().as_str() {
            "off" => Some(ConversationMode::Off),
            "producer" => Some(ConversationMode::Producer),
            "dj" => Some(ConversationMode::Dj),
            "mc" => Some(ConversationMode::Mc),
            _ => None,
        };
        if let Some(cm) = cm
            && let Some(a) = s.llm_agents.iter_mut().find(|a| a.id == agent_id)
        {
            a.conversation_mode = cm;
        }
    }
    if tts {
        let tts_id = s.rack.add_module(crate::state::ModuleKind::NeuTts);
        s.tts_modules
            .push(crate::state::TtsModuleState::new(tts_id));
        s.rack.connect_control(agent_id, tts_id);
        // Scroll the UI to the new TTS module so the viewer (or recording)
        // sees where the MC/DJ voice comes from.
        s.scroll_target = Some("tts".to_string());
    }
    s
}

/// Format an LLM response for display in the log.
///
/// Returns the user-visible summary text:
/// - `ConversationMode::Off` → just the changed keys ("updated bass, fx")
/// - Otherwise → the `_comment` field if present, else keys
/// - If no param update → returns the raw text unchanged
pub fn format_llm_display(
    param_update: Option<&serde_json::Value>,
    text: &str,
    conversation_mode: &ConversationMode,
) -> String {
    match param_update {
        Some(update) => {
            let keys: Vec<&str> = update
                .as_object()
                .map(|o| {
                    o.keys()
                        .filter(|k| *k != "_comment")
                        .map(|k| k.as_str())
                        .collect()
                })
                .unwrap_or_default();
            if *conversation_mode == ConversationMode::Off {
                format!("updated {}", keys.join(", "))
            } else if let Some(comment) = update.get("_comment").and_then(|v| v.as_str()) {
                comment.to_string()
            } else {
                format!("updated {}", keys.join(", "))
            }
        }
        None => text.to_string(),
    }
}

/// Record a style observation for all agents (e.g. "user raised reverb_mix to 0.8").
/// Caps at STYLE_OBS_MAX. Deduplicates by param path prefix.
pub fn observe_user_edit(state: AppState, param_path: &str, value: f32) -> AppState {
    let mut s = state;
    let obs = if value > 0.7 {
        format!("user prefers high {} ({:.1})", param_path, value)
    } else if value < 0.3 {
        format!("user prefers low {} ({:.1})", param_path, value)
    } else {
        return s; // mid-range edits aren't interesting
    };
    for agent in &mut s.llm_agents {
        // Remove old observation for the same param
        agent.style_observations.retain(|o| !o.contains(param_path));
        agent.style_observations.push(obs.clone());
        if agent.style_observations.len() > super::STYLE_OBS_MAX {
            agent
                .style_observations
                .drain(..agent.style_observations.len() - super::STYLE_OBS_MAX);
        }
    }
    s
}

/// Propagate a style change to all agents whose style is not locked.
pub fn propagate_style(state: AppState, style_id: &str) -> AppState {
    let mut s = state;
    for agent in &mut s.llm_agents {
        if !agent.style_locked {
            agent.active_style = Some(style_id.to_string());
        }
    }
    s
}

/// Apply a pattern's `pattern_style` tag on chain advance.  When the song
/// chain walks into a slot whose loaded `SequencerState.pattern_style` is
/// `Some(id)`, this sets the global `llm.active_style` and propagates it
/// to all unlocked agents — the core of per-chain style transitions.
/// `None` leaves the active style unchanged.
pub fn apply_pattern_style_on_advance(state: AppState, style: Option<&str>) -> AppState {
    match style {
        Some(id) => {
            let mut s = state;
            s.llm.active_style = Some(id.to_string());
            propagate_style(s, id)
        }
        None => state,
    }
}

/// Propagate a seed change to all agents whose seed is not locked.  Mirrors
/// `propagate_style` — `seed = -1` means "random each call".
pub fn propagate_seed(state: AppState, seed: i64) -> AppState {
    let mut s = state;
    s.llm.seed = seed;
    for agent in &mut s.llm_agents {
        if !agent.seed_locked {
            agent.seed = seed;
        }
    }
    s
}

/// Push a memory snippet to an agent's persistent memory, capping at AGENT_MEMORY_MAX.
pub fn push_agent_memory(state: AppState, agent_id: u32, snippet: String) -> AppState {
    let mut s = state;
    if let Some(agent) = s.llm_agents.iter_mut().find(|a| a.id == agent_id) {
        agent.memory.push(snippet);
        if agent.memory.len() > super::AGENT_MEMORY_MAX {
            agent
                .memory
                .drain(..agent.memory.len() - super::AGENT_MEMORY_MAX);
        }
    }
    s
}
