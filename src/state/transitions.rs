// ─── state/transitions.rs ────────────────────────────────────────────────────
// Pure state transition helpers extracted to keep state/mod.rs under 1000 lines.

use super::{AppState, DrumVoice, FilterMode, MAX_STEPS, Waveform};

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
